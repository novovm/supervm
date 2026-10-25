//! Thin custody adapter: systemd credential + existing encrypted service journal.
//! This does not define a cipher or a parallel persistence format.
use crate::linux_owner_files::{check_path, check_private_file, check_private_root, read_bounded};
use anyhow::{ensure, Context, Result};
use novovm_network::product_service_file::{ServiceLeaseFileV1, ServiceLeasePersistenceV1};
use std::{
    fs::{File, OpenOptions},
    mem::MaybeUninit,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};
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
    check_credential_source(&directory, &path)?;
    let bytes = Zeroizing::new(read_bounded(&path, 32)?);
    let key = Zeroizing::new(
        <[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| anyhow::anyhow!("owner credential must contain exactly 32 bytes"))?,
    );
    ensure!(*key != [0; 32], "empty owner credential");
    Ok(key)
}
// systemd v255 keeps root ownership and grants the service UID read access
// through a named-user ACL. Its ACL mask appears as 0440/0550 in stat(), so the
// ordinary state-file owner check is deliberately not used for this case.
fn systemd_copy_shape(directory: &Path, uid: u32, gid: u32, mode: u32, is_directory: bool) -> bool {
    let expected_modes = if is_directory {
        [0o500, 0o550]
    } else {
        [0o400, 0o440]
    };
    directory.parent() == Some(Path::new("/run/credentials"))
        && directory.file_name().is_some()
        && uid == 0
        && gid == 0
        && expected_modes.contains(&(mode & 0o7777))
}
// linux/magic.h; libc does not expose RAMFS_MAGIC on all Linux targets.
const RAMFS_MAGIC: libc::c_long = 0x858458f6_u32 as libc::c_long;
fn readonly_memory_mount(flags: libc::c_ulong, kind: libc::c_long) -> bool {
    flags & libc::ST_RDONLY != 0
        && (kind == libc::TMPFS_MAGIC as libc::c_long || kind == RAMFS_MAGIC)
}
fn check_systemd_mount(directory: &Path) -> Result<()> {
    let descriptor: File = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(directory)?;
    let mut mount = MaybeUninit::<libc::statvfs>::uninit();
    let mut filesystem = MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: live directory descriptor and correctly sized writable outputs;
    // values are only read after both syscalls succeed.
    let (mount, filesystem) = unsafe {
        if libc::fstatvfs(descriptor.as_raw_fd(), mount.as_mut_ptr()) != 0
            || libc::fstatfs(descriptor.as_raw_fd(), filesystem.as_mut_ptr()) != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        (mount.assume_init(), filesystem.assume_init())
    };
    ensure!(
        readonly_memory_mount(mount.f_flag, filesystem.f_type),
        "systemd credential directory must be a read-only memory mount"
    );
    Ok(())
}
fn check_credential_source(directory: &Path, path: &Path) -> Result<()> {
    check_path(path)?;
    let parent = std::fs::symlink_metadata(directory)?;
    let file = std::fs::symlink_metadata(path)?;
    ensure!(
        parent.is_dir() && file.is_file(),
        "regular credential file and directory required"
    );
    // A directly supplied per-user credential remains supported, with no group
    // or public access. Other users' files never qualify through this branch.
    let uid = unsafe { libc::geteuid() };
    if file.uid() == uid
        && parent.uid() == uid
        && file.mode() & 0o077 == 0
        && parent.mode() & 0o077 == 0
    {
        return check_private_file(path);
    }
    ensure!(
        systemd_copy_shape(directory, parent.uid(), parent.gid(), parent.mode(), true)
            && systemd_copy_shape(directory, file.uid(), file.gid(), file.mode(), false),
        "credential must be private per-user custody or a systemd read-only credential copy"
    );
    check_systemd_mount(directory)
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
/// Offline purpose-key inspection never provisions a missing owner lock.
/// The same source module is also compiled into service binaries, which retain
/// their existing read_custody behavior rather than using this library entry.
#[allow(dead_code)]
pub fn read_existing_custody(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    validate_path(path, true)?;
    let (_, data) = ServiceLeaseFileV1::open_existing(path, credential()?)?;
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

#[cfg(test)]
mod credential_tests {
    use super::*;
    #[test]
    fn systemd_acl_copy_exception_is_confined_to_readonly_manager_credentials() {
        let directory = Path::new("/run/credentials/novonet-mailbox.service");
        assert!(systemd_copy_shape(
            directory,
            0,
            0,
            libc::S_IFDIR | 0o550,
            true
        ));
        assert!(systemd_copy_shape(
            directory,
            0,
            0,
            libc::S_IFREG | 0o440,
            false
        ));
        for path in [
            "/etc/novonet-mailbox",
            "/tmp/credentials/unit",
            "/run/credentials/unit/child",
            "/run/credentials",
        ] {
            assert!(!systemd_copy_shape(Path::new(path), 0, 0, 0o440, false));
        }
        for (uid, gid, mode) in [
            (1, 0, 0o440),
            (0, 1, 0o440),
            (0, 0, 0o444),
            (0, 0, 0o640),
            (0, 0, 0o460),
            (0, 0, 0o2440),
        ] {
            assert!(!systemd_copy_shape(directory, uid, gid, mode, false));
        }
        assert!(!systemd_copy_shape(directory, 0, 0, 0o750, true));
        assert!(!systemd_copy_shape(directory, 0, 0, 0o555, true));
        assert!(readonly_memory_mount(
            libc::ST_RDONLY,
            libc::TMPFS_MAGIC as _
        ));
        assert!(readonly_memory_mount(libc::ST_RDONLY, RAMFS_MAGIC));
        assert!(!readonly_memory_mount(0, libc::TMPFS_MAGIC as _));
        assert!(!readonly_memory_mount(
            libc::ST_RDONLY,
            libc::EXT4_SUPER_MAGIC as _
        ));
    }
}
