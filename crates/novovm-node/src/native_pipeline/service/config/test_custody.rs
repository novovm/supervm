//! Isolated test-only custody fixture shared by original process/startup tests.
//! Never uses or changes any deployment credential. The Linux environment
//! change is serialized for the entire guard lifetime and restored on unwind.
use anyhow::Result;
use std::path::Path;

/// Actual AOEM ML-DSA validator and separately generated transport identity.
/// No plaintext validator format or test signature implementation is exposed.
pub(crate) struct TestNodeKeys {
    pub(crate) signer:
        std::sync::Arc<crate::native_pipeline::consensus::crypto::AoemConsensusSigner>,
    pub(crate) validator_path: std::path::PathBuf,
    pub(crate) transport: ed25519_dalek::SigningKey,
    pub(crate) transport_path: std::path::PathBuf,
}

/// The caller keeps TestCredentialGuard alive while creating and loading
/// protected custody; original child processes receive its directory explicitly.
pub(crate) fn generate_node_keys(
    root: &Path,
    chain: u64,
    index: usize,
    crypto: &crate::native_pipeline::consensus::crypto::AoemConsensusCrypto,
) -> Result<TestNodeKeys> {
    use novovm_relay::purpose_keys::{self, PurposeV1};
    let validator_path = root.join(format!("test-validator-{index}.sealed"));
    let transport_path = root.join(format!("test-transport-{index}.sealed"));
    purpose_keys::generate_validator(
        &validator_path,
        chain,
        || crypto.generate_test_key(),
        |public, secret, challenge| crypto.check_key_pair(public, secret, challenge),
    )?;
    let material =
        purpose_keys::load_validator(&validator_path, chain, |public, secret, challenge| {
            crypto.check_key_pair(public, secret, challenge)
        })?;
    purpose_keys::generate(&transport_path, PurposeV1::Transport, chain)?;
    Ok(TestNodeKeys {
        signer: crypto.signer(material)?,
        validator_path,
        transport: purpose_keys::load(&transport_path, PurposeV1::Transport, chain)?,
        transport_path,
    })
}

#[cfg(target_os = "linux")]
static ENVIRONMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) struct TestCredentialGuard {
    #[cfg(target_os = "linux")]
    directory: std::path::PathBuf,
    #[cfg(target_os = "linux")]
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl TestCredentialGuard {
    pub(crate) fn new(root: &Path) -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            use anyhow::{ensure, Context};
            use rand::RngCore;
            use std::{
                io::Write,
                os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
            };
            let lock = ENVIRONMENT
                .lock()
                .map_err(|_| anyhow::anyhow!("fixture credential lock poisoned"))?;
            ensure!(
                std::env::var_os("CREDENTIALS_DIRECTORY").is_none(),
                "node fixtures must not borrow an inherited service credential"
            );
            let root = root.canonicalize().context("resolve isolated test root")?;
            ensure!(
                root.is_dir() && !std::fs::symlink_metadata(&root)?.file_type().is_symlink(),
                "isolated test root must be a regular directory"
            );
            // `root` was just uniquely created by the calling original test.
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
            let directory = root.join("test-only-node-owner-credential");
            std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
            let mut secret = zeroize::Zeroizing::new([0u8; 32]);
            rand::rngs::OsRng.try_fill_bytes(secret.as_mut())?;
            ensure!(*secret != [0; 32], "empty fixture wrapping key");
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(directory.join("owner-key"))?;
            file.write_all(secret.as_ref())?;
            file.sync_all()?;
            std::env::set_var("CREDENTIALS_DIRECTORY", &directory);
            Ok(Self {
                directory,
                _lock: lock,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = root;
            Ok(Self {})
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
}

impl Drop for TestCredentialGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        std::env::remove_var("CREDENTIALS_DIRECTORY");
    }
}
