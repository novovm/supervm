use super::*;
use crate::mailbox_store::{MailboxGrant, MailboxStore, MAX_BULK_CIPHERTEXT};
use ed25519_dalek::SigningKey;

#[test]
#[cfg(feature = "mailbox-seal")]
fn bulk_schema_bounds_legacy_rejection_and_hpke_domain() -> Result<()> {
    let key = crate::mailbox_seal::MailboxSecretV1::generate()?;
    let plain = vec![71; 1_600_067];
    let sealed = crate::mailbox_seal::seal_mailbox_object_v2(key.public_key(), &plain, b"object")?;
    assert_eq!(&*key.open_object_v2(&sealed, b"object")?, &plain);
    assert!(key.open(&sealed, b"object").is_err());
    assert!(key.open_object_v2(&sealed, b"wrong").is_err());
    let mut tampered = sealed.clone();
    tampered[1000] ^= 1;
    assert!(key.open_object_v2(&tampered, b"object").is_err());
    assert!(crate::mailbox_seal::seal_mailbox_v1(key.public_key(), &plain, b"object").is_err());
    let make = |payload| Request::PutBulkV1 {
        mailbox: [1; 32],
        write: [2; 32],
        object: [3; 32],
        expires: 600,
        ciphertext: payload,
    };
    let bytes = encode_requests([41; 32], &[make(sealed.clone())])?;
    assert!(apfl_batch::decode(&bytes, REQUEST_SCHEMA, LIMITS).is_err());
    assert!(
        matches!(RequestBatch::decode(&bytes, [41;32])?.request(0)?, Request::PutBulkV1 { ref ciphertext, .. } if ciphertext == &sealed)
    );
    assert!(encode_requests([41; 32], &[make(vec![1; MAX_BULK_CIPHERTEXT + 1])]).is_err());
    assert!(encode_requests([41; 32], &[make(vec![1; 10]), make(vec![2; 10])]).is_err());
    assert!(encode_requests(
        [41; 32],
        &[Request::Put {
            mailbox: [1; 32],
            write: [2; 32],
            object: [3; 32],
            expires: 600,
            ciphertext: sealed
        }]
    )
    .is_err());
    let mut wrong_schema = bytes.to_vec();
    wrong_schema[16..24].copy_from_slice(&REQUEST_SCHEMA.to_le_bytes());
    assert!(RequestBatch::decode(&wrong_schema, [41; 32]).is_err());
    Ok(())
}

#[tokio::test]
async fn bulk_one_object_restart_receipt_quota_revoke_and_consume() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("mailbox.db");
    let now = crate::product_rendezvous_daemon::now_ms()? as i64 / 1000;
    let expires = now + 600;
    let signer = SigningKey::from_bytes(&[71; 32]);
    let pin = signer.verifying_key().to_bytes();
    let grant = MailboxGrant::generate()?;
    let payload = vec![17; 1_600_067];
    let mut store = MailboxStore::open(&path, Default::default())?;
    store.register(&grant, now, expires)?;
    assert!(store
        .put(&grant.id, &grant.write, &[4; 32], &payload, expires, now)
        .is_err());
    let service = MailboxServiceV1::new(store, signer.clone());
    let request = || Request::PutBulkV1 {
        mailbox: grant.id,
        write: *grant.write.as_bytes(),
        object: [4; 32],
        ciphertext: payload.clone(),
        expires,
    };
    let encoded = encode_requests([41; 32], &[request()])?;
    let response = execute_local_batch(&service, &encoded, [41; 32], || Ok(())).await?;
    let replies = decode_replies(&response, [41; 32], 1)?;
    let Reply::Stored(receipt) = &replies[0] else {
        anyhow::bail!("no receipt")
    };
    receipt.verify(pin, grant.id, [4; 32], &payload, expires)?;
    assert!(matches!(
        service
            .request_local_v1(Request::Fetch {
                mailbox: grant.id,
                read: *grant.read.as_bytes()
            })
            .await?,
        Reply::Empty
    ));
    drop(service);
    let mut store = MailboxStore::open(&path, Default::default())?;
    // The asynchronous service has advanced the durable clock. Reopening the
    // sole owner samples processing time again, without changing object expiry.
    let now = crate::product_rendezvous_daemon::now_ms()? as i64 / 1000;
    let row = store.fetch_bulk_v1(&grant.id, &grant.read, now)?.unwrap();
    assert_eq!(row.ciphertext, payload);
    assert_eq!(row.expires, expires);
    store.put_bulk_v1(&grant.id, &grant.write, &[4; 32], &payload, expires, now)?;
    assert!(matches!(
        store.put_bulk_v1(&grant.id, &grant.write, &[5; 32], &payload, expires, now),
        Err(crate::mailbox_store::StoreError::Quota)
    ));
    assert!(store
        .put_bulk_v1(
            &grant.id,
            &grant.write,
            &[4; 32],
            &payload,
            expires + 1,
            now
        )
        .is_err());
    let reply = Reply::Object {
        object: row.id,
        ciphertext: row.ciphertext,
        digest: row.digest,
        expires: row.expires,
    };
    let bytes = encode_replies([41; 32], &[reply])?;
    assert!(apfl_batch::decode(&bytes, REPLY_SCHEMA, LIMITS).is_err());
    assert!(
        matches!(&decode_replies(&bytes,[41;32],1)?[0],Reply::Object { ciphertext,.. } if ciphertext==&payload)
    );
    store.consume(&grant.id, &grant.read, &[4; 32], &row.digest, now)?;
    assert!(store.fetch_bulk_v1(&grant.id, &grant.read, now)?.is_none());
    store.revoke(&grant.id, &grant.read, now)?;
    assert!(store
        .put_bulk_v1(&grant.id, &grant.write, &[6; 32], &payload, expires, now)
        .is_err());
    Ok(())
}
