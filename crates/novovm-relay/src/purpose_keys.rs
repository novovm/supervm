//! Purpose-bound node keys over the existing OS custody adapters.
//! This module never opens a chain store, votes, publishes or starts a service.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use std::{fs::Metadata, path::Path};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"NVPK0002";
const VERSION: u16 = 2;
const HEADER: usize = 8 + 2 + 1 + 2 + 8 + 2 + 2;
const ED25519: u16 = 1;
const MLDSA65: u16 = 65;
pub const VALIDATOR_PUBLIC_BYTES: usize = 1952;
pub const VALIDATOR_SECRET_BYTES: usize = 4032;
/// This local proof of possession is deliberately not a vote or an asset grant.
pub const VALIDATOR_PAIR_CONTEXT: &[u8] = b"NOVOVM/node-purpose-key-pair/v2";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PurposeV1 {
    Validator,
    Transport,
}

impl PurposeV1 {
    fn tag(self) -> u8 {
        match self {
            Self::Validator => 1,
            Self::Transport => 2,
        }
    }
}

/// Public material only. It is not a validator-membership or launch approval.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublicIdentityV1 {
    pub schema_version: u16,
    pub purpose: PurposeV1,
    pub chain_id: u64,
    pub public_key: Vec<u8>,
    pub algorithm: &'static str,
    pub qualification: &'static str,
}

fn public(public_key: &[u8], purpose: PurposeV1, chain_id: u64) -> PublicIdentityV1 {
    PublicIdentityV1 {
        schema_version: VERSION,
        purpose,
        chain_id,
        public_key: public_key.to_vec(),
        algorithm: match purpose {
            PurposeV1::Validator => "ML-DSA-65",
            PurposeV1::Transport => "Ed25519",
        },
        qualification: "purpose_key_only_not_membership_launch_or_whole_node_pq",
    }
}

/// Never serializable, printable or cloned. Secret erasure belongs to this
/// existing custody owner even when validation or downstream startup fails.
pub struct ValidatorKeyMaterialV2 {
    pub public_key: Vec<u8>,
    pub expanded_secret_key: Zeroizing<Vec<u8>>,
}

impl ValidatorKeyMaterialV2 {
    pub fn new(public_key: Vec<u8>, expanded_secret_key: Zeroizing<Vec<u8>>) -> Result<Self> {
        let key = Self {
            public_key,
            expanded_secret_key,
        };
        validate_validator_lengths(&key)?;
        Ok(key)
    }
}

fn validate_validator_lengths(key: &ValidatorKeyMaterialV2) -> Result<()> {
    ensure!(
        key.public_key.len() == VALIDATOR_PUBLIC_BYTES
            && key.expanded_secret_key.len() == VALIDATOR_SECRET_BYTES,
        "ML-DSA-65 requires public1952/expanded-secret4032 bytes"
    );
    ensure!(
        key.public_key.iter().any(|&b| b != 0) && key.expanded_secret_key.iter().any(|&b| b != 0),
        "empty ML-DSA-65 key material"
    );
    Ok(())
}

fn validate_path(path: &Path, chain_id: u64) -> Result<()> {
    ensure!(
        chain_id != 0,
        "explicit nonzero isolated test chain id required"
    );
    ensure!(
        path.is_absolute()
            && !path
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir)),
        "absolute non-traversing purpose key path required"
    );
    #[cfg(windows)]
    crate::windows_owner_files::check_path(path)?;
    #[cfg(target_os = "linux")]
    crate::linux_owner_files::check_path(path)?;
    ensure!(
        path.parent().context("purpose key parent")?.is_dir(),
        "prepare an explicit private purpose key directory first"
    );
    #[cfg(target_os = "linux")]
    crate::linux_owner_files::check_private_root(path.parent().context("purpose key parent")?)?;
    #[cfg(any(windows, target_os = "linux"))]
    {
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        anyhow::bail!("protected node purpose custody is unavailable on this platform")
    }
}

