use novovm_network::mailbox_store::*;

#[test]
fn equal_expiry_objects_do_not_overtake_by_random_object_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("arrival.db");
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    let grant = store.create(1000, 600).unwrap();
    // Expiry is in seconds: a description and its following ICE candidates
    // commonly share one deadline despite different random encrypted IDs.
    for id in [240, 2, 1] {
        store
            .put(&grant.id, &grant.write, &[id; 32], &[id; 8], 1200, 1000)
            .unwrap();
    }
    assert_eq!(
        store
            .fetch_one(&grant.id, &grant.read, 1000)
            .unwrap()
            .unwrap()
            .id,
        [240; 32]
    );
    let cursor = store.observe(&grant.id, &grant.read, 1000).unwrap().0;
    assert_eq!(
        store
            .put(&grant.id, &grant.write, &[240; 32], &[240; 8], 1200, 1000)
            .unwrap(),
        PutResult::Duplicate
    );
    assert_eq!(
        store.observe(&grant.id, &grant.read, 1000).unwrap().0,
        cursor
    );
    drop(store);
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    assert_eq!(
        store
            .fetch(&grant.id, &grant.read, 1001)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        vec![[240; 32], [2; 32], [1; 32]]
    );
    // Keep earliest-expiry precedence, signed content/TTL, and real consume.
    store
        .put(&grant.id, &grant.write, &[250; 32], &[250; 8], 1100, 1001)
        .unwrap();
    let first = store
        .fetch_one(&grant.id, &grant.read, 1001)
        .unwrap()
        .unwrap();
    assert_eq!(first.id, [250; 32]);
    store
        .consume(&grant.id, &grant.read, &first.id, &first.digest, 1001)
        .unwrap();
    assert_eq!(
        store
            .fetch_one(&grant.id, &grant.read, 1001)
            .unwrap()
            .unwrap()
            .id,
        [240; 32]
    );
    assert!(store
        .fetch(&grant.id, &Capability::from_bytes([99; 32]).unwrap(), 1001)
        .is_err());
    assert!(store
        .fetch(&grant.id, &grant.read, 1200)
        .unwrap()
        .is_empty());
}

#[test]
fn durable_cursor_migration_commit_atomicity_and_recreation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cursor.db");
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    let grant = store.create(1000, 600).unwrap();
    store
        .put(&grant.id, &grant.write, &[2; 32], &[3; 8], 1200, 1000)
        .unwrap();
    drop(store);
    // Exact prior schema with a real committed envelope, not an empty replacement.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP TABLE mailbox_revisions; DROP TABLE mailbox_contribution; PRAGMA user_version=0;",
    )
    .unwrap();
    drop(db);
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    let (before, pending) = store.observe(&grant.id, &grant.read, 1000).unwrap();
    assert!(pending);
    assert_eq!(before.sequence, 0);
    assert_eq!(
        store.fetch(&grant.id, &grant.read, 1000).unwrap()[0].expires,
        1200
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_cursor BEFORE UPDATE ON mailbox_revisions BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(store
        .put(&grant.id, &grant.write, &[4; 32], &[5; 8], 1300, 1000)
        .is_err());
    assert_eq!(store.fetch(&grant.id, &grant.read, 1000).unwrap().len(), 1);
    assert_eq!(
        store.observe(&grant.id, &grant.read, 1000).unwrap().0,
        before
    );
    db.execute_batch("DROP TRIGGER reject_cursor;").unwrap();
    drop(db);
    store
        .put(&grant.id, &grant.write, &[4; 32], &[5; 8], 1300, 1000)
        .unwrap();
    store
        .put(&grant.id, &grant.write, &[4; 32], &[5; 8], 1300, 1000)
        .unwrap();
    let committed = store.observe(&grant.id, &grant.read, 1000).unwrap().0;
    assert_eq!(committed.sequence, 1);
    assert_eq!(committed.generation, before.generation);
    drop(store);
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    assert_eq!(
        store.observe(&grant.id, &grant.read, 1001).unwrap().0,
        committed
    );
    store.revoke(&grant.id, &grant.read, 1001).unwrap();
    assert!(matches!(
        store.observe(&grant.id, &grant.read, 1001),
        Err(StoreError::Unauthorized)
    ));
    store.sweep(2000).unwrap();
    store.register(&grant, 2000, 2600).unwrap();
    let recreated = store.observe(&grant.id, &grant.read, 2000).unwrap();
    assert_ne!(recreated.0.generation, before.generation);
    assert!(!recreated.1);
}

