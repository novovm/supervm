//! Process-scoped ownership for the single local locator. The permanent lock
//! is never unlinked while another process could have opened the same inode.
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
};

const MAX_LOCATOR_BYTES: usize = 64 * 1024;

pub(super) struct LocalLocator {
    path: PathBuf,
    _lock: File,
}

impl LocalLocator {
    pub(super) fn replace(
        path: PathBuf,
        bytes: &[u8],
        legacy_is_live: impl FnOnce(&[u8]) -> bool,
    ) -> Result<Self> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_LOCATOR_BYTES,
            "invalid local locator size"
        );
        // Use the same absolute, long-path-safe prefix for both names passed to
        // tempfile's Windows rename operation.
        let directory = path
            .parent()
            .context("missing locator directory")?
            .canonicalize()?;
        let path = directory.join(path.file_name().context("missing locator filename")?);
        let mut lock = lock(&path)?;

        if check_regular(&path)? {
            // The marker only recognizes a locator previously published under
            // this same lock. An old-version active lease must expire first.
            let mut prior = Vec::new();
            File::open(&path)?
                .take(MAX_LOCATOR_BYTES as u64 + 1)
                .read_to_end(&mut prior)?;
            let mut marker = [0; 32];
            let ours = lock.metadata()?.len() == 32
                && lock.read_exact(&mut marker).is_ok()
                && marker == <[u8; 32]>::from(Sha256::digest(&prior));
            ensure!(
                ours || !legacy_is_live(&prior),
                "existing legacy snapshot locator still has a live lease"
            );
        }
        let mut staged = tempfile::NamedTempFile::new_in(&directory)?;
        staged.write_all(bytes)?;
        staged.as_file().sync_all()?;
        // On a process crash, the lock is released by the OS. The marker lets
        // the next owner distinguish our still-dated lease from a legacy owner.
        lock.rewind()?;
        lock.write_all(&Sha256::digest(bytes))?;
        lock.set_len(32)?;
        lock.sync_all()?;
        staged
            .persist(&path)
            .context("installing local snapshot locator")?;
        Ok(Self { path, _lock: lock })
    }
}

impl Drop for LocalLocator {
    fn drop(&mut self) {
        // Still holding the lock. A successor cannot publish before removal.
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn lock(path: &Path) -> Result<File> {
    let lock_path = path.with_extension("lock");
    check_regular(&lock_path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Hold the lock inode through shutdown, deny delete sharing.
        options.share_mode(0x1 | 0x2).custom_flags(0x0020_0000);
    }
    let lock = options.open(&lock_path)?;
    ensure!(regular(&lock.metadata()?), "invalid local locator lock");
    lock.try_lock()
        .context("another process owns the local snapshot locator")?;

    Ok(lock)
}

fn regular(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.is_file() && metadata.file_attributes() & 0x400 == 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_file() && !metadata.file_type().is_symlink()
    }
}

fn check_regular(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(regular(&meta), "invalid local locator path");
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    struct StopChild(Child);
    impl Drop for StopChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn local_locator_excludes_concurrent_writer_and_recovers_after_process_kill() -> Result<()> {
        const CHILD_DIRECTORY: &str = "NOVOVM_TEST_SNAPSHOT_LOCATOR_CRASH_DIR";
        const TEST_NAME: &str =
            "local_locator_excludes_concurrent_writer_and_recovers_after_process_kill";
        if let Some(path) = std::env::var_os(CHILD_DIRECTORY) {
            let root = PathBuf::from(path);
            let _owner = LocalLocator::replace(root.join("locator.json"), b"original", |_| false)?;
            fs::write(root.join("child.ready"), b"ready")?;
            thread::sleep(Duration::from_secs(30));
            anyhow::bail!("test parent failed to stop child");
        }
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("locator.json");
        let mut child = StopChild(
            Command::new(std::env::current_exe()?)
                .arg(TEST_NAME)
                .arg("--test-threads=1")
                .env(CHILD_DIRECTORY, temp.path())
                .stdout(Stdio::null())
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !temp.path().join("child.ready").is_file() {
            ensure!(
                child.0.try_wait()?.is_none() && Instant::now() < deadline,
                "child failed to publish"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(LocalLocator::replace(path.clone(), b"concurrent", |_| false).is_err());
        assert_eq!(fs::read(&path)?, b"original");
        child.0.kill()?;
        child.0.wait()?;
        assert_eq!(fs::read(&path)?, b"original");
        let next = LocalLocator::replace(path.clone(), b"recovered", |_| true)?;
        assert_eq!(fs::read(&path)?, b"recovered");
        drop(next);
        assert!(!path.exists());
        assert!(path.with_extension("lock").is_file());
        Ok(())
    }

    #[test]
    fn live_unmarked_locator_survives_and_expired_legacy_locator_is_replaced() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("locator.json");
        fs::write(&path, b"legacy")?;
        assert!(LocalLocator::replace(path.clone(), b"current", |v| v == b"legacy").is_err());
        assert_eq!(fs::read(&path)?, b"legacy");
        let _owner = LocalLocator::replace(path.clone(), b"current", |_| false)?;
        assert_eq!(fs::read(&path)?, b"current");
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn local_locator_publishes_in_a_long_installed_path() -> Result<()> {
        use std::os::windows::ffi::OsStrExt;

        let temp = tempfile::tempdir()?;
        let directory = temp.path().join("a".repeat(120)).join("b".repeat(120));
        fs::create_dir_all(&directory)?;
        let path = directory.join("locator.json");
        assert!(path.as_os_str().encode_wide().count() > 260);
        let owner = LocalLocator::replace(path.clone(), b"new locator", |_| false)?;
        assert_eq!(fs::read(&path)?, b"new locator");
        drop(owner);
        assert!(!path.exists());
        assert!(path.with_extension("lock").is_file());
        Ok(())
    }
}
