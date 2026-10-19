use super::*;
use crate::mailbox_store::MailboxStore;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};

const CONTEXT: [u8; 32] = [41; 32];
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
fn put(id: u8, expires: i64) -> Request {
    Request::Put {
        mailbox: [1; 32],
        write: [2; 32],
        object: [id; 32],
        ciphertext: vec![id; 128],
        expires,
    }
}
fn fetch(read: u8) -> Request {
    Request::Fetch {
        mailbox: [1; 32],
        read: [read; 32],
    }
}
fn watch() -> MailboxWatchV1 {
    MailboxWatchV1 {
        mailbox: [1; 32],
        read: [3; 32],
        after: Some(MailboxCursorV1 {
            generation: [9; 32],
            sequence: u64::MAX,
        }),
    }
}
fn notices_fixture() -> Vec<MailboxNoticeV1> {
    vec![MailboxNoticeV1 {
        cursor: watch().after.unwrap(),
        pending: true,
    }]
}

#[test]
fn all_operational_rows_roundtrip_without_json_on_the_wire() -> Result<()> {
    let requests = vec![
        Request::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires: i64::MAX,
        },
        put(4, i64::MIN),
        fetch(3),
        Request::Consume {
            mailbox: [1; 32],
            read: [3; 32],
            object: [4; 32],
            digest: [5; 32],
        },
        Request::Renew {
            mailbox: [1; 32],
            read: [3; 32],
            ttl: i64::MAX,
        },
        Request::Revoke {
            mailbox: [1; 32],
            read: [3; 32],
        },
        Request::Watch {
            boxes: vec![watch()],
            wait_ms: 0,
        },
        Request::WatchFetchV6 {
            boxes: vec![watch()],
            wait_ms: 10_000,
        },
    ];
    let wire = encode_requests(CONTEXT, &requests)?;
    let batch = RequestBatch::decode(&wire, CONTEXT)?;
    assert_eq!(batch.len(), requests.len());
    for (i, request) in requests.iter().enumerate() {
        // Test oracle ONLY; production codec never serializes JSON/Value.
        assert_eq!(
            serde_json::to_value(request)?,
            serde_json::to_value(batch.request(i)?)?
        );
    }
    assert!(batch.request(batch.len()).is_err());
    let replies = vec![
        Reply::Registered,
        Reply::Stored(StoreReceiptV1 {
            version: 1,
            node: [1; 32],
            mailbox: [2; 32],
            object: [3; 32],
            digest: [4; 32],
            bytes: 128,
            expires: i64::MAX,
            signature: vec![6; 64],
        }),
        Reply::Object {
            object: [3; 32],
            digest: [4; 32],
            expires: i64::MIN,
            ciphertext: vec![7; 128],
        },
        Reply::Empty,
        Reply::Consumed,
        Reply::Renewed,
        Reply::Revoked,
        Reply::Notice(notices_fixture()),
        Reply::NoticeObjectV6 {
            notices: notices_fixture(),
            object: None,
        },
        Reply::NoticeObjectV6 {
            notices: notices_fixture(),
            object: Some(MailboxPrefetchV6 {
                box_index: 0,
                object: [3; 32],
                ciphertext: vec![7; 128],
                digest: [4; 32],
                expires: i64::MAX,
            }),
        },
    ];
    let wire = encode_replies(CONTEXT, &replies)?;
    let decoded = decode_replies(&wire, CONTEXT, replies.len())?;
    assert_eq!(
        serde_json::to_value(&replies)?,
        serde_json::to_value(decoded)?
    );
    for reason in [
        MailboxRejectionV1::Unauthorized,
        MailboxRejectionV1::Invalid,
        MailboxRejectionV1::Quota,
        MailboxRejectionV1::Conflict,
        MailboxRejectionV1::Consumed,
        MailboxRejectionV1::Storage,
    ] {
        let wire = encode_replies(CONTEXT, &[Reply::Rejected(reason)])?;
        assert!(
            matches!(decode_replies(&wire,CONTEXT,1)?.as_slice(),[Reply::Rejected(r)] if *r==reason)
        );
    }
    Ok(())
}

