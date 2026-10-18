//! Lost Put ACK recovery uses the existing service and durable tombstone.
//! These are local SQLite/service tests, not Android or network acceptance.
use super::*;
use crate::mailbox_store::PutResult;
use sha2::{Digest, Sha256};

const BOX_ID: [u8; 32] = [81; 32];
const WRITE: [u8; 32] = [82; 32];
const READ: [u8; 32] = [83; 32];
const OBJECT: [u8; 32] = [84; 32];
const PAYLOAD: [u8; 64] = [85; 64];

fn grant() -> Result<MailboxGrant> {
    Ok(MailboxGrant {
        id: BOX_ID,
        write: Capability::from_bytes(WRITE)?,
        read: Capability::from_bytes(READ)?,
    })
}

fn put(expires: i64) -> MailboxRequestV1 {
    MailboxRequestV1::Put {
        mailbox: BOX_ID,
        write: WRITE,
        object: OBJECT,
        ciphertext: PAYLOAD.to_vec(),
        expires,
    }
}

#[tokio::test]
async fn consumed_evidence_recovers_lost_ack_after_restart_without_new_custody() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("mailbox.db");
    let key = SigningKey::from_bytes(&[86; 32]);
    let pin = key.verifying_key().to_bytes();
    let expires = now()? + 600;
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(&path, Default::default(), true)?,
        key.clone(),
    );
    service.set_resources_available(true).await?;
    assert!(matches!(
        service
            .request_local_v1(MailboxRequestV1::Register {
                mailbox: BOX_ID,
                write: WRITE,
                read: READ,
                expires: expires + 60,
            })
            .await?,
        MailboxReplyV1::Registered
    ));
    let MailboxReplyV1::Stored(original) = service.request_local_v1(put(expires)).await? else {
        anyhow::bail!("original durable receipt missing")
    };
    // The caller never commits this ACK. Its reader can already fetch/consume.
    let MailboxReplyV1::Object { digest, .. } = service
        .request_local_v1(MailboxRequestV1::Fetch {
            mailbox: BOX_ID,
            read: READ,
        })
        .await?
    else {
        anyhow::bail!("committed payload missing")
    };
    assert!(matches!(
        service
            .request_local_v1(MailboxRequestV1::Consume {
                mailbox: BOX_ID,
                read: READ,
                object: OBJECT,
                digest,
            })
            .await?,
        MailboxReplyV1::Consumed
    ));
    service.set_contribution_enabled(false).await?;
    drop(service);

    let mut store = MailboxStore::open_contributor(&path, Default::default(), true)?;
    // Existing low-level callers still cannot resurrect consumed objects.
    assert!(matches!(
        store.put(&BOX_ID, &grant()?.write, &OBJECT, &PAYLOAD, expires, now()?),
        Err(StoreError::Consumed)
    ));
    let before = store.observe(&BOX_ID, &grant()?.read, now()?)?;
    assert!(!before.1);
    let service = MailboxServiceV1::new(store, key);
    let changed = service.changed.subscribe();
    let MailboxReplyV1::Stored(recovered) = service.request_local_v1(put(expires)).await? else {
        anyhow::bail!("historical evidence not recovered")
    };
    recovered.verify(pin, BOX_ID, OBJECT, &PAYLOAD, expires)?;
    assert_eq!(
        serde_json::to_vec(&original)?,
        serde_json::to_vec(&recovered)?
    );
    assert!(!changed.has_changed()?);
    assert!(matches!(
        service
            .request_local_v1(MailboxRequestV1::Fetch {
                mailbox: BOX_ID,
                read: READ,
            })
            .await?,
        MailboxReplyV1::Empty
    ));
    let status = service.contribution_status().await?;
    assert!(!status.enabled && !status.resources_available);
    assert_eq!(status.retained_objects, 0);
    assert_eq!(status.retained_bytes, 0);
    let after = service
        .store
        .lock()
        .unwrap()
        .observe(&BOX_ID, &grant()?.read, now()?)?;
    assert_eq!(before.0, after.0);
    assert!(!after.1);
    let db = rusqlite::Connection::open(&path)?;
    let tombstone: (i64, i64, i64) = db.query_row(
        "SELECT length(payload),consumed,expires FROM envelopes WHERE mailbox=?1 AND id=?2",
        rusqlite::params![BOX_ID, OBJECT],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(tombstone, (0, 1, expires));
    Ok(())
}

