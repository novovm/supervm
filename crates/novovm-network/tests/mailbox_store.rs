use novovm_network::mailbox_store::*;

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
    assert!(matches!(
        store.put(&phone.id, &phone.write, &object, &data, 2000, 1001),
        Err(StoreError::Consumed)
    ));
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
fn consumed_tombstones_are_bounded_and_wrong_digest_cannot_delete() {
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
    assert!(matches!(
        s.put(&a.id, &a.write, &[2; 32], &[5; 32], 250, 100),
        Err(StoreError::Quota)
    ));
    s.sweep(501).unwrap();
    assert!(s.put(&a.id, &a.write, &[2; 32], &[5; 32], 600, 501).is_ok());
}
