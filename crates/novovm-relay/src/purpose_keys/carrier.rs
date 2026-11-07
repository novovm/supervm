//! Carrier-only random seed in the original purpose envelope and OS custody.
//! It is independently generated, never derived from a wallet/signing secret.
use super::*;
use rand::RngCore;
const CARRIER_SUITE: u16 = 1001;

pub fn create(path: &Path, chain_id: u64, native_public: &[u8]) -> Result<()> {
    validate_creation(path, chain_id)?;
    validate_owner(native_public)?;
    let mut seed = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.try_fill_bytes(seed.as_mut())?;
    ensure!(*seed != [0; 32], "empty carrier seed");
    let bytes = encode(
        PurposeV1::Transport,
        CARRIER_SUITE,
        chain_id,
        native_public,
        seed.as_ref(),
    );
    create_protected(path, &bytes)?;
    ensure!(
        *read(path, chain_id, native_public)? == *seed,
        "carrier custody readback differs"
    );
    Ok(())
}
pub fn read(path: &Path, chain_id: u64, native_public: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    validate_owner(native_public)?;
    let bytes = read_stable(path, chain_id)?;
    let (owner, secret) = decode(
        &bytes,
        PurposeV1::Transport,
        CARRIER_SUITE,
        chain_id,
        VALIDATOR_PUBLIC_BYTES,
        32,
    )?;
    ensure!(
        owner == native_public,
        "carrier belongs to another native identity"
    );
    let seed = Zeroizing::new(<[u8; 32]>::try_from(secret)?);
    ensure!(*seed != [0; 32], "empty carrier seed");
    Ok(seed)
}
fn validate_owner(key: &[u8]) -> Result<()> {
    ensure!(
        key.len() == VALIDATOR_PUBLIC_BYTES && key.iter().any(|b| *b != 0),
        "native carrier owner key shape"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn carrier_suite_cannot_be_reinterpreted_as_native_authority() {
        let bytes = encode(
            PurposeV1::Transport,
            CARRIER_SUITE,
            91,
            &[4; 1952],
            &[5; 32],
        );
        assert!(decode(&bytes, PurposeV1::Transport, CARRIER_SUITE, 91, 1952, 32).is_ok());
        for purpose in [
            PurposeV1::Transport,
            PurposeV1::Validator,
            PurposeV1::Identity,
            PurposeV1::AssetManagement,
        ] {
            assert!(decode(&bytes, purpose, MLDSA65, 91, 1952, 4032).is_err());
        }
        assert!(decode(&bytes, PurposeV1::Transport, CARRIER_SUITE, 92, 1952, 32).is_err());
    }
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn protected_carrier_reopens_exact_seed_but_refuses_replacement_owner_and_domain() -> Result<()>
    {
        #[cfg(target_os = "linux")]
        crate::linux_custody::prepare_test_credential()?;
        let root = std::env::temp_dir().join(format!(
            "novovm-carrier-custody-{:016x}",
            rand::random::<u64>()
        ));
        #[cfg(windows)]
        std::fs::create_dir(&root)?;
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().mode(0o700).create(&root)?;
        }
        let path = root.join("carrier");
        let owner = [4; 1952];
        create(&path, 91, &owner)?;
        let seed = read(&path, 91, &owner)?;
        assert!(*read(&path, 91, &owner)? == *seed);
        assert!(create(&path, 91, &owner).is_err());
        assert!(read(&path, 92, &owner).is_err());
        assert!(read(&path, 91, &[6; 1952]).is_err());
        assert!(read_mldsa_material(&path, PurposeV1::Transport, 91).is_err());
        Ok(())
    }
}