#[test]
fn contributor_choice_survives_restart_without_losing_retention_or_limits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contributor.db");
    let limits = Limits {
        object_bytes: 8,
        mailbox_bytes: 8,
        mailbox_objects: 2,
        total_bytes: 12,
        total_objects: 3,
        mailboxes: 2,
    };
    let mut store = MailboxStore::open_contributor(&path, limits, true).unwrap();
    assert!(store.contribution_status(1000).unwrap().enabled);
    assert!(matches!(store.create(1000, 600), Err(StoreError::Quota)));
    store.set_resources_available(true);
    let phone = store.create(1000, 600).unwrap();
    let tablet = store.create(1000, 600).unwrap();
    store
        .put(&phone.id, &phone.write, &[1; 32], &[7; 8], 1300, 1000)
        .unwrap();
    store
        .put(&tablet.id, &tablet.write, &[2; 32], &[8; 4], 1400, 1000)
        .unwrap();
    let cursor = store.observe(&phone.id, &phone.read, 1000).unwrap().0;
    store.set_contribution_enabled(false, 1001).unwrap();
    assert!(matches!(
        store.renew(&phone.id, &phone.read, 1001, 900),
        Err(StoreError::Quota)
    ));
    // Lost ACK reconciliation is not a new storage obligation.
    store.register(&phone, 1001, 1600).unwrap();
    store.renew(&phone.id, &phone.read, 1001, 60).unwrap();
    assert_eq!(
        store
            .put(&phone.id, &phone.write, &[1; 32], &[7; 8], 1300, 1001)
            .unwrap(),
        PutResult::Duplicate
    );
    drop(store);
    let mut store = MailboxStore::open_contributor(&path, limits, true).unwrap();
    let state = store.contribution_status(1002).unwrap();
    assert!(!state.enabled && !state.resources_available);
    assert_eq!(
        (
            state.retained_objects,
            state.retained_bytes,
            state.retain_until
        ),
        (2, 12, Some(1400))
    );
    assert_eq!(
        store.observe(&phone.id, &phone.read, 1002).unwrap().0,
        cursor
    );
    let row = store.fetch(&phone.id, &phone.read, 1002).unwrap().remove(0);
    assert_eq!(
        (row.id, row.expires, row.ciphertext),
        ([1; 32], 1300, vec![7; 8])
    );
    store.set_resources_available(true);
    assert!(matches!(
        store.put(&tablet.id, &tablet.write, &[3; 32], &[9], 1350, 1002),
        Err(StoreError::Quota)
    ));
    store
        .consume(&phone.id, &phone.read, &[1; 32], &row.digest, 1002)
        .unwrap();
    assert_eq!(
        store.fetch(&tablet.id, &tablet.read, 1002).unwrap().len(),
        1
    );
    store.set_contribution_enabled(true, 1003).unwrap();
    store
        .put(&tablet.id, &tablet.write, &[3; 32], &[9; 4], 1350, 1003)
        .unwrap();
    assert!(matches!(
        store.put(&tablet.id, &tablet.write, &[4; 32], &[9], 1350, 1003),
        Err(StoreError::Quota)
    ));
    assert!(matches!(
        store.put(&phone.id, &phone.write, &[1; 32], &[7; 8], 1300, 1003),
        Err(StoreError::Quota)
    ));
    store.set_resources_available(false);
    assert_eq!(
        store.fetch(&tablet.id, &tablet.read, 1003).unwrap().len(),
        2
    );
    store.set_contribution_enabled(false, 1004).unwrap();
    store.sweep(1400).unwrap();
    let state = store.contribution_status(1400).unwrap();
    assert_eq!(
        (
            state.retained_objects,
            state.retained_bytes,
            state.retain_until
        ),
        (0, 0, None)
    );
    assert!(matches!(
        store.set_contribution_enabled(true, 1399),
        Err(StoreError::ClockRollback)
    ));
    assert!(!store.contribution_status(1400).unwrap().enabled);
}

