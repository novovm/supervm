//! Linux adapter for the existing single-owner files. No shared writer or symlinks.
use anyhow::{ensure, Context, Result};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path},
};

pub fn check_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute() && !path.components().any(|p| matches!(p, Component::ParentDir)),
        "absolute non-traversing owner path required"
    );
    for part in path.ancestors() {
        match std::fs::symlink_metadata(part) {
            Ok(meta) => ensure!(
                !meta.file_type().is_symlink(),
                "managed path must not use a symlink"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
pub fn check_private_root(path: &Path) -> Result<()> {
    check_path(path)?;
    let meta = std::fs::symlink_metadata(path)?;
    // SAFETY: geteuid has no preconditions and does not access Rust memory.
    ensure!(
        meta.is_dir() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o777 == 0o700,
        "owner directory must be owned by this user and mode 0700"
    );
    Ok(())
}
pub fn check_private_file(path: &Path) -> Result<()> {
    check_path(path)?;
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o077 == 0,
        "owner file must be private and owned by this user"
    );
    Ok(())
}
pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    check_path(path)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    ensure!(file.metadata()?.is_file(), "regular owner file required");
    let mut data = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut data)?;
    ensure!(data.len() <= limit, "owner file size");
    Ok(data)
}
pub fn create(path: &Path, data: &[u8]) -> Result<()> {
    check_path(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    sync_parent(path)
}
pub fn sync_parent(path: &Path) -> Result<()> {
    File::open(path.parent().context("owner file parent")?)?.sync_all()?;
    Ok(())
}
pub fn atomic(path: &Path, data: &[u8]) -> Result<()> {
    check_path(path)?;
    let stage = path.with_extension(format!("{:016x}.pending", rand::random::<u64>()));
    create(&stage, data)?;
    if let Err(error) = std::fs::rename(&stage, path) {
        let _ = std::fs::remove_file(&stage);
        return Err(error.into());
    }
    sync_parent(path)
}
