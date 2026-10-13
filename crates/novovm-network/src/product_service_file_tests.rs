use super::*;

fn material() -> Zeroizing<[u8; 32]> {
    Zeroizing::new([91; 32])
}

// Deliberately legacy encrypted fixtures, never installed node data.
fn legacy(path: &Path, checkpoint: &[u8], count: usize) -> Result<()> {
    let mut file = private_options().create_new(true).open(path)?;
    file.write_all(LEGACY_MAGIC)?;
    let mut tail = [0; 32];
    for _ in 0..count {
        let frame = encrypt(&cipher(material()), checkpoint, &tail)?;
        file.write_all(&(frame.len() as u32).to_le_bytes())?;
        file.write_all(&frame)?;
        tail = Sha256::digest(frame).into();
    }
    file.sync_all()?;
    Ok(())
}

#[test]
fn legacy_compaction_keeps_exact_checkpoint_and_cas_across_reopen() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("service.journal");
    let bytes = vec![7; 16 * 1024];
    legacy(&path, &bytes, 35)?;
    let (store, saved) = ServiceLeaseFileV1::open(&path, material())?;
    assert_eq!(saved, bytes);
    store.commit(&saved, &saved)?;
    assert!(std::fs::metadata(&path)?.len() < COMPACT_AT);
    assert_eq!(store.state.lock().unwrap().generation, 1);
    assert!(ServiceLeaseFileV1::open(&path, material()).is_err());
    let next = vec![8; 16 * 1024];
    store.commit(&saved, &next)?;
    drop(store);
    let (store, saved) = ServiceLeaseFileV1::open(&path, material())?;
    assert_eq!(saved, next);
    assert!(store.commit(&bytes, b"stale writer").is_err());
    assert!(store.commit(&next, b"must stay stopped").is_err());
    Ok(())
}

#[test]
fn repeated_compactions_stay_bounded_and_keep_every_acknowledged_head() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("service.journal");
    let mut previous = vec![0; 32 * 1024];
    let store = ServiceLeaseFileV1::create(&path, material(), &previous)?;
    for sequence in 1u64..=70 {
        let mut next = previous.clone();
        next[..8].copy_from_slice(&sequence.to_le_bytes());
        store.commit(&previous, &next)?;
        previous = next;
        assert!(std::fs::metadata(&path)?.len() < COMPACT_AT + MAX_FRAME as u64 + 4);
    }
    assert!(store.state.lock().unwrap().generation >= 4);
    let generation = store.state.lock().unwrap().generation;
    drop(store);
    let (store, saved) = ServiceLeaseFileV1::open(&path, material())?;
    assert_eq!(saved, previous);
    assert_eq!(store.state.lock().unwrap().generation, generation);
    Ok(())
}

#[test]
fn staged_copy_never_repairs_missing_or_corrupt_authoritative_history() -> Result<()> {
    for corrupt in [false, true] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("service.journal");
        let store = ServiceLeaseFileV1::create(&path, material(), b"revocation=1; sequence=99")?;
        drop(store);
        std::fs::copy(&path, sibling(&path, ".compact"))?;
        if corrupt {
            let mut bytes = std::fs::read(&path)?;
            // Anchor/header authentication also protects generation tampering.
            bytes[8] ^= 1;
            std::fs::write(&path, bytes)?;
        } else {
            std::fs::remove_file(&path)?;
        }
        assert!(ServiceLeaseFileV1::open(&path, material()).is_err());
        assert!(sibling(&path, ".compact").exists());
    }
    Ok(())
}

#[test]
fn failed_compaction_stops_owner_without_releasing_next_state() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("service.journal");
    let bytes = vec![9; 16 * 1024];
    legacy(&path, &bytes, 35)?;
    let (store, saved) = ServiceLeaseFileV1::open(&path, material())?;
    let staging = sibling(&path, ".compact");
    std::fs::create_dir(&staging)?; // Real filesystem publication precondition failure.
    assert!(store.commit(&saved, b"must not publish").is_err());
    std::fs::remove_dir(&staging)?;
    assert!(store.commit(&saved, b"still stopped").is_err());
    drop(store);
    let (_, recovered) = ServiceLeaseFileV1::open(&path, material())?;
    assert_eq!(recovered, saved);
    Ok(())
}

#[test]
fn compaction_child() -> Result<()> {
    let Some(path) = std::env::var_os("NOVOVM_TEST_COMPACTION_PATH") else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    let point: u8 = std::env::var("NOVOVM_TEST_COMPACTION_POINT")?.parse()?;
    if point == 0 {
        assert!(ServiceLeaseFileV1::open(&path, material()).is_err());
        return Ok(());
    }
    let (store, _) = ServiceLeaseFileV1::open(&path, material())?;
    let mut state = store.state.lock().unwrap();
    store.compact_with(&mut state, |at| {
        if at == point {
            std::process::exit(73);
        } // No Drop or graceful close.
        Ok(())
    })?;
    anyhow::bail!("crash point was not reached")
}

fn child(path: &Path, point: u8) -> Result<std::process::ExitStatus> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "product_service_lease::file::tests::compaction_child",
        ])
        .env("NOVOVM_TEST_COMPACTION_PATH", path)
        .env("NOVOVM_TEST_COMPACTION_POINT", point.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("compaction child timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn process_crashes_at_every_publication_boundary_preserve_exact_head() -> Result<()> {
    for point in 1..=6 {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("service.journal");
        let bytes = vec![point; 16 * 1024];
        legacy(&path, &bytes, 35)?;
        assert_eq!(child(&path, point)?.code(), Some(73));
        let (store, saved) = ServiceLeaseFileV1::open(&path, material())?;
        assert_eq!(saved, bytes, "point {point}");
        store.commit(&saved, &saved)?; // Recovers only uncommitted staging debris.
        let next = vec![point + 1; 16 * 1024];
        store.commit(&saved, &next)?;
        assert!(child(&path, 0)?.success()); // Second process cannot take ownership.
        drop(store);
        let (_, saved) = ServiceLeaseFileV1::open(&path, material())?;
        assert_eq!(saved, next);
    }
    Ok(())
}