#[test]
fn contribution_migration_preserves_v1_and_missing_or_failed_policy_is_not_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    let grant = store.create(1000, 600).unwrap();
    store
        .put(&grant.id, &grant.write, &[1; 32], &[8; 8], 1300, 1000)
        .unwrap();
    let cursor = store.observe(&grant.id, &grant.read, 1000).unwrap().0;
    drop(store);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE mailbox_contribution; PRAGMA user_version=1;")
        .unwrap();
    drop(db);
    let mut store = MailboxStore::open_contributor(&path, Limits::default(), false).unwrap();
    assert!(!store.contribution_status(1000).unwrap().enabled);
    assert_eq!(
        store.observe(&grant.id, &grant.read, 1000).unwrap().0,
        cursor
    );
    let row = store.fetch(&grant.id, &grant.read, 1000).unwrap().remove(0);
    assert_eq!(
        (row.id, row.expires, row.ciphertext),
        ([1; 32], 1300, vec![8; 8])
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_choice BEFORE UPDATE ON mailbox_contribution BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(store.set_contribution_enabled(true, 1000).is_err());
    assert!(!store.contribution_status(1000).unwrap().enabled);
    drop(store);
    db.execute_batch("DROP TRIGGER fail_choice; DELETE FROM mailbox_contribution;")
        .unwrap();
    assert!(MailboxStore::open_contributor(&path, Limits::default(), true).is_err());
    assert!(MailboxStore::open(&path, Limits::default()).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM envelopes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    db.execute_batch("DROP TABLE mailbox_contribution;")
        .unwrap();
    assert!(MailboxStore::open_contributor(&path, Limits::default(), true).is_err());
}

#[test]
fn existing_empty_or_partial_store_never_silently_reenrolls() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty.db");
    std::fs::write(&empty, []).unwrap();
    assert!(MailboxStore::open(&empty, Limits::default()).is_err());
    let partial = dir.path().join("partial.db");
    let mut store = MailboxStore::open(&partial, Limits::default()).unwrap();
    store.create(100, 1000).unwrap();
    drop(store);
    let raw = rusqlite::Connection::open(&partial).unwrap();
    raw.execute_batch("DROP TABLE envelopes;").unwrap();
    drop(raw);
    assert!(MailboxStore::open(&partial, Limits::default()).is_err());
}

#[test]
fn restart_dedup_device_ack_and_ttl() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mailbox.db");
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    let phone = store.create(1000, 3600).unwrap();
    let tablet = store.create(1000, 3600).unwrap();
    // This test verifies storage semantics, not encryption. Crypto probe is separate.
    let data = [42u8; 128];
    let object = [3; 32];
    assert_eq!(
        store
            .put(&phone.id, &phone.write, &object, &data, 2000, 1000)
            .unwrap(),
        PutResult::Stored
    );
    assert_eq!(
        store
            .put(&phone.id, &phone.write, &object, &data, 2000, 1000)
            .unwrap(),
        PutResult::Duplicate
    );
    store
        .put(&tablet.id, &tablet.write, &object, &data, 2000, 1000)
        .unwrap();
    drop(store);
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    let fetched = store.fetch(&phone.id, &phone.read, 1001).unwrap();
    assert_eq!(fetched[0].ciphertext, data);
    store
        .consume(&phone.id, &phone.read, &object, &fetched[0].digest, 1001)
        .unwrap();
    store
        .consume(&phone.id, &phone.read, &object, &fetched[0].digest, 1001)
        .unwrap();
    assert!(store
        .fetch(&phone.id, &phone.read, 1001)
        .unwrap()
        .is_empty());
    assert_eq!(
        store.fetch(&tablet.id, &tablet.read, 1001).unwrap().len(),
        1
    );
    assert_eq!(
        store
            .put(&phone.id, &phone.write, &object, &data, 2000, 1001)
            .unwrap(),
        PutResult::Stored
    );
    assert_eq!(
        store.fetch(&phone.id, &phone.read, 1001).unwrap()[0].expires,
        2000
    );
    store.renew(&tablet.id, &tablet.read, 1001, 7200).unwrap();
    assert!(store
        .fetch(&tablet.id, &tablet.read, 2000)
        .unwrap()
        .is_empty());
    drop(store);
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    assert!(matches!(
        store.fetch(&tablet.id, &tablet.read, 1999),
        Err(StoreError::ClockRollback)
    ));
}

