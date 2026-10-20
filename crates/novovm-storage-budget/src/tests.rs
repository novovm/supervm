use super::*;
use rusqlite::Connection;

fn db(path: &Path) -> Connection {
    let c = Connection::open(path).unwrap();
    c.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE IF NOT EXISTS items(id INTEGER PRIMARY KEY, value BLOB);").unwrap();
    c
}
fn size_tree(root: &Path) -> u64 {
    let mut files = HashSet::new();
    walk(root, &mut files).unwrap();
    files.iter().map(|p| fs::metadata(p).unwrap().len()).sum()
}

#[test]
fn unlimited_inventory_includes_unopened_databases_indexes_and_plain_files() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.db");
    let b = dir.path().join("b.db");
    {
        let c = db(&a);
        c.execute("INSERT INTO items(value) VALUES (zeroblob(200000))", [])
            .unwrap();
        c.execute("CREATE INDEX idx ON items(value)", []).unwrap();
    }
    {
        let c = db(&b);
        c.execute("INSERT INTO items(value) VALUES (zeroblob(100000))", [])
            .unwrap();
    }
    fs::write(dir.path().join("cache"), vec![9; 50_000]).unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    let before = budget.snapshot().unwrap();
    assert_eq!(before.limit_bytes, None);
    assert_eq!(before.used_bytes, size_tree(dir.path()));
    let c = Connection::open(a).unwrap();
    c.execute("INSERT INTO items(value) VALUES (zeroblob(200000))", [])
        .unwrap();
    let after = budget.snapshot().unwrap();
    assert!(after.used_bytes > before.used_bytes);
    assert_eq!(after.used_bytes, size_tree(dir.path()));
    assert!(!after.blocked);
}

#[test]
fn finite_limit_is_aggregate_and_failed_write_rolls_back() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.db");
    let b = dir.path().join("b.db");
    let ca = db(&a);
    let cb = db(&b);
    drop((ca, cb));
    let ballast = dir.path().join("retained-media");
    File::create(&ballast)
        .unwrap()
        .set_len(MIN_LIMIT_BYTES - headroom(Some(MIN_LIMIT_BYTES)) - 1_500_000)
        .unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    let ca = Connection::open(&a).unwrap();
    let cb = Connection::open(&b).unwrap();
    ca.execute("INSERT INTO items(value) VALUES (zeroblob(1000000))", [])
        .unwrap();
    let failed = cb
        .execute("INSERT INTO items(value) VALUES (zeroblob(1000000))", [])
        .unwrap_err();
    assert_eq!(
        failed.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(
        cb.query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        ca.query_row("SELECT length(value) FROM items", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1_000_000
    );
    assert!(budget.snapshot().unwrap().used_bytes <= MIN_LIMIT_BYTES);
    budget.set_limit(None).unwrap();
    cb.execute("INSERT INTO items(value) VALUES (zeroblob(1000000))", [])
        .unwrap();
}

#[test]
fn simultaneous_independent_databases_cannot_each_spend_the_total() {
    let dir = tempfile::tempdir().unwrap();
    let paths = [dir.path().join("first.db"), dir.path().join("second.db")];
    for path in &paths {
        drop(db(path));
    }
    File::create(dir.path().join("media"))
        .unwrap()
        .set_len(MIN_LIMIT_BYTES - headroom(Some(MIN_LIMIT_BYTES)) - 1_500_000)
        .unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    let gate = Arc::new(std::sync::Barrier::new(2));
    let tasks = paths
        .into_iter()
        .map(|path| {
            let gate = gate.clone();
            std::thread::spawn(move || {
                let c = Connection::open(path).unwrap();
                gate.wait();
                c.execute("INSERT INTO items(value) VALUES (zeroblob(1000000))", [])
                    .is_ok()
            })
        })
        .collect::<Vec<_>>();
    let successes = tasks
        .into_iter()
        .map(|t| usize::from(t.join().unwrap()))
        .sum::<usize>();
    assert!(
        successes <= 1,
        "both writers must not each spend an independent quota"
    );
    assert!(budget.snapshot().unwrap().used_bytes <= MIN_LIMIT_BYTES);
}

#[test]
fn over_limit_owner_can_delete_plain_cache_then_resume() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache");
    File::create(&path)
        .unwrap()
        .set_len(MIN_LIMIT_BYTES + 4096)
        .unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    assert!(budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap().blocked);
    let guard = reserve_for_paths(std::slice::from_ref(&path), 0).unwrap();
    fs::remove_file(&path).unwrap();
    guard.finish().unwrap();
    assert!(!budget.snapshot().unwrap().blocked);
}