/// Creates a transport key only. Validator preparation requires AOEM ML-DSA-65
/// through `generate_validator`; this API cannot downgrade it to Ed25519.
pub fn generate(path: &Path, purpose: PurposeV1, chain_id: u64) -> Result<PublicIdentityV1> {
    ensure!(
        purpose == PurposeV1::Transport,
        "validator requires AOEM ML-DSA-65 preparation; Ed25519 forbidden"
    );
    validate_creation(path, chain_id)?;
    let mut seed = Zeroizing::new([0u8; 32]);
    OsRng.try_fill_bytes(seed.as_mut())?;
    ensure!(*seed != [0; 32], "invalid generated key material");
    let key = SigningKey::from_bytes(&seed);
    let envelope = encode(
        purpose,
        ED25519,
        chain_id,
        &key.verifying_key().to_bytes(),
        seed.as_ref(),
    );
    create_protected(path, &envelope)?;
    let checked = inspect(path, purpose, chain_id)?;
    ensure!(
        checked == public(&key.verifying_key().to_bytes(), purpose, chain_id),
        "created purpose key readback differs"
    );
    Ok(checked)
}

/// Decrypts a transport key; validator callers must use `load_validator`.
pub fn load(path: &Path, expected_purpose: PurposeV1, chain_id: u64) -> Result<SigningKey> {
    ensure!(
        expected_purpose == PurposeV1::Transport,
        "validator requires ML-DSA-65 custody; Ed25519 forbidden"
    );
    let bytes = read_stable(path, chain_id)?;
    let (public, secret) = decode(&bytes, expected_purpose, ED25519, chain_id, 32, 32)?;
    let seed = Zeroizing::new(<[u8; 32]>::try_from(secret)?);
    ensure!(*seed != [0; 32], "invalid purpose custody key material");
    let key = SigningKey::from_bytes(&seed);
    ensure!(
        public == key.verifying_key().to_bytes(),
        "purpose custody public and private key mismatch"
    );
    Ok(key)
}

/// Key generation and pair checks must be supplied by the existing exec/AOEM
/// consumer. Custody owns format/storage, not a second crypto implementation.
/// Validation signs this owner's purpose/chain/public-bound challenge and
/// verifies it with the supplied public key before any file is created.
pub fn generate_validator(
    path: &Path,
    chain_id: u64,
    generate: impl FnOnce() -> Result<ValidatorKeyMaterialV2>,
    verify_pair: impl Fn(&[u8], &[u8], &[u8]) -> Result<()>,
) -> Result<PublicIdentityV1> {
    validate_creation(path, chain_id)?;
    let key = generate()?;
    validate_pair(&key, chain_id, &verify_pair)?;
    let envelope = encode(
        PurposeV1::Validator,
        MLDSA65,
        chain_id,
        &key.public_key,
        &key.expanded_secret_key,
    );
    create_protected(path, &envelope)?;
    let checked = inspect_validator(path, chain_id, verify_pair)?;
    ensure!(
        checked == public(&key.public_key, PurposeV1::Validator, chain_id),
        "created validator key readback differs"
    );
    Ok(checked)
}

/// Existing protected ML-DSA-65 material only. Old versions, Ed25519, wrong
/// level/purpose/domain/shape and a mismatched secret are rejected, never cast.
pub fn load_validator(
    path: &Path,
    chain_id: u64,
    verify_pair: impl Fn(&[u8], &[u8], &[u8]) -> Result<()>,
) -> Result<ValidatorKeyMaterialV2> {
    let bytes = read_stable(path, chain_id)?;
    let (public, secret) = decode(
        &bytes,
        PurposeV1::Validator,
        MLDSA65,
        chain_id,
        VALIDATOR_PUBLIC_BYTES,
        VALIDATOR_SECRET_BYTES,
    )?;
    let key = ValidatorKeyMaterialV2::new(public.to_vec(), Zeroizing::new(secret.to_vec()))?;
    validate_pair(&key, chain_id, verify_pair)?;
    Ok(key)
}

pub fn inspect_validator(
    path: &Path,
    chain_id: u64,
    verify_pair: impl Fn(&[u8], &[u8], &[u8]) -> Result<()>,
) -> Result<PublicIdentityV1> {
    let key = load_validator(path, chain_id, verify_pair)?;
    Ok(public(&key.public_key, PurposeV1::Validator, chain_id))
}

fn validate_pair(
    key: &ValidatorKeyMaterialV2,
    chain_id: u64,
    verify_pair: impl Fn(&[u8], &[u8], &[u8]) -> Result<()>,
) -> Result<()> {
    validate_validator_lengths(key)?;
    let challenge = encode(
        PurposeV1::Validator,
        MLDSA65,
        chain_id,
        &key.public_key,
        &[],
    );
    verify_pair(&key.public_key, &key.expanded_secret_key, &challenge)
        .context("AOEM ML-DSA-65 purpose key pair verification")
}