#[test]
fn capabilities_conflicts_and_resource_budgets() {
    let dir = tempfile::tempdir().unwrap();
    let limits = Limits {
        object_bytes: 8,
        mailbox_bytes: 8,
        mailbox_objects: 2,
        total_bytes: 12,
        total_objects: 3,
        mailboxes: 2,
    };
    let mut s = MailboxStore::open(dir.path().join("test.db"), limits).unwrap();
    let a = s.create(100, 1000).unwrap();
    let b = s.create(100, 1000).unwrap();
    assert!(matches!(s.create(100, 1000), Err(StoreError::Quota)));
    assert!(matches!(
        s.put(&a.id, &b.write, &[1; 32], &[1; 4], 200, 100),
        Err(StoreError::Unauthorized)
    ));
    assert!(matches!(
        s.fetch(&a.id, &a.write, 100),
        Err(StoreError::Unauthorized)
    ));
    s.put(&a.id, &a.write, &[1; 32], &[1; 8], 200, 100).unwrap();
    assert!(matches!(
        s.put(&a.id, &a.write, &[1; 32], &[2; 8], 200, 100),
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        s.put(&a.id, &a.write, &[2; 32], &[1; 1], 200, 100),
        Err(StoreError::Quota)
    ));
    assert!(matches!(
        s.put(&b.id, &b.write, &[2; 32], &[1; 5], 200, 100),
        Err(StoreError::Quota)
    ));
    assert!(matches!(
        s.put(&b.id, &b.write, &[2; 32], &[1; 9], 200, 100),
        Err(StoreError::Invalid)
    ));
    assert!(matches!(
        s.put(&b.id, &b.write, &[2; 32], &[1; 1], 2000, 100),
        Err(StoreError::Invalid)
    ));
}

