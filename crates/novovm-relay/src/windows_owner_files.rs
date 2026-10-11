//! Bounded private owner files; deployment ACLs must exclude untrusted writers.
use anyhow::{ensure, Result};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub fn check_path(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "absolute private path required");
    for part in path.ancestors() {
        if let Ok(meta) = std::fs::symlink_metadata(part) {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                !meta.file_type().is_symlink() && meta.file_attributes() & 0x400 == 0,
                "managed path must not use a reparse point"
            );
        }
    }
    Ok(())
}
pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    check_path(path)?;
    let mut data = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= limit, "owner file size");
    Ok(data)
}
pub fn create(path: &Path, data: &[u8]) -> Result<()> {
    check_path(path)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}
pub fn atomic(path: &Path, data: &[u8]) -> Result<()> {
    check_path(path)?;
    let stage = path.with_extension("pending");
    check_path(&stage)?;
    // Only public proposals, already-journaled signed outputs, or status use
    // this helper. It is never used for keys or an authoritative journal.
    if stage.try_exists()? {
        std::fs::remove_file(&stage)?;
    }
    create(&stage, data)?;
    std::fs::rename(stage, path)?;
    Ok(())
}
