use super::*;

#[test]
fn response_transport_completion_releases_only_its_exact_body() -> Result<()> {
    let make = |context| -> Result<MailboxResponse> {
        Ok(MailboxResponse {
            failed: false,
            sent: true,
            peer: 0,
            session: [7; 16],
            deadline: Instant::now() + REQUEST_DEADLINE,
            body: outgoing(encode_replies(context, &[Reply::Empty])?)?,
            guard: WorkGuard {
                slots: slots(),
                slot: 0,
                sequence: 1,
                armed: false,
                cancellation: NativeSendCancellation::default(),
                validity: NativeSessionValidity::new(),
            },
        })
    };
    let a = make([1; 32])?;
    let b = make([2; 32])?;
    let mut event = NativeSendCompletion {
        peer: 0,
        stream: STREAM,
        state: ObjectSendState::Cancelled,
        body: a.body.clone(),
    };
    assert!(a.transport_complete(&event));
    assert!(!b.transport_complete(&event));
    event.peer = 1;
    assert!(!a.transport_complete(&event));
    event.peer = 0;
    event.stream = STREAM + 1;
    assert!(!a.transport_complete(&event));
    event.stream = STREAM;
    event.body = b.body.clone();
    assert!(b.transport_complete(&event));
    assert!(!a.transport_complete(&event));
    let controller = ServiceConcurrencyV1::new(crate::service_concurrency::ResourceSnapshotV1 {
        cpu_parallelism: 8,
        available_memory_bytes: 1024 * 1024 * 1024,
        pressure: crate::service_concurrency::ResourcePressureV1::Normal,
    });
    event.body = a.body.clone();
    let mut retained = vec![
        (a, controller.try_acquire_work(4096).unwrap()),
        (b, controller.try_acquire_work(4096).unwrap()),
    ];
    assert_eq!(controller.snapshot().active_work, 2);
    retained.retain(|(response, _)| !response.transport_complete(&event));
    assert_eq!(controller.snapshot().active_work, 1);
    assert_eq!(retained.len(), 1);
    assert!(!retained[0].0.transport_complete(&event));
    Ok(())
}

#[test]
fn bulk_probe_reply_requires_exact_capability_and_original_request() {
    let ready = Reply::BulkReady {
        max_ciphertext: crate::mailbox_store::MAX_BULK_CIPHERTEXT as u32,
    };
    assert!(verify_reply([1; 32], &Request::ProbeBulkV1, &ready).is_ok());
    assert!(verify_reply([1; 32], &Request::ServiceRecordV7, &ready).is_err());
    assert!(verify_reply(
        [1; 32],
        &Request::ProbeBulkV1,
        &Reply::BulkReady { max_ciphertext: 1 }
    )
    .is_err());
}