#[test]
fn consumed_evidence_rejects_mismatched_revoked_expired_and_missing_commitments() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut store = MailboxStore::open(dir.path().join("mailbox.db"), Default::default())?;
    let grant = grant()?;
    store.register(&grant, 100, 1000)?;
    assert_eq!(
        store.put(&BOX_ID, &grant.write, &OBJECT, &PAYLOAD, 600, 100)?,
        PutResult::Stored
    );
    // A live payload is not consumed evidence.
    assert!(matches!(
        store.verify_consumed_deposit(&BOX_ID, &grant.write, &OBJECT, &PAYLOAD, 600, 100),
        Err(StoreError::Consumed)
    ));
    store.consume(
        &BOX_ID,
        &grant.read,
        &OBJECT,
        &Sha256::digest(PAYLOAD).into(),
        100,
    )?;
    let wrong = Capability::from_bytes([90; 32])?;
    assert!(matches!(
        store.verify_consumed_deposit(&BOX_ID, &wrong, &OBJECT, &PAYLOAD, 600, 101),
        Err(StoreError::Unauthorized)
    ));
    assert!(matches!(
        store.verify_consumed_deposit(&[91; 32], &grant.write, &OBJECT, &PAYLOAD, 600, 101),
        Err(StoreError::Unauthorized)
    ));
    assert!(matches!(
        store.verify_consumed_deposit(&BOX_ID, &grant.write, &[92; 32], &PAYLOAD, 600, 101),
        Err(StoreError::Consumed)
    ));
    assert!(matches!(
        store.verify_consumed_deposit(&BOX_ID, &grant.write, &OBJECT, &[93; 64], 600, 101),
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store.verify_consumed_deposit(&BOX_ID, &grant.write, &OBJECT, &PAYLOAD, 601, 101),
        Err(StoreError::Conflict)
    ));
    store.verify_consumed_deposit(&BOX_ID, &grant.write, &OBJECT, &PAYLOAD, 600, 101)?;
    assert!(matches!(
        store.verify_consumed_deposit(&BOX_ID, &grant.write, &OBJECT, &PAYLOAD, 600, 600),
        Err(StoreError::Invalid)
    ));
    store.revoke(&BOX_ID, &grant.read, 102)?;
    assert!(matches!(
        store.verify_consumed_deposit(&BOX_ID, &grant.write, &OBJECT, &PAYLOAD, 600, 102),
        Err(StoreError::Unauthorized)
    ));
    Ok(())
}

#[tokio::test]
async fn consumed_evidence_service_does_not_sign_conflicting_or_revoked_put() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut store = MailboxStore::open(dir.path().join("mailbox.db"), Default::default())?;
    let grant = grant()?;
    let at = now()?;
    let expires = at + 600;
    store.register(&grant, at, expires + 60)?;
    store.put(&BOX_ID, &grant.write, &OBJECT, &PAYLOAD, expires, at)?;
    store.consume(
        &BOX_ID,
        &grant.read,
        &OBJECT,
        &Sha256::digest(PAYLOAD).into(),
        at,
    )?;
    let service = MailboxServiceV1::new(store, SigningKey::from_bytes(&[86; 32]));
    let mut conflict = put(expires);
    if let MailboxRequestV1::Put { ciphertext, .. } = &mut conflict {
        ciphertext[0] ^= 1;
    }
    assert!(matches!(
        service.request_local_v1(conflict).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict)
    ));
    assert!(matches!(
        service.request_local_v1(put(expires + 1)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict)
    ));
    assert!(matches!(
        service.request_local_v1(put(at)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Invalid)
    ));
    assert!(matches!(
        service
            .request_local_v1(MailboxRequestV1::Revoke {
                mailbox: BOX_ID,
                read: READ,
            })
            .await?,
        MailboxReplyV1::Revoked
    ));
    assert!(matches!(
        service.request_local_v1(put(expires)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    Ok(())
}