fn validate_creation(path: &Path, chain_id: u64) -> Result<()> {
    validate_path(path, chain_id)?;
    ensure_absent(path)?;
    #[cfg(target_os = "linux")]
    {
        ensure_absent(&lock_path(path))?;
        crate::linux_custody::validate_credential()?;
    }
    Ok(())
}

fn create_protected(path: &Path, envelope: &[u8]) -> Result<()> {
    #[cfg(windows)]
    crate::windows_owner_files::create(path, &crate::windows_custody::protect(envelope)?)?;
    #[cfg(target_os = "linux")]
    crate::linux_custody::create_custody(path, envelope)?;
    #[cfg(not(any(windows, target_os = "linux")))]
    anyhow::bail!("unsupported custody platform");
    #[cfg(any(windows, target_os = "linux"))]
    Ok(())
}

fn read_stable(path: &Path, chain_id: u64) -> Result<Zeroizing<Vec<u8>>> {
    validate_path(path, chain_id)?;
    let before = snapshot(path)?;
    #[cfg(target_os = "linux")]
    let lock_before = {
        let lock = lock_path(path);
        // The original owner's open_existing adapter also forbids lock
        // creation at open time, including deletion after this precheck.
        crate::linux_owner_files::check_private_file(&lock)?;
        snapshot(&lock)?
    };
    let bytes = read_protected(path)?;
    ensure!(
        same_metadata(&before, &snapshot(path)?),
        "purpose custody changed during read"
    );
    #[cfg(target_os = "linux")]
    ensure!(
        same_metadata(&lock_before, &snapshot(&lock_path(path))?),
        "purpose custody lock changed during read"
    );
    Ok(bytes)
}

/// Public-only inspection of the exact protected material the node will load.
pub fn inspect(path: &Path, purpose: PurposeV1, chain_id: u64) -> Result<PublicIdentityV1> {
    Ok(public(
        &load(path, purpose, chain_id)?.verifying_key().to_bytes(),
        purpose,
        chain_id,
    ))
}

#[cfg(windows)]
fn read_protected(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    crate::windows_custody::unprotect(&crate::windows_owner_files::read_bounded(path, 16384)?)
}
#[cfg(target_os = "linux")]
fn read_protected(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    crate::linux_custody::read_existing_custody(path)
}
#[cfg(not(any(windows, target_os = "linux")))]
fn read_protected(_path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    anyhow::bail!("unsupported custody platform")
}

fn encode(
    purpose: PurposeV1,
    suite: u16,
    chain_id: u64,
    public: &[u8],
    secret: &[u8],
) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(HEADER + public.len() + secret.len()));
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.push(purpose.tag());
    bytes.extend_from_slice(&suite.to_be_bytes());
    bytes.extend_from_slice(&chain_id.to_be_bytes());
    bytes.extend_from_slice(&(public.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&(secret.len() as u16).to_be_bytes());
    bytes.extend_from_slice(public);
    bytes.extend_from_slice(secret);
    bytes
}

fn decode(
    bytes: &[u8],
    purpose: PurposeV1,
    suite: u16,
    chain_id: u64,
    public_len: usize,
    secret_len: usize,
) -> Result<(&[u8], &[u8])> {
    ensure!(
        bytes.len() == HEADER + public_len + secret_len,
        "purpose custody envelope width"
    );
    ensure!(
        &bytes[..8] == MAGIC && bytes[8..10] == VERSION.to_be_bytes(),
        "purpose custody envelope version"
    );
    ensure!(
        bytes[10] == purpose.tag(),
        "purpose custody purpose mismatch"
    );
    ensure!(
        bytes[11..13] == suite.to_be_bytes(),
        "purpose custody algorithm mismatch; no downgrade"
    );
    ensure!(
        bytes[13..21] == chain_id.to_be_bytes(),
        "purpose custody chain mismatch"
    );
    ensure!(
        bytes[21..23] == (public_len as u16).to_be_bytes()
            && bytes[23..25] == (secret_len as u16).to_be_bytes(),
        "purpose custody algorithm key widths"
    );
    Ok((
        &bytes[HEADER..HEADER + public_len],
        &bytes[HEADER + public_len..],
    ))
}

fn snapshot(path: &Path) -> Result<Metadata> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 16384,
        "bounded regular purpose custody file required"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes() & 0x400 == 0,
            "purpose custody reparse point"
        );
    }
    #[cfg(target_os = "linux")]
    crate::linux_owner_files::check_private_file(path)?;
    Ok(meta)
}

