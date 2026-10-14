#![cfg(feature = "iroh-transport")]
//! Same native journal entrypoints on Windows and Android; no APK data touched.
use anyhow::{ensure, Result};
use novovm_network::product_service_lease::{ServiceLeaseFileV1, ServiceLeasePersistenceV1};
use std::{path::PathBuf, process::Command};
use zeroize::Zeroizing;

#[test]
fn journal_child_lock_check() -> Result<()> {
    let Some(path) = std::env::var_os("NOVOVM_JOURNAL_PLATFORM_CHILD") else {
        return Ok(());
    };
    ensure!(
        ServiceLeaseFileV1::open(&PathBuf::from(path), Zeroizing::new([29; 32])).is_err(),
        "a second process opened the owned journal"
    );
    Ok(())
}

#[test]
fn native_lock_commit_reopen_and_wrong_material() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("journal.bin");
    let journal = ServiceLeaseFileV1::create(&path, Zeroizing::new([29; 32]), b"initial")?;
    ensure!(ServiceLeaseFileV1::open(&path, Zeroizing::new([29; 32])).is_err());
    let child = Command::new(std::env::current_exe()?)
        .args(["--exact", "journal_child_lock_check", "--nocapture"])
        .env("NOVOVM_JOURNAL_PLATFORM_CHILD", &path)
        .status()?;
    ensure!(child.success(), "child lock check failed");
    journal.commit(b"initial", b"committed")?;
    ensure!(journal.commit(b"initial", b"stale").is_err());
    drop(journal);
    ensure!(ServiceLeaseFileV1::open(&path, Zeroizing::new([30; 32])).is_err());
    let (journal, value) = ServiceLeaseFileV1::open(&path, Zeroizing::new([29; 32]))?;
    ensure!(value == b"committed");
    journal.commit(&value, b"reopened")?;
    drop(journal);
    ensure!(ServiceLeaseFileV1::open(&path, Zeroizing::new([29; 32]))?.1 == b"reopened");
    Ok(())
}