#[test]
fn shrinking_preserves_existing_rows_and_bounded_cleanup_journals() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    {
        let c = db(&path);
        c.execute("INSERT INTO items(value) VALUES (zeroblob(10000))", [])
            .unwrap();
    }
    File::create(dir.path().join("media"))
        .unwrap()
        .set_len(MIN_LIMIT_BYTES + 1_000_000)
        .unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    let before = budget.snapshot().unwrap().used_bytes;
    let snapshot = budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    assert!(snapshot.over_limit && snapshot.blocked);
    let c = Connection::open(&path).unwrap();
    // In-place clock/status update and expiry/consume remain possible; their
    // rollback journal is bounded and does not require new payload allocation.
    c.execute("UPDATE items SET value=zeroblob(10000)", [])
        .unwrap();
    c.execute("DELETE FROM items", []).unwrap();
    assert_eq!(
        c.query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(c
        .execute("INSERT INTO items(value) VALUES (zeroblob(200000))", [])
        .is_err());
    assert!(budget.snapshot().unwrap().used_bytes <= before + headroom(Some(MIN_LIMIT_BYTES)));
    budget.set_limit(None).unwrap();
    c.execute("INSERT INTO items(value) VALUES (zeroblob(200000))", [])
        .unwrap();
}

#[test]
fn external_atomic_file_reservations_share_the_database_budget() {
    let dir = tempfile::tempdir().unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    let path = dir.path().join("prefs.new");
    let hold = reserve_for_paths(std::slice::from_ref(&path), 400 * 1024 * 1024).unwrap();
    let p = path.clone();
    let concurrent =
        std::thread::spawn(move || reserve_for_paths(&[p], 400 * 1024 * 1024).is_err());
    assert!(concurrent.join().unwrap());
    fs::write(&path, [3; 1024]).unwrap();
    hold.finish().unwrap();
    let state = budget.snapshot().unwrap();
    assert_eq!(state.reserved_bytes, 0);
    assert_eq!(state.used_bytes, size_tree(dir.path()));
    let revision = capacity_revision();
    let release = reserve_for_paths(std::slice::from_ref(&path), 0).unwrap();
    fs::remove_file(&path).unwrap();
    release.finish().unwrap();
    assert!(capacity_revision() > revision);
}

#[test]
fn policy_minimum_persistence_and_torn_update() {
    let dir = tempfile::tempdir().unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    assert!(budget.set_limit(Some(MIN_LIMIT_BYTES - 1)).is_err());
    budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    assert_eq!(load_policy(dir.path()).unwrap().0, Some(MIN_LIMIT_BYTES));
    budget.set_limit(None).unwrap();
    assert_eq!(load_policy(dir.path()).unwrap().0, None);
    // Generation 2 is slot zero; damage it as a torn write would, then old
    // generation 1 remains available instead of silently resetting unlimited.
    let mut f = OpenOptions::new()
        .write(true)
        .open(dir.path().join(POLICY_FILE))
        .unwrap();
    f.write_all(&[0; 20]).unwrap();
    f.sync_all().unwrap();
    assert_eq!(load_policy(dir.path()).unwrap().0, Some(MIN_LIMIT_BYTES));
}

#[test]
fn unrelated_database_is_not_limited_and_wal_is_counted() {
    let dir = tempfile::tempdir().unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let c = db(&outside.path().join("outside.db"));
    c.execute("INSERT INTO items(value) VALUES (zeroblob(1000000))", [])
        .unwrap();
    let c = db(&dir.path().join("inside.db"));
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    c.execute("INSERT INTO items(value) VALUES (zeroblob(100000))", [])
        .unwrap();
    assert_eq!(budget.snapshot().unwrap().used_bytes, size_tree(dir.path()));
    drop(c);
    assert_eq!(budget.snapshot().unwrap().used_bytes, size_tree(dir.path()));
}

