//! No DLL/backend required: incompatible or incomplete stores must fail before
//! the loader, without changing database bytes or creating a guard in an old DB.
use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Result<Self> {
        let root = std::env::temp_dir().canonicalize()?;
        let unique = format!(
            "novovm-store-guard-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = root.join(unique);
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        // Only this test's exclusively created directory; no ambient database.
        if self
            .0
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("novovm-store-guard-"))
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn config(root: &Path) -> StoreConfig {
    StoreConfig {
        library: root.join("no-aoem-library-must-load.dll"),
        database: root.join("database"),
        domain: StorageDomain {
            chain_id: 7,
            genesis_config_commitment: [8; 32],
            protocol_commitment: [9; 32],
        },
        storage: StorageConfig::default(),
        packet_budget: PacketBudget::default(),
    }
}

fn existing_fixture(config: &StoreConfig, guard: bool) -> Result<()> {
    std::fs::create_dir(&config.database)?;
    std::fs::write(config.database.join("CURRENT"), b"MANIFEST-000001\n")?;
    std::fs::write(
        config.database.join("LOG"),
        b"old database log; must not be touched",
    )?;
    std::fs::write(
        config.database.join("OLD-FORMAT"),
        b"legacy opaque ledger marker",
    )?;
    if guard {
        std::fs::write(
            config.database.join(DOMAIN_GUARD),
            config.domain.encoding()?,
        )?;
    }
    Ok(())
}

fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn collect(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            let key = path.strip_prefix(root)?.to_path_buf();
            if is_redirect(&metadata) {
                out.insert(
                    key,
                    format!("redirect:{:?}", std::fs::read_link(&path)?).into_bytes(),
                );
            } else if metadata.is_dir() {
                out.insert(key, b"directory".to_vec());
                collect(root, &path, out)?;
            } else {
                out.insert(key, std::fs::read(path)?);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    collect(root, root, &mut out)?;
    Ok(out)
}

fn refused(config: StoreConfig, mode: OpenMode) -> String {
    match CandidateStore::open(config, mode) {
        Ok(_) => panic!("invalid-library store unexpectedly opened"),
        Err(error) => format!("{error:#}"),
    }
}

fn assert_preopen_refusal(root: &Path, config: StoreConfig) -> Result<String> {
    let before = snapshot(root)?;
    let error = refused(config, OpenMode::Existing);
    assert!(error.contains("pre-open"), "{error}");
    assert!(
        !error.contains("open AOEM after"),
        "guard must run before the loader: {error}"
    );
    assert_eq!(
        snapshot(root)?,
        before,
        "read-only refusal changed database bytes"
    );
    Ok(error)
}

#[test]
fn legacy_store_without_guard_is_rejected_without_opening_or_migration() -> Result<()> {
    let directory = TestDirectory::new()?;
    let config = config(&directory.0);
    existing_fixture(&config, false)?;
    let error = assert_preopen_refusal(&directory.0, config.clone())?;
    assert!(error.contains("no implicit migration"));
    assert!(!config.database.join(DOMAIN_GUARD).exists());
    Ok(())
}

#[test]
fn guard_requires_exact_bounded_format_and_every_domain_field_before_loader() -> Result<()> {
    let directory = TestDirectory::new()?;
    let config = config(&directory.0);
    existing_fixture(&config, true)?;
    let expected = config.domain.encoding()?;
    let mut old_format = expected.clone();
    old_format[FORMAT.len() - 2] ^= 1;
    let mut wrong_chain = config.domain;
    wrong_chain.chain_id += 1;
    let mut wrong_genesis = config.domain;
    wrong_genesis.genesis_config_commitment[0] ^= 1;
    let mut wrong_protocol = config.domain;
    wrong_protocol.protocol_commitment[0] ^= 1;
    for bytes in [
        old_format,
        wrong_chain.encoding()?,
        wrong_genesis.encoding()?,
        wrong_protocol.encoding()?,
        expected[..expected.len() - 1].to_vec(),
        [expected.clone(), vec![0]].concat(),
        vec![0; 4096],
    ] {
        std::fs::write(config.database.join(DOMAIN_GUARD), bytes)?;
        assert_preopen_refusal(&directory.0, config.clone())?;
    }
    Ok(())
}

#[test]
fn new_store_guard_is_exclusive_synced_and_incomplete_creation_is_not_repaired() -> Result<()> {
    let directory = TestDirectory::new()?;
    let config = config(&directory.0);
    let error = refused(config.clone(), OpenMode::CreateNew);
    assert!(
        error.contains("open AOEM after candidate pre-open guard"),
        "{error}"
    );
    assert_eq!(
        std::fs::read(config.database.join(DOMAIN_GUARD))?,
        config.domain.encoding()?
    );
    assert!(!config.database.join("CURRENT").exists());
    let error = assert_preopen_refusal(&directory.0, config.clone())?;
    assert!(error.contains("CURRENT missing"), "{error}");
    let before = snapshot(&directory.0)?;
    let error = refused(config, OpenMode::CreateNew);
    assert!(error.contains("must not exist"));
    assert_eq!(snapshot(&directory.0)?, before);
    Ok(())
}

#[test]
fn matching_guard_still_requires_current_and_does_not_replace_database_validation() -> Result<()> {
    let directory = TestDirectory::new()?;
    let config = config(&directory.0);
    existing_fixture(&config, true)?;
    let before = snapshot(&directory.0)?;
    let error = refused(config.clone(), OpenMode::Existing);
    assert!(
        error.contains("open AOEM after candidate pre-open guard"),
        "{error}"
    );
    assert_eq!(snapshot(&directory.0)?, before);
    for bytes in [vec![], vec![0; 257]] {
        std::fs::write(config.database.join("CURRENT"), bytes)?;
        assert_preopen_refusal(&directory.0, config.clone())?;
    }
    std::fs::remove_file(config.database.join("CURRENT"))?;
    std::fs::create_dir(config.database.join("CURRENT"))?;
    assert_preopen_refusal(&directory.0, config)?;
    Ok(())
}

#[cfg(any(unix, windows))]
fn link_directory(target: &Path, link: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)?;
    #[cfg(windows)]
    {
        // A directory junction needs no developer-mode symlink privilege and
        // exercises the actual Windows reparse-point boundary.
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()?;
        ensure!(
            output.status.success(),
            "create isolated fixture junction: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
#[cfg(any(unix, windows))]
fn database_or_ancestor_redirect_cannot_bypass_guard() -> Result<()> {
    let directory = TestDirectory::new()?;
    let config = config(&directory.0);
    existing_fixture(&config, true)?;
    let alias = directory.0.join("database-alias");
    link_directory(&config.database, &alias)?;
    let mut linked = config.clone();
    linked.database = alias;
    assert_preopen_refusal(&directory.0, linked)?;
    let ancestor = directory.0.join("ancestor-alias");
    let target = directory.0.join("parent");
    std::fs::create_dir(&target)?;
    let mut nested = config.clone();
    nested.database = target.join("database");
    existing_fixture(&nested, true)?;
    link_directory(&target, &ancestor)?;
    nested.database = ancestor.join("database");
    assert_preopen_refusal(&directory.0, nested)?;
    Ok(())
}

#[test]
#[cfg(any(unix, windows))]
fn redirected_guard_and_current_are_rejected_without_following_them() -> Result<()> {
    for name in [DOMAIN_GUARD, "CURRENT"] {
        let directory = TestDirectory::new()?;
        let config = config(&directory.0);
        existing_fixture(&config, true)?;
        std::fs::remove_file(config.database.join(name))?;
        let target = directory.0.join("retained-target");
        std::fs::create_dir(&target)?;
        std::fs::write(target.join("must-remain"), b"unrelated retained bytes")?;
        link_directory(&target, &config.database.join(name))?;
        let error = assert_preopen_refusal(&directory.0, config)?;
        assert!(error.contains("redirected path"), "{error}");
    }
    Ok(())
}