#[test]
fn shared_columns_borrow_capabilities_and_reject_ambiguous_or_unbounded_input() -> Result<()> {
    let requests = (4..20).map(|id| put(id, 600)).collect::<Vec<_>>();
    let wire = encode_requests(CONTEXT, &requests)?;
    let view = apfl_batch::decode(&wire, REQUEST_SCHEMA, LIMITS).map_err(anyhow::Error::msg)?;
    assert!(wire.len() < view.expanded_bytes());
    assert_eq!(
        view.field(0, 1).unwrap().as_ptr(),
        view.field(15, 1).unwrap().as_ptr()
    );
    assert_eq!(
        view.field(0, 2).unwrap().as_ptr(),
        view.field(15, 2).unwrap().as_ptr()
    );
    for n in 0..wire.len() {
        assert!(RequestBatch::decode(&wire[..n], CONTEXT).is_err());
    }
    let mut bad = wire.to_vec();
    bad.push(0);
    assert!(RequestBatch::decode(&bad, CONTEXT).is_err());
    assert!(RequestBatch::decode(&wire, [42; 32]).is_err());
    assert!(decode_replies(&wire, CONTEXT, 16).is_err());
    for (range, value) in [
        (10..12, 17u16.to_le_bytes()),
        (12..14, 9u16.to_le_bytes()),
        (14..16, 1u16.to_le_bytes()),
    ] {
        let mut bad = wire.to_vec();
        bad[range].copy_from_slice(&value);
        assert!(RequestBatch::decode(&bad, CONTEXT).is_err());
    }
    for request in [
        Request::BeginSession,
        Request::BeginContinuousV4,
        Request::BeginContinuousV6,
        Request::EndSession,
    ] {
        assert!(encode_requests(CONTEXT, &[request]).is_err());
    }
    assert!(encode_requests([0; 32], &[fetch(3)]).is_err());
    assert!(encode_requests(CONTEXT, &[]).is_err());
    assert!(encode_requests(CONTEXT, &(0..17).map(|_| fetch(3)).collect::<Vec<_>>()).is_err());
    assert!(encode_requests(
        CONTEXT,
        &[Request::Watch {
            boxes: vec![watch(); 5],
            wait_ms: 0
        }]
    )
    .is_err());
    assert!(encode_requests(
        CONTEXT,
        &[Request::Watch {
            boxes: vec![watch()],
            wait_ms: 10_001
        }]
    )
    .is_err());
    let mut large = put(4, 600);
    if let Request::Put { ciphertext, .. } = &mut large {
        ciphertext.resize(65_537, 0);
    }
    assert!(encode_requests(CONTEXT, &[large]).is_err());
    // Even unused columns must be empty; an unknown operation is not ignored.
    for row in [
        [
            &[3][..],
            &[1; 32],
            &[3; 32],
            b"unexpected",
            b"",
            b"",
            b"",
            b"",
        ],
        [&[99][..], b"", b"", b"", b"", b"", b"", b""],
    ] {
        let bad = build(REQUEST_SCHEMA, CONTEXT, &[row])?;
        assert!(RequestBatch::decode(&bad, CONTEXT).is_err());
    }
    let reply = encode_replies(CONTEXT, &[Reply::Empty])?;
    assert!(decode_replies(&reply, [42; 32], 1).is_err());
    assert!(decode_replies(&reply, CONTEXT, 2).is_err());
    assert!(encode_replies(
        CONTEXT,
        &[Reply::NoticeObjectV6 {
            notices: notices_fixture(),
            object: Some(MailboxPrefetchV6 {
                box_index: 1,
                object: [3; 32],
                ciphertext: vec![1],
                digest: [4; 32],
                expires: 600,
            })
        }]
    )
    .is_err());
    Ok(())
}

async fn run(service: &MailboxServiceV1, requests: &[Request]) -> Result<Vec<Reply>> {
    let wire = encode_requests(CONTEXT, requests)?;
    let reply = execute_local_batch(service, &wire, CONTEXT, || Ok(())).await?;
    decode_replies(&reply, CONTEXT, requests.len())
}