#[cfg(windows)]
#[test]
fn windows_ascii_case_variants_use_the_same_quota() {
    let dir = tempfile::tempdir().unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    let path = dir.path().join("MixedCase.db");
    {
        let c = db(&path);
        c.execute("INSERT INTO items(value) VALUES (zeroblob(40000))", [])
            .unwrap();
    }
    let before = budget.snapshot().unwrap().used_bytes;
    let variant = PathBuf::from(path.to_string_lossy().to_ascii_uppercase());
    let c = Connection::open(&variant).unwrap();
    c.execute("INSERT INTO items(value) VALUES (zeroblob(40000))", [])
        .unwrap();
    assert!(budget.snapshot().unwrap().used_bytes > before);
    assert_eq!(budget.snapshot().unwrap().used_bytes, size_tree(dir.path()));
    let guard = reserve_for_paths(&[variant], 4096).unwrap();
    assert_eq!(budget.snapshot().unwrap().reserved_bytes, 4096);
    guard.finish().unwrap();
}

#[cfg(windows)]
#[test]
fn windows_extended_canonical_database_cannot_bypass_full_quota() {
    let dir = tempfile::tempdir().unwrap();
    let budget = DiskBudget::configure(dir.path()).unwrap();
    budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    let path = dir.path().join("identity.db");
    drop(db(&path));
    let canonical = fs::canonicalize(&path).unwrap();
    assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
    let c = Connection::open(&canonical).unwrap();
    // Both spellings must identify one existing file, without double charging.
    assert_eq!(budget.snapshot().unwrap().used_bytes, size_tree(dir.path()));
    let status = budget.snapshot().unwrap();
    let available = MIN_LIMIT_BYTES - status.maintenance_bytes - status.used_bytes;
    let held = reserve_for_paths(&[dir.path().join("other-writer.new")], available).unwrap();
    let error = c
        .execute("INSERT INTO items(value) VALUES (zeroblob(65536))", [])
        .unwrap_err();
    assert_eq!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    held.finish().unwrap();
    c.execute("INSERT INTO items(value) VALUES (zeroblob(65536))", [])
        .unwrap();
    assert_eq!(budget.snapshot().unwrap().used_bytes, size_tree(dir.path()));
}

#[cfg(any(unix, windows))]
fn directory_alias(target: &Path, alias: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, alias).unwrap();
    #[cfg(windows)]
    {
        // Junction creation is available to ordinary Windows users and avoids
        // silently skipping the Android-root-alias regression without symlink privilege.
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(alias)
            .arg(target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn canonical_app_root_alias_reserves_new_files_and_rejects_escape() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real-app");
    let alias = temp.path().join("alias-app");
    fs::create_dir(&real).unwrap();
    directory_alias(&real, &alias);
    let budget = DiskBudget::configure(&alias).unwrap();
    budget.set_limit(Some(MIN_LIMIT_BYTES)).unwrap();
    let target = alias.join("not-yet-created").join("prefs.new");
    let hold = reserve_for_paths(std::slice::from_ref(&target), 4096).unwrap();
    assert_eq!(budget.snapshot().unwrap().reserved_bytes, 4096);
    fs::create_dir(alias.join("not-yet-created")).unwrap();
    fs::write(&target, [1; 1024]).unwrap();
    hold.finish().unwrap();
    assert_eq!(budget.snapshot().unwrap().used_bytes, size_tree(&real));
    // Android's UNIX VFS resolves parent symlinks before dispatching xOpen.
    #[cfg(unix)]
    {
        let c = db(&alias.join("history.db"));
        c.execute("INSERT INTO items(value) VALUES (zeroblob(30000))", [])
            .unwrap();
        assert_eq!(budget.snapshot().unwrap().used_bytes, size_tree(&real));
    }
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let escape = real.join("escape");
    directory_alias(&outside, &escape);
    assert!(reserve_for_paths(&[alias.join("escape").join("payload")], 4096).is_err());
    #[cfg(windows)]
    {
        fs::remove_dir(&escape).unwrap();
        fs::remove_dir(&alias).unwrap();
    }
    #[cfg(unix)]
    {
        fs::remove_file(&escape).unwrap();
        fs::remove_file(&alias).unwrap();
    }
}
