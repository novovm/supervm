use super::*;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn local_client_shares_remote_commits_notifications_and_retained_custody() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("mailbox.db");
    let signer = SigningKey::from_bytes(&[71; 32]);
    let pin = signer.verifying_key().to_bytes();
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(&path, Default::default(), true)?,
        signer.clone(),
    );
    let expires = now()? + 600;
    let register = || MailboxRequestV1::Register {
        mailbox: [1; 32],
        write: [2; 32],
        read: [3; 32],
        expires,
    };
    assert!(matches!(
        service.request_local_v1(register()).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Quota)
    ));
    service.set_resources_available(true).await?;
    assert!(matches!(
        service.request_local_v1(register()).await?,
        MailboxReplyV1::Registered
    ));
    let query = |after| MailboxRequestV1::WatchFetchV6 {
        boxes: vec![MailboxWatchV1 {
            mailbox: [1; 32],
            read: [3; 32],
            after,
        }],
        wait_ms: 10_000,
    };
    let MailboxReplyV1::NoticeObjectV6 {
        notices,
        object: None,
    } = service.request_local_v1(query(None)).await?
    else {
        anyhow::bail!("initial snapshot")
    };
    let put = |id| MailboxRequestV1::Put {
        mailbox: [1; 32],
        write: [2; 32],
        object: [id; 32],
        ciphertext: vec![9; 64],
        expires,
    };
    // Local Watch is awakened by the exact executor used after a remote
    // stream's authentication/admission, not a separate local cache or event.
    let (notice, stored) = tokio::join!(
        service.request_local_v1(query(Some(notices[0].cursor))),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            service.admit_request_v4().await?;
            service.execute(put(4)).await
        }
    );
    let MailboxReplyV1::Stored(receipt) = stored? else {
        anyhow::bail!("store proof")
    };
    receipt.verify(pin, [1; 32], [4; 32], &[9; 64], expires)?;
    let MailboxReplyV1::NoticeObjectV6 {
        object: Some(row), ..
    } = notice?
    else {
        anyhow::bail!("local receive wake lost")
    };
    assert_eq!(row.object, [4; 32]);
    assert_eq!(row.expires, expires);
    service.set_contribution_enabled(false).await?;
    assert!(matches!(
        service.request_local_v1(put(5)).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Quota)
    ));
    // Restart OFF; platform availability starts false again. Prior custody
    // remains readable, with the original ciphertext, digest and expiry.
    drop(service);
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(&path, Default::default(), true)?,
        signer,
    );
    assert!(!service.contribution_status().await?.enabled);
    let fetch = |read| MailboxRequestV1::Fetch {
        mailbox: [1; 32],
        read,
    };
    assert!(matches!(
        service.request_local_v1(fetch([7; 32])).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    let MailboxReplyV1::Object {
        object,
        ciphertext,
        expires: retained,
        ..
    } = service.request_local_v1(fetch([3; 32])).await?
    else {
        anyhow::bail!("lost custody")
    };
    assert_eq!(object, [4; 32]);
    assert_eq!(ciphertext, vec![9; 64]);
    assert_eq!(retained, expires);
    assert!(matches!(
        service
            .request_local_v1(MailboxRequestV1::Consume {
                mailbox: [1; 32],
                read: [3; 32],
                object,
                digest: Sha256::digest(&ciphertext).into(),
            })
            .await?,
        MailboxReplyV1::Consumed
    ));
    assert!(matches!(
        service.request_local_v1(fetch([3; 32])).await?,
        MailboxReplyV1::Empty
    ));
    Ok(())
}

#[tokio::test]
async fn local_client_cannot_negotiate_transport_or_bypass_rate_and_revocation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut store = MailboxStore::open(directory.path().join("mailbox.db"), Default::default())?;
    let grant = store.create(now()?, 600)?;
    let service = MailboxServiceV1::new(store, SigningKey::from_bytes(&[72; 32]))
        .with_continuous_request_rate_v4(1, 10)?;
    for request in [
        MailboxRequestV1::BeginSession,
        MailboxRequestV1::BeginContinuousV4,
        MailboxRequestV1::BeginContinuousV6,
        MailboxRequestV1::EndSession,
    ] {
        assert!(matches!(
            service.request_local_v1(request).await?,
            MailboxReplyV1::Rejected(MailboxRejectionV1::Invalid)
        ));
    }
    service.admit_request_v4().await?; // A remote request spent the same budget.
    let local = service.request_local_v1(MailboxRequestV1::Fetch {
        mailbox: grant.id,
        read: *grant.read.as_bytes(),
    });
    tokio::pin!(local);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut local)
            .await
            .is_err()
    );
    assert!(matches!(local.await?, MailboxReplyV1::Empty));
    assert!(matches!(
        service
            .request_local_v1(MailboxRequestV1::Revoke {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
            })
            .await?,
        MailboxReplyV1::Revoked
    ));
    assert!(matches!(
        service
            .request_local_v1(MailboxRequestV1::Fetch {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
            })
            .await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    Ok(())
}