#[tokio::test]
async fn original_owner_receipts_retries_capabilities_and_restart_custody() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("mailbox.db");
    let key = SigningKey::from_bytes(&[71; 32]);
    let pin = key.verifying_key().to_bytes();
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(&path, Default::default(), true)?,
        key.clone(),
    );
    service.set_resources_available(true).await?;
    let expires = now() + 600;
    let replies = run(
        &service,
        &[
            Request::Register {
                mailbox: [1; 32],
                write: [2; 32],
                read: [3; 32],
                expires,
            },
            put(4, expires),
            put(4, expires),
        ],
    )
    .await?;
    assert!(matches!(replies[0], Reply::Registered));
    for r in &replies[1..] {
        let Reply::Stored(receipt) = r else {
            anyhow::bail!("missing durable receipt")
        };
        receipt.verify(pin, [1; 32], [4; 32], &[4; 128], expires)?;
    }
    let mut bad = put(5, expires);
    if let Request::Put { write, .. } = &mut bad {
        *write = [99; 32];
    }
    assert!(matches!(
        run(&service, &[bad, fetch(99)]).await?.as_slice(),
        [
            Reply::Rejected(MailboxRejectionV1::Unauthorized),
            Reply::Rejected(MailboxRejectionV1::Unauthorized)
        ]
    ));
    let watch = run(
        &service,
        &[Request::WatchFetchV6 {
            boxes: vec![MailboxWatchV1 {
                mailbox: [1; 32],
                read: [3; 32],
                after: None,
            }],
            wait_ms: 0,
        }],
    )
    .await?;
    assert!(
        matches!(&watch[0],Reply::NoticeObjectV6{object:Some(o),..} if o.object==[4;32]&&o.ciphertext==vec![4;128])
    );
    service.set_contribution_enabled(false).await?;
    drop(service);
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(&path, Default::default(), true)?,
        key.clone(),
    );
    assert!(!service.contribution_status().await?.enabled);
    let r = run(&service, &[fetch(3), put(5, expires)]).await?;
    assert!(
        matches!(&r[0],Reply::Object{object,ciphertext,expires:e,..} if *object==[4;32]&&ciphertext==&vec![4;128]&&*e==expires)
    );
    assert!(matches!(r[1], Reply::Rejected(MailboxRejectionV1::Quota)));
    let consume = || Request::Consume {
        mailbox: [1; 32],
        read: [3; 32],
        object: [4; 32],
        digest: Sha256::digest([4; 128]).into(),
    };
    assert!(matches!(
        run(&service, &[consume(), fetch(3)]).await?.as_slice(),
        [Reply::Consumed, Reply::Empty]
    ));
    drop(service);
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(&path, Default::default(), true)?,
        key,
    );
    assert!(matches!(
        run(&service, &[consume(), fetch(3)]).await?.as_slice(),
        [Reply::Consumed, Reply::Empty]
    ));
    service.set_resources_available(true).await?;
    service.set_contribution_enabled(true).await?;
    // Schema 3 deletes consumed metadata as well as ciphertext. A new valid
    // Put must establish real custody again, never return a phantom receipt.
    // End-device journals, not node tombstones, deduplicate the original message.
    let restored = run(&service, &[put(4, expires), fetch(3)]).await?;
    let [Reply::Stored(recovered), Reply::Object {
        object,
        ciphertext,
        expires: retained_until,
        ..
    }] = restored.as_slice()
    else {
        anyhow::bail!("new storage receipt requires actual retained custody")
    };
    recovered.verify(pin, [1; 32], [4; 32], &[4; 128], expires)?;
    assert_eq!(*object, [4; 32]);
    assert_eq!(ciphertext, &vec![4; 128]);
    assert_eq!(*retained_until, expires);
    assert_eq!(service.contribution_status().await?.retained_objects, 1);
    assert_eq!(service.contribution_status().await?.retained_bytes, 128);
    assert!(matches!(
        run(&service, &[consume(), fetch(3)]).await?.as_slice(),
        [Reply::Consumed, Reply::Empty]
    ));
    assert_eq!(service.contribution_status().await?.retained_objects, 0);
    assert_eq!(service.contribution_status().await?.retained_bytes, 0);
    assert!(matches!(
        run(
            &service,
            &[
                Request::Revoke {
                    mailbox: [1; 32],
                    read: [3; 32]
                },
                fetch(3)
            ]
        )
        .await?
        .as_slice(),
        [
            Reply::Revoked,
            Reply::Rejected(MailboxRejectionV1::Unauthorized)
        ]
    ));
    Ok(())
}

#[tokio::test]
async fn validates_whole_batch_before_effects_and_does_not_fake_rollback_after_authority_loss(
) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(dir.path().join("mailbox.db"), Default::default(), true)?,
        SigningKey::from_bytes(&[71; 32]),
    );
    service.set_resources_available(true).await?;
    let expires = now() + 600;
    let bytes = encode_requests(
        CONTEXT,
        &[Request::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires,
        }],
    )?;
    let view = apfl_batch::decode(&bytes, REQUEST_SCHEMA, LIMITS).map_err(anyhow::Error::msg)?;
    let invalid = build(
        REQUEST_SCHEMA,
        CONTEXT,
        &[
            fields(&view, 0)?,
            [&[99], b"", b"", b"", b"", b"", b"", b""],
        ],
    )?;
    assert!(execute_local_batch(&service, &invalid, CONTEXT, || Ok(()))
        .await
        .is_err());
    assert!(matches!(
        run(&service, &[fetch(3)]).await?.as_slice(),
        [Reply::Rejected(MailboxRejectionV1::Unauthorized)]
    ));
    execute_local_batch(&service, &bytes, CONTEXT, || Ok(())).await?;
    let bytes = encode_requests(CONTEXT, &[put(4, expires), put(5, expires)])?;
    let checks = AtomicUsize::new(0);
    assert!(execute_local_batch(&service, &bytes, CONTEXT, || {
        ensure!(
            checks.fetch_add(1, Ordering::SeqCst) < 2,
            "authority revoked after commit"
        );
        Ok(())
    })
    .await
    .is_err());
    let r = run(&service, &[fetch(3)]).await?;
    assert!(matches!(&r[0],Reply::Object{object,..} if *object==[4;32]));
    // Retry recovers the original signed receipt; it does not create a second object.
    assert!(matches!(
        run(&service, &[put(4, expires)]).await?.as_slice(),
        [Reply::Stored(_)]
    ));
    let r = run(
        &service,
        &[
            Request::Consume {
                mailbox: [1; 32],
                read: [3; 32],
                object: [4; 32],
                digest: Sha256::digest([4; 128]).into(),
            },
            fetch(3),
        ],
    )
    .await?;
    assert!(matches!(r.as_slice(), [Reply::Consumed, Reply::Empty]));
    Ok(())
}
