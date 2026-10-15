use super::*;

fn binding() -> Binding {
    Binding {
        peer: 0,
        id: "test".into(),
        session: [7; 16],
        local: [3; 32],
        role: NativeApplicationRole::Client,
    }
}
fn slots() -> Slots {
    Arc::new(Mutex::new(std::array::from_fn(|_| ServerSlot {
        sequence: 1,
        phase: Phase::Ready,
    })))
}
fn work(slots: Slots, deadline: Instant) -> Result<MailboxWork> {
    let context = binding().context(0, 1);
    slots.lock().unwrap()[0].phase = Phase::Working;
    Ok(MailboxWork {
        bytes: encode_requests(
            context,
            &[Request::Register {
                mailbox: [1; 32],
                write: [2; 32],
                read: [3; 32],
                expires: i64::MAX,
            }],
        )?,
        context,
        guard: WorkGuard {
            slots,
            slot: 0,
            sequence: 1,
            armed: true,
        },
        peer: 0,
        session: [7; 16],
        signer: [3; 32],
        deadline,
    })
}

#[test]
fn cancelled_unpolled_work_latches_only_its_slot_and_context_separates_scopes() -> Result<()> {
    let slots = slots();
    drop(work(slots.clone(), Instant::now() + REQUEST_DEADLINE)?);
    let locked = slots.lock().unwrap();
    assert!(locked[0].phase == Phase::Failed);
    assert!(locked[1..]
        .iter()
        .all(|s| s.phase == Phase::Ready && s.sequence == 1));
    let b = binding();
    let c = b.context(0, 1);
    assert_ne!(c, b.context(1, 1));
    assert_ne!(c, b.context(0, 2));
    assert_ne!(
        c,
        Binding {
            session: [8; 16],
            ..binding()
        }
        .context(0, 1)
    );
    let requester = [31; 32];
    let responder = [32; 32];
    let client = Binding {
        local: requester,
        id: peer_id_from_ed25519_public_key_v1(&responder),
        ..binding()
    };
    let server = Binding {
        local: responder,
        id: peer_id_from_ed25519_public_key_v1(&requester),
        role: NativeApplicationRole::Service,
        ..binding()
    };
    assert_eq!(client.context(0, 1), server.context(0, 1));
    let reverse_client = Binding {
        role: NativeApplicationRole::Client,
        ..server
    };
    assert_ne!(client.context(0, 1), reverse_client.context(0, 1));
    Ok(())
}

#[tokio::test]
async fn wrong_owner_and_expired_work_fail_before_store_effects() -> Result<()> {
    use crate::mailbox_store::MailboxStore;
    use ed25519_dalek::SigningKey;
    let dir = tempfile::tempdir()?;
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(dir.path().join("owner.db"), Default::default(), true)?,
        SigningKey::from_bytes(&[11; 32]),
    );
    service.set_resources_available(true).await?;
    for deadline in [
        Instant::now() + REQUEST_DEADLINE,
        Instant::now() - Duration::from_secs(1),
    ] {
        let slots = slots();
        assert!(work(slots.clone(), deadline)?
            .execute(&service, || Ok(()))
            .await
            .is_err());
        assert!(slots.lock().unwrap()[0].phase == Phase::Failed);
        assert!(matches!(
            service
                .request_local_v1(Request::Fetch {
                    mailbox: [1; 32],
                    read: [3; 32]
                })
                .await?,
            Reply::Rejected(MailboxRejectionV1::Unauthorized)
        ));
    }
    Ok(())
}

#[test]
fn reply_semantics_are_checked_not_only_context_and_row_count() -> Result<()> {
    use ed25519_dalek::SigningKey;
    let key = SigningKey::from_bytes(&[11; 32]);
    let node = key.verifying_key().to_bytes();
    let put = Request::Put {
        mailbox: [1; 32],
        write: [2; 32],
        object: [3; 32],
        ciphertext: vec![4; 16],
        expires: 1000,
    };
    let mut receipt = StoreReceiptV1::committed(&key, [1; 32], [3; 32], &[4; 16], 1000);
    assert!(verify_reply(node, &put, &Reply::Empty).is_err());
    verify_reply(node, &put, &Reply::Stored(receipt.clone()))?;
    receipt.signature[0] ^= 1;
    assert!(verify_reply(node, &put, &Reply::Stored(receipt)).is_err());
    let fetch = Request::Fetch {
        mailbox: [1; 32],
        read: [3; 32],
    };
    assert!(verify_reply(
        node,
        &fetch,
        &Reply::Object {
            object: [3; 32],
            ciphertext: vec![4; 16],
            digest: [0; 32],
            expires: 1000
        }
    )
    .is_err());
    let watch = Request::Watch {
        boxes: vec![MailboxWatchV1 {
            mailbox: [1; 32],
            read: [3; 32],
            after: None,
        }],
        wait_ms: 0,
    };
    assert!(verify_reply(node, &watch, &Reply::Notice(vec![])).is_err());
    Ok(())
}