fn ensure_absent(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => anyhow::bail!("purpose custody target already exists; refusing replacement"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn same_metadata(a: &Metadata, b: &Metadata) -> bool {
    let same = a.len() == b.len()
        && a.modified().ok() == b.modified().ok()
        && a.permissions().readonly() == b.permissions().readonly();
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        same && a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.uid() == b.uid()
            && a.gid() == b.gid()
            && a.mode() == b.mode()
            && a.nlink() == b.nlink()
    }
    #[cfg(not(target_os = "linux"))]
    {
        same
    }
}

#[cfg(target_os = "linux")]
fn lock_path(path: &Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    name.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn original_linux_custody_roundtrip_has_distinct_purposes() -> Result<()> {
        use std::os::unix::fs::DirBuilderExt;
        ensure!(
            std::env::var_os("CREDENTIALS_DIRECTORY").is_none(),
            "library test must not consume an inherited service credential"
        );
        crate::linux_custody::prepare_test_credential()?;
        let root = std::env::temp_dir().join(format!(
            "novovm-purpose-library-fixture-{:016x}",
            rand::random::<u64>()
        ));
        std::fs::DirBuilder::new().mode(0o700).create(&root)?;
        let other_transport = root.join("other-transport");
        let transport = root.join("transport");
        let a = generate(&other_transport, PurposeV1::Transport, 83)?;
        let b = generate(&transport, PurposeV1::Transport, 83)?;
        assert_ne!(a.public_key, b.public_key);
        assert_eq!(inspect(&other_transport, PurposeV1::Transport, 83)?, a);
        assert!(inspect(&transport, PurposeV1::Validator, 83).is_err());
        for path in [&other_transport, &transport] {
            std::fs::remove_file(path)?;
            std::fs::remove_file(lock_path(path))?;
        }
        std::fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn envelope_binds_exact_version_purpose_domain_and_public_material() {
        let mut bytes = encode(
            PurposeV1::Validator,
            MLDSA65,
            91,
            &[29; VALIDATOR_PUBLIC_BYTES],
            &[31; VALIDATOR_SECRET_BYTES],
        );
        let check = |bytes: &[u8]| {
            decode(
                bytes,
                PurposeV1::Validator,
                MLDSA65,
                91,
                VALIDATOR_PUBLIC_BYTES,
                VALIDATOR_SECRET_BYTES,
            )
            .map(|(pk, sk)| (pk.len(), sk.len()))
        };
        assert_eq!(check(&bytes).unwrap(), (1952, 4032));
        assert!(decode(&bytes, PurposeV1::Transport, MLDSA65, 91, 1952, 4032).is_err());
        assert!(decode(&bytes, PurposeV1::Validator, ED25519, 91, 1952, 4032).is_err());
        assert!(decode(&bytes, PurposeV1::Validator, MLDSA65, 92, 1952, 4032).is_err());
        assert!(check(&bytes[..bytes.len() - 1]).is_err());
        for index in [0, 8, 10, 11, 13, 21, 23] {
            bytes[index] ^= 1;
            assert!(check(&bytes).is_err());
            bytes[index] ^= 1;
        }
        bytes.extend_from_slice(&[0]);
        assert!(check(&bytes).is_err());
    }

    #[test]
    fn pair_check_is_mandatory_and_binds_purpose_chain_and_public() -> Result<()> {
        let material = ValidatorKeyMaterialV2::new(vec![29; 1952], Zeroizing::new(vec![31; 4032]))?;
        assert!(validate_pair(&material, 99, |_, _, _| anyhow::bail!("invalid pair")).is_err());
        let expected = encode(PurposeV1::Validator, MLDSA65, 99, &material.public_key, &[]);
        validate_pair(&material, 99, |public, secret, challenge| {
            assert_eq!(public, &[29; 1952]);
            assert_eq!(secret, &[31; 4032]);
            assert_eq!(challenge, expected.as_slice());
            Ok(())
        })?;
        assert!(
            ValidatorKeyMaterialV2::new(vec![29; 2592], Zeroizing::new(vec![31; 4896])).is_err()
        );
        assert!(ValidatorKeyMaterialV2::new(vec![0; 1952], Zeroizing::new(vec![0; 4032])).is_err());
        Ok(())
    }
}