#[test]
fn consumed_rows_release_quota_and_wrong_digest_cannot_delete() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = MailboxStore::open(
        dir.path().join("test.db"),
        Limits {
            mailbox_objects: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    let a = s.create(100, 1000).unwrap();
    s.put(&a.id, &a.write, &[1; 32], &[5; 32], 200, 100)
        .unwrap();
    assert!(matches!(
        s.consume(&a.id, &a.read, &[1; 32], &[0; 32], 100),
        Err(StoreError::Conflict)
    ));
    let row = s.fetch(&a.id, &a.read, 100).unwrap().remove(0);
    s.consume(&a.id, &a.read, &row.id, &row.digest, 100)
        .unwrap();
    assert_eq!(
        s.put(&a.id, &a.write, &[2; 32], &[5; 32], 250, 100)
            .unwrap(),
        PutResult::Stored
    );
    s.sweep(501).unwrap();
    assert!(s.put(&a.id, &a.write, &[2; 32], &[5; 32], 600, 501).is_ok());
}

#[test]
fn retention_v3_migrates_only_consumed_rows_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retention.db");
    let mut s = MailboxStore::open(&path, Limits::default()).unwrap();
    let a = s.create(100, 1000).unwrap();
    let b = s.create(100, 1000).unwrap();
    s.put(&a.id, &a.write, &[1; 32], &[5; 32], 600, 100)
        .unwrap();
    s.put(&b.id, &b.write, &[2; 32], &[6; 32], 700, 100)
        .unwrap();
    let cursor = s.observe(&b.id, &b.read, 100).unwrap().0;
    s.set_contribution_enabled(false, 100).unwrap();
    drop(s);
    let db = rusqlite::Connection::open(&path).unwrap();
    // Construct the exact previous v2 consumed state, preserving a second
    // device's outstanding custody and the owner's persistent opt-out.
    db.execute(
        "UPDATE envelopes SET consumed=1,payload=x'' WHERE mailbox=?1",
        [a.id],
    )
    .unwrap();
    db.execute_batch(
        "PRAGMA user_version=2;
      CREATE TRIGGER fail_cleanup BEFORE DELETE ON envelopes
      BEGIN SELECT RAISE(ABORT,'fixture'); END;",
    )
    .unwrap();
    assert!(MailboxStore::open_contributor(&path, Limits::default(), true).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM envelopes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    db.execute_batch("DROP TRIGGER fail_cleanup;").unwrap();
    let mut s = MailboxStore::open_contributor(&path, Limits::default(), true).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM envelopes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM envelopes WHERE mailbox=?1",
            [a.id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert!(!s.contribution_status(100).unwrap().enabled);
    assert_eq!(s.observe(&b.id, &b.read, 100).unwrap().0, cursor);
    let row = s.fetch(&b.id, &b.read, 100).unwrap().remove(0);
    assert_eq!(
        (row.id, row.ciphertext, row.expires),
        ([2; 32], vec![6; 32], 700)
    );
    assert!(matches!(
        s.fetch(&b.id, &b.read, 99),
        Err(StoreError::ClockRollback)
    ));
    s.sweep(700).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM envelopes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(s);
    db.execute_batch("PRAGMA user_version=99;").unwrap();
    assert!(MailboxStore::open(&path, Limits::default()).is_err());
}

#[test]
fn consume_delete_failure_preserves_custody_and_retry_removes_all_object_columns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("consume.db");
    let mut s = MailboxStore::open(&path, Limits::default()).unwrap();
    let a = s.create(100, 1000).unwrap();
    s.put(&a.id, &a.write, &[1; 32], &[7; 32], 600, 100)
        .unwrap();
    let row = s.fetch(&a.id, &a.read, 100).unwrap().remove(0);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_delete BEFORE DELETE ON envelopes
      BEGIN SELECT RAISE(ABORT,'fixture'); END;",
    )
    .unwrap();
    assert!(s
        .consume(&a.id, &a.read, &row.id, &row.digest, 101)
        .is_err());
    assert_eq!(
        s.fetch(&a.id, &a.read, 100).unwrap()[0].ciphertext,
        vec![7; 32]
    );
    db.execute_batch("DROP TRIGGER fail_delete;").unwrap();
    s.consume(&a.id, &a.read, &row.id, &row.digest, 101)
        .unwrap();
    drop(s);
    let mut s = MailboxStore::open(&path, Limits::default()).unwrap();
    s.consume(&a.id, &a.read, &row.id, &row.digest, 102)
        .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM envelopes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(s.fetch(&a.id, &a.read, 102).unwrap().is_empty());
}