fn binding() -> Binding {
    Binding {
        peer: 0,
        id: "test".into(),
        session: [7; 16],
        local: [3; 32],
        role: NativeApplicationRole::Client,
        validity: NativeSessionValidity::new(),
    }
}
fn slots() -> Slots {
    Arc::new(Mutex::new(
        (0..SLOTS)
            .map(|_| ServerSlot {
                sequence: 1,
                phase: Phase::Ready,
            })
            .collect(),
    ))
}
fn work(slots: Slots, deadline: Instant) -> Result<MailboxWork> {
    let context = binding().context(0, 1);
    slots.lock().unwrap()[0].phase = Phase::Working;
    Ok(MailboxWork {
        capacity_response: None,
        retained_bytes: 8192,
        waiting_watch: false,
        rows: 1,
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
            cancellation: NativeSendCancellation::default(),
            validity: NativeSessionValidity::new(),
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

#[test]
fn native_carrier_locator_reply_requires_explicit_capability_and_bound_valid_record() -> Result<()>
{
    use crate::{
        iroh_transport::IrohAddressV1,
        product_directory::sign_relay_record_v1,
        product_service_lease::{
            IrohServiceEndpointV1, NativeCarrierMailboxEndpointV7, NativeMailboxEndpointV7,
        },
    };
    use ed25519_dalek::SigningKey;
    let key = SigningKey::from_bytes(&[72; 32]);
    let node = key.verifying_key().to_bytes();
    let carrier = NativeCarrierMailboxEndpointV7 {
        iroh: IrohServiceEndpointV1 {
            instance: [7; 32],
            address: IrohAddressV1 {
                endpoint_key: SigningKey::from_bytes(&[73; 32]).verifying_key().to_bytes(),
                direct: vec!["127.0.0.1:4434".parse()?],
                relays: Vec::new(),
            },
        },
    };
    let udp = NativeMailboxEndpointV7 {
        address: "127.0.0.1:4433".parse()?,
        max_connections: 4,
        max_bytes_per_minute: 8 * 1024 * 1024,
    }
    .endpoint()?;
    let now = crate::product_rendezvous_daemon::now_ms()?;
    let sign = |endpoints, issued, expires| {
        sign_relay_record_v1(&key, "native-reply", endpoints, issued, expires, 1)
    };
    let signed = sign(vec![carrier.endpoint()?], now, now + 60_000)?;
    for endpoints in [
        vec![carrier.endpoint()?],
        vec![udp.clone()],
        vec![carrier.endpoint()?, udp.clone()],
    ] {
        verify_reply(
            node,
            &Request::ServiceRecordV7,
            &Reply::ServiceRecordV7 {
                record: sign(endpoints, now, now + 60_000)?,
            },
        )?;
    }
    assert!(verify_reply(
        [99; 32],
        &Request::ServiceRecordV7,
        &Reply::ServiceRecordV7 {
            record: signed.clone()
        }
    )
    .is_err());
    // Authenticated metadata may be inspected even when it is not a valid
    // route yet/anymore. No application authority is granted by this reply.
    for record in [
        sign(vec![carrier.endpoint()?], now - 100, now - 1)?,
        sign(vec![carrier.endpoint()?], now + 60_000, now + 120_000)?,
    ] {
        verify_reply(
            node,
            &Request::ServiceRecordV7,
            &Reply::ServiceRecordV7 {
                record: record.clone(),
            },
        )?;
        assert!(crate::product_directory::validate_relay_record_v1(&record, now).is_err());
        let mut forged = record;
        forged.signature[0] ^= 1;
        assert!(verify_reply(
            node,
            &Request::ServiceRecordV7,
            &Reply::ServiceRecordV7 { record: forged }
        )
        .is_err());
    }
    let mut forged = signed;
    forged.signature[0] ^= 1;
    let mut malformed = carrier.endpoint()?;
    malformed.uri.push('/');
    for rejected in [
        forged,
        sign(vec![carrier.iroh.endpoint()?], now, now + 60_000)?,
        sign(vec![udp, malformed], now, now + 60_000)?,
    ] {
        assert!(verify_reply(
            node,
            &Request::ServiceRecordV7,
            &Reply::ServiceRecordV7 { record: rejected }
        )
        .is_err());
    }
    Ok(())
}

#[test]
fn waiting_watch_classification_uses_validated_batch_and_zero_wait_is_immediate() -> Result<()> {
    let context = [71; 32];
    for wait_ms in [0, 1, 10_000] {
        let requests = [
            Request::Put {
                mailbox: [1; 32],
                write: [2; 32],
                object: [3; 32],
                ciphertext: vec![9; 65_536],
                expires: 1000,
            },
            Request::WatchFetchV6 {
                boxes: vec![MailboxWatchV1 {
                    mailbox: [1; 32],
                    read: [4; 32],
                    after: None,
                }],
                wait_ms,
            },
        ];
        let bytes = encode_requests(context, &requests)?;
        let batch = RequestBatch::decode(&bytes, context)?;
        assert_eq!(batch.has_waiting_watch()?, wait_ms > 0);
        assert!(RequestBatch::decode(&bytes, [72; 32]).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn overload_rejects_without_storage_effects_or_slot_reset() -> Result<()> {
    use crate::mailbox_store::MailboxStore;
    let dir = tempfile::tempdir()?;
    let key = ed25519_dalek::SigningKey::from_bytes(&[81; 32]);
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(dir.path().join("quota.db"), Default::default(), true)?,
        key.clone(),
    );
    service.set_resources_available(true).await?;
    let slots = slots();
    let mut item = work(slots.clone(), Instant::now() + REQUEST_DEADLINE)?;
    item.signer = key.verifying_key().to_bytes();
    assert!(!item.has_waiting_watch());
    let response = item.reject_quota(&service, || Ok(()))?;
    {
        let state = slots.lock().unwrap();
        assert!(state[0].phase == Phase::Working);
        assert_eq!(state[0].sequence, 1);
        assert!(state[1..].iter().all(|s| s.phase == Phase::Ready));
    }
    assert!(matches!(
        service
            .request_local_v1(Request::Fetch {
                mailbox: [1; 32],
                read: [3; 32],
            })
            .await?,
        Reply::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    drop(response);
    assert!(slots.lock().unwrap()[0].phase == Phase::Failed);
    Ok(())
}
