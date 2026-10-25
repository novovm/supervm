//! Purpose-bound test-node keys over the existing OS custody adapters.
//! This module never opens a chain store, votes, publishes or starts a service.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use std::{fs::Metadata, path::Path};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"NVPK0001";
const WIDTH: usize = 8 + 2 + 1 + 8 + 32 + 32;

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
    pub public_key: [u8; 32],
    pub algorithm: &'static str,
    pub qualification: &'static str,
}

fn public(key: &SigningKey, purpose: PurposeV1, chain_id: u64) -> PublicIdentityV1 {
    PublicIdentityV1 {
        schema_version: 1,
        purpose,
        chain_id,
        public_key: key.verifying_key().to_bytes(),
        algorithm: "Ed25519",
        qualification: "test_node_purpose_only_not_full_node_mldsa",
    }
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

/// Explicitly creates ONE protected key, without replacement or implicit setup.
/// Linux requires the existing per-user/systemd credential supplied by the owner.
pub fn generate(path: &Path, purpose: PurposeV1, chain_id: u64) -> Result<PublicIdentityV1> {
    validate_path(path, chain_id)?;
    ensure_absent(path)?;
    #[cfg(target_os = "linux")]
    {
        ensure_absent(&lock_path(path))?;
        crate::linux_custody::validate_credential()?;
    }
    let mut seed = Zeroizing::new([0u8; 32]);
    OsRng.try_fill_bytes(seed.as_mut())?;
    ensure!(*seed != [0; 32], "invalid generated key material");
    let key = SigningKey::from_bytes(&seed);
    let mut envelope = Zeroizing::new([0u8; WIDTH]);
    envelope[..8].copy_from_slice(MAGIC);
    envelope[8..10].copy_from_slice(&1u16.to_be_bytes());
    envelope[10] = purpose.tag();
    envelope[11..19].copy_from_slice(&chain_id.to_be_bytes());
    envelope[19..51].copy_from_slice(&key.verifying_key().to_bytes());
    envelope[51..].copy_from_slice(seed.as_ref());
    #[cfg(windows)]
    crate::windows_owner_files::create(path, &crate::windows_custody::protect(envelope.as_ref())?)?;
    #[cfg(target_os = "linux")]
    crate::linux_custody::create_custody(path, envelope.as_ref())?;
    let checked = inspect(path, purpose, chain_id)?;
    ensure!(
        checked == public(&key, purpose, chain_id),
        "created purpose key readback differs"
    );
    Ok(checked)
}

/// Decrypts and verifies an existing key; there is no raw-key fallback.
pub fn load(path: &Path, expected_purpose: PurposeV1, chain_id: u64) -> Result<SigningKey> {
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
    let key = decode(bytes.as_ref(), expected_purpose, chain_id)?;
    ensure!(
        same_metadata(&before, &snapshot(path)?),
        "purpose custody changed during read"
    );
    #[cfg(target_os = "linux")]
    ensure!(
        same_metadata(&lock_before, &snapshot(&lock_path(path))?),
        "purpose custody lock changed during read"
    );
    Ok(key)
}

/// Public-only inspection of the exact protected material the node will load.
pub fn inspect(path: &Path, purpose: PurposeV1, chain_id: u64) -> Result<PublicIdentityV1> {
    Ok(public(&load(path, purpose, chain_id)?, purpose, chain_id))
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

fn decode(bytes: &[u8], purpose: PurposeV1, chain_id: u64) -> Result<SigningKey> {
    ensure!(bytes.len() == WIDTH, "purpose custody envelope width");
    ensure!(
        &bytes[..8] == MAGIC && bytes[8..10] == 1u16.to_be_bytes(),
        "purpose custody envelope version"
    );
    ensure!(
        bytes[10] == purpose.tag(),
        "purpose custody purpose mismatch"
    );
    ensure!(
        bytes[11..19] == chain_id.to_be_bytes(),
        "purpose custody chain mismatch"
    );
    let seed = Zeroizing::new(<[u8; 32]>::try_from(&bytes[51..])?);
    ensure!(*seed != [0; 32], "invalid purpose custody key material");
    let key = SigningKey::from_bytes(&seed);
    ensure!(
        bytes[19..51] == key.verifying_key().to_bytes(),
        "purpose custody public and private key mismatch"
    );
    Ok(key)
}

fn snapshot(path: &Path) -> Result<Metadata> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "regular purpose custody file required"
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
        let validator = root.join("validator");
        let transport = root.join("transport");
        let a = generate(&validator, PurposeV1::Validator, 83)?;
        let b = generate(&transport, PurposeV1::Transport, 83)?;
        assert_ne!(a.public_key, b.public_key);
        assert_eq!(inspect(&validator, PurposeV1::Validator, 83)?, a);
        assert!(inspect(&validator, PurposeV1::Transport, 83).is_err());
        for path in [&validator, &transport] {
            std::fs::remove_file(path)?;
            std::fs::remove_file(lock_path(path))?;
        }
        std::fs::remove_dir(root)?;
        Ok(())
    }

    #[test]
    fn envelope_binds_exact_version_purpose_domain_and_public_material() {
        let key = SigningKey::from_bytes(&[29; 32]);
        let mut bytes = Zeroizing::new([0u8; WIDTH]);
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..10].copy_from_slice(&1u16.to_be_bytes());
        bytes[10] = 1;
        bytes[11..19].copy_from_slice(&91u64.to_be_bytes());
        bytes[19..51].copy_from_slice(&key.verifying_key().to_bytes());
        bytes[51..].fill(29);
        assert_eq!(
            decode(bytes.as_ref(), PurposeV1::Validator, 91)
                .unwrap()
                .verifying_key(),
            key.verifying_key()
        );
        assert!(decode(bytes.as_ref(), PurposeV1::Transport, 91).is_err());
        assert!(decode(bytes.as_ref(), PurposeV1::Validator, 92).is_err());
        assert!(decode(&bytes[..WIDTH - 1], PurposeV1::Validator, 91).is_err());
        for index in [0, 8, 19, 51] {
            bytes[index] ^= 1;
            assert!(decode(bytes.as_ref(), PurposeV1::Validator, 91).is_err());
            bytes[index] ^= 1;
        }
    }
}