#[test]
fn multi_observation_uses_one_commit_and_rejects_all_before_prefetch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("watch.db");
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    let a = store.create(1000, 600).unwrap();
    let b = store.create(1000, 600).unwrap();
    store
        .put(&a.id, &a.write, &[7; 32], &[1; 8], 1200, 1000)
        .unwrap();
    let cursor = store.observe(&b.id, &b.read, 1000).unwrap().0;
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE watch_test_commits(n INTEGER NOT NULL);
        INSERT INTO watch_test_commits VALUES(0);
        CREATE TRIGGER watch_test_clock AFTER UPDATE ON clock_guard BEGIN
          UPDATE watch_test_commits SET n=n+1;
        END;",
    )
    .unwrap();
    let count = || {
        db.query_row("SELECT n FROM watch_test_commits", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap()
    };
    let query = |read, after| {
        [
            MailboxObservationQuery {
                mailbox: &a.id,
                read: &a.read,
                after: None,
            },
            MailboxObservationQuery {
                mailbox: &b.id,
                read,
                after,
            },
        ]
    };
    let wrong = Capability::from_bytes([99; 32]).unwrap();
    assert!(matches!(
        store.observe_many_with_prefetch(&query(&wrong, None), MailboxPrefetchMode::Small, 1001),
        Err(StoreError::Unauthorized)
    ));
    assert_eq!(count(), 0); // clock update/maintenance rolled back with rejected snapshot
    let ahead = MailboxCursorV1 {
        sequence: cursor.sequence + 1,
        ..cursor
    };
    assert!(matches!(
        store.observe_many_with_prefetch(
            &query(&b.read, Some(ahead)),
            MailboxPrefetchMode::Small,
            1001
        ),
        Err(StoreError::Conflict)
    ));
    assert_eq!(count(), 0);
    let result = store
        .observe_many_with_prefetch(
            &query(&b.read, Some(cursor)),
            MailboxPrefetchMode::Small,
            1001,
        )
        .unwrap();
    assert_eq!(count(), 1); // two boxes plus prefetch, one begin/commit
    assert_eq!(result.notices.len(), 2);
    assert!(result.notices[0].1);
    assert!(!result.notices[1].1);
    let (index, row) = result.object.unwrap();
    assert_eq!(index, 0);
    assert_eq!(row.id, [7; 32]);
    assert_eq!(row.expires, 1200);
    drop(store);
    let mut store = MailboxStore::open(&path, Limits::default()).unwrap();
    assert_eq!(
        store.fetch_one(&a.id, &a.read, 1001).unwrap().unwrap().id,
        row.id
    );
    assert!(matches!(
        store.observe_many_with_prefetch(&query(&b.read, None), MailboxPrefetchMode::None, 1000),
        Err(StoreError::ClockRollback)
    ));
    store.revoke(&b.id, &b.read, 1002).unwrap();
    assert!(matches!(
        store.observe_many_with_prefetch(&query(&b.read, None), MailboxPrefetchMode::Bulk, 1002),
        Err(StoreError::Unauthorized)
    ));
    assert!(store.fetch_one(&a.id, &a.read, 1002).unwrap().is_some());
}

#[test]
fn multi_observation_keeps_small_bulk_ttl_and_query_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = MailboxStore::open(dir.path().join("bounds.db"), Limits::default()).unwrap();
    let a = store.create(1000, 600).unwrap();
    store
        .put_bulk_v1(&a.id, &a.write, &[8; 32], &vec![3; 65_537], 1200, 1000)
        .unwrap();
    let q = || MailboxObservationQuery {
        mailbox: &a.id,
        read: &a.read,
        after: None,
    };
    let small = store
        .observe_many_with_prefetch(&[q()], MailboxPrefetchMode::Small, 1000)
        .unwrap();
    assert!(small.notices[0].1);
    assert!(small.object.is_none());
    let bulk = store
        .observe_many_with_prefetch(&[q()], MailboxPrefetchMode::Bulk, 1000)
        .unwrap();
    assert_eq!(bulk.object.unwrap().1.ciphertext.len(), 65_537);
    assert!(store
        .observe_many_with_prefetch(&[q()], MailboxPrefetchMode::None, 1000)
        .unwrap()
        .object
        .is_none());
    assert!(matches!(
        store.observe_many_with_prefetch(&[], MailboxPrefetchMode::Small, 1000),
        Err(StoreError::Invalid)
    ));
    assert!(matches!(
        store.observe_many_with_prefetch(&[q(), q()], MailboxPrefetchMode::Small, 1000),
        Err(StoreError::Invalid)
    ));
    assert!(matches!(
        store.observe_many_with_prefetch(
            &[q(), q(), q(), q(), q()],
            MailboxPrefetchMode::None,
            1000
        ),
        Err(StoreError::Invalid)
    ));
    let expired = store
        .observe_many_with_prefetch(&[q()], MailboxPrefetchMode::Bulk, 1200)
        .unwrap();
    assert!(!expired.notices[0].1);
    assert!(expired.object.is_none());
    assert!(matches!(
        store.observe_many_with_prefetch(&[q()], MailboxPrefetchMode::Bulk, 1600),
        Err(StoreError::Unauthorized)
    ));
}
