//! Thin custody adapter: systemd credential + existing encrypted service journal.
//! This does not define a cipher or a parallel persistence format.
use crate::linux_owner_files::{check_path, check_private_file, check_private_root, read_bounded};
use anyhow::{ensure, Context, Result};
use novovm_network::product_service_lease::{ServiceLeaseFileV1, ServiceLeasePersistenceV1};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

fn credential() -> Result<Zeroizing<[u8; 32]>> {
    let directory = match std::env::var_os("CREDENTIALS_DIRECTORY") {
        Some(path) => PathBuf::from(path),
        #[cfg(test)]
        None => TEST_DIRECTORY
            .get()
            .cloned()
            .context("systemd owner credential missing")?,
        #[cfg(not(test))]
        None => anyhow::bail!("systemd owner credential missing"),
    };
    let path = directory.join("owner-key");
    check_private_file(&path)?;
    let bytes = Zeroizing::new(read_bounded(&path, 32)?);
    let key = Zeroizing::new(
        <[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| anyhow::anyhow!("owner credential must contain exactly 32 bytes"))?,
    );
    ensure!(*key != [0; 32], "empty owner credential");
    Ok(key)
}
pub fn validate_credential() -> Result<()> {
    credential().map(|_| ())
}
fn validate_path(path: &Path, exists: bool) -> Result<()> {
    check_path(path)?;
    check_private_root(path.parent().context("custody directory")?)?;
    if exists {
        check_private_file(path)?;
    }
    let lock = PathBuf::from(format!("{}.lock", path.display()));
    if lock.try_exists()? {
        check_private_file(&lock)?;
    }
    Ok(())
}
pub fn create_custody(path: &Path, data: &[u8]) -> Result<()> {
    validate_path(path, false)?;
    ensure!(!path.try_exists()?, "custody already exists");
    drop(ServiceLeaseFileV1::create(path, credential()?, data)?);
    Ok(())
}
pub fn read_custody(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    validate_path(path, true)?;
    let (_, data) = ServiceLeaseFileV1::open(path, credential()?)?;
    Ok(Zeroizing::new(data))
}
pub fn save_custody(path: &Path, data: &[u8]) -> Result<()> {
    validate_path(path, true)?;
    let (file, prior) = ServiceLeaseFileV1::open(path, credential()?)?;
    let prior = Zeroizing::new(prior);
    file.commit(&prior, data)?;
    Ok(())
}

// Standalone test binaries use isolated random fixture credentials. Production
// builds never create credentials; subprocess tests can supply their own path.
#[cfg(test)]
static TEST_DIRECTORY: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
#[cfg(test)]
pub(super) fn prepare_test_credential() -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(path) = TEST_DIRECTORY.get() {
        return Ok(path.clone());
    }
    static INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = INIT
        .lock()
        .map_err(|_| anyhow::anyhow!("fixture credential lock"))?;
    if let Some(path) = TEST_DIRECTORY.get() {
        return Ok(path.clone());
    }
    let path = std::env::temp_dir().join(format!(
        "novovm-credential-fixture-{:016x}",
        rand::random::<u64>()
    ));
    std::fs::create_dir(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    use rand::RngCore;
    let mut material = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.try_fill_bytes(material.as_mut())?;
    crate::linux_owner_files::create(&path.join("owner-key"), material.as_ref())?;
    let _ = TEST_DIRECTORY.set(path.clone());
    Ok(path)
}
