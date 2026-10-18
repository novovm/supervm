//! Intermediary retention v3: deletion and lost ACK recovery with real custody.
//! Local SQLite/service tests, not Android or physical flash-erasure evidence.
use super::*;
use sha2::{Digest, Sha256};

const BOX_ID: [u8; 32] = [81; 32];
const WRITE: [u8; 32] = [82; 32];
const READ: [u8; 32] = [83; 32];
const OBJECT: [u8; 32] = [84; 32];
const PAYLOAD: [u8; 64] = [85; 64];

fn put(expires: i64) -> MailboxRequestV1 {
    MailboxRequestV1::Put {
        mailbox: BOX_ID,
        write: WRITE,
        object: OBJECT,
        ciphertext: PAYLOAD.to_vec(),
        expires,
    }
}
fn consume(digest: [u8; 32]) -> MailboxRequestV1 {
    MailboxRequestV1::Consume {
        mailbox: BOX_ID,
        read: READ,
        object: OBJECT,
        digest,
    }
}
fn fetch() -> MailboxRequestV1 {
    MailboxRequestV1::Fetch {
        mailbox: BOX_ID,
        read: READ,
    }
}
fn row_count(path: &std::path::Path) -> Result<i64> {
    Ok(rusqlite::Connection::open(path)?
        .query_row("SELECT count(*) FROM envelopes", [], |r| r.get(0))?)
}
async fn register(service: &MailboxServiceV1, expires: i64) -> Result<()> {
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
    Ok(())
}

#[tokio::test]
async fn consumed_retry_after_restart_requires_real_custody_and_original_expiry() -> Result<()> {
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
    register(&service, expires).await?;
    let MailboxReplyV1::Stored(original) = service.request_local_v1(put(expires)).await? else {
        anyhow::bail!("original durable receipt missing")
    };
    let digest = Sha256::digest(PAYLOAD).into();
    // Sender loses the Put ACK. Receiver has saved its message and sends Consume.
    assert!(matches!(
        service.request_local_v1(consume(digest)).await?,
        MailboxReplyV1::Consumed
    ));
    assert_eq!(row_count(&path)?, 0);
    service.set_contribution_enabled(false).await?;
    drop(service);

    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(&path, Default::default(), true)?,
        key,
    );
    assert_eq!(row_count(&path)?, 0);
    // Lost Consume ACK is retryable with read authority, even while contribution
    // is disabled. Absence is not evidence for signing a historical StoreReceipt.
    assert!(matches!(
        service.request_local_v1(consume(digest)).await?,
        MailboxReplyV1::Consumed
    ));
    assert!(matches!(
        service.request_local_v1(put(expires)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Quota)
    ));
    service.set_contribution_enabled(true).await?;
    assert!(matches!(
        service.request_local_v1(put(expires)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Quota)
    ));
    service.set_resources_available(true).await?;
    let before = service
        .store
        .lock()
        .unwrap()
        .observe(&BOX_ID, &Capability::from_bytes(READ)?, now()?)?
        .0;
    let changed = service.changed.subscribe();
    let MailboxReplyV1::Stored(recovered) = service.request_local_v1(put(expires)).await? else {
        anyhow::bail!("retry did not commit real custody")
    };
    recovered.verify(pin, BOX_ID, OBJECT, &PAYLOAD, expires)?;
    // Receipt is deterministic for the same commitment, NOT a new delivery/NOV reward.
    assert_eq!(original.signature, recovered.signature);
    assert!(changed.has_changed()?);
    assert_eq!(row_count(&path)?, 1);
    let MailboxReplyV1::Object {
        object,
        ciphertext,
        expires: stored_expiry,
        ..
    } = service.request_local_v1(fetch()).await?
    else {
        anyhow::bail!("retry missing payload")
    };
    assert_eq!(
        (object, ciphertext, stored_expiry),
        (OBJECT, PAYLOAD.to_vec(), expires)
    );
    let after = service
        .store
        .lock()
        .unwrap()
        .observe(&BOX_ID, &Capability::from_bytes(READ)?, now()?)?
        .0;
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.sequence, before.sequence + 1);
    // Existing receiver dedup must reconcile this retry, then delete it again.
    assert!(matches!(
        service.request_local_v1(consume(digest)).await?,
        MailboxReplyV1::Consumed
    ));
    assert_eq!(row_count(&path)?, 0);
    Ok(())
}

#[tokio::test]
async fn consumed_cleanup_keeps_capability_conflict_expiry_and_revocation_checks() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("mailbox.db");
    let service = MailboxServiceV1::new(
        MailboxStore::open(&path, Default::default())?,
        SigningKey::from_bytes(&[86; 32]),
    );
    let expires = now()? + 600;
    register(&service, expires).await?;
    service.request_local_v1(put(expires)).await?;
    let digest = Sha256::digest(PAYLOAD).into();
    let wrong = || MailboxRequestV1::Consume {
        mailbox: BOX_ID,
        read: [90; 32],
        object: OBJECT,
        digest,
    };
    assert!(matches!(
        service.request_local_v1(wrong()).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    assert!(matches!(
        service.request_local_v1(consume([0; 32])).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict)
    ));
    let conflict = || {
        let mut request = put(expires);
        if let MailboxRequestV1::Put { ciphertext, .. } = &mut request {
            ciphertext[0] ^= 1;
        }
        request
    };
    assert!(matches!(
        service.request_local_v1(conflict()).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict)
    ));
    assert!(matches!(
        service.request_local_v1(put(expires + 1)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict)
    ));
    service.request_local_v1(consume(digest)).await?;
    assert_eq!(row_count(&path)?, 0);
    assert!(matches!(
        service.request_local_v1(wrong()).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    // Once deleted, the node cannot remember the former commitment. A new
    // authorized deposit is real custody; an old Consume must not erase it.
    assert!(matches!(
        service.request_local_v1(conflict()).await?,
        MailboxReplyV1::Stored(_)
    ));
    assert!(matches!(
        service.request_local_v1(consume(digest)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict)
    ));
    assert_eq!(row_count(&path)?, 1);
    assert!(matches!(
        service.request_local_v1(put(now()?)).await?,
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
    assert_eq!(row_count(&path)?, 0);
    assert!(matches!(
        service.request_local_v1(put(expires)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    assert!(matches!(
        service.request_local_v1(consume(digest)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    Ok(())
}
