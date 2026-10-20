//! Fault-injected signed metadata over the existing real UDP SDK fixture.
//! A production service uses its own clock; this peer deliberately models a
//! valid signer whose clock differs from the receiver's clock.
use super::*;
use novovm_network::{
    mailbox_structured::{
        encode_replies,
        native_requests::{NativeMailboxDriver, NativeRequestAuthority, NativeRequestErrorKind},
        RequestBatch,
    },
    novorudp::fragments::{FragmentProfile, OutgoingMessage},
    product_directory::{validate_relay_record_v1, ProductDirectoryErrorV1},
};
use std::collections::{BTreeSet, VecDeque};

async fn temporal_peer(
    mut link: NativeObjectLink,
    route: NativeMailboxEndpointV7,
    stop: Arc<AtomicBool>,
    forged: bool,
) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(10);
    let mut contexts = BTreeSet::new();
    let mut replies = VecDeque::new();
    let mut metadata_count = 0;
    let mut attached = None;
    while !stop.load(Ordering::Acquire) {
        ensure!(Instant::now() < end, "temporal fixture server deadline");
        let now = Instant::now();
        let _ = link.poll(now_ms()?, now, 64)?;
        if attached.is_none() && link.session_id(0).is_some() {
            attached = Some(NativeMailboxServer::attach(&mut link, 0)?);
        }
        while let Some(body) = link.poll_complete(STREAM, now, 64)? {
            // Real peer/session encryption and reassembly precede this test-only
            // response injection; request context is taken from validated APFL.
            let bytes: Vec<u8> = body.chunks.into_iter().flatten().collect();
            let context: [u8; 32] = bytes
                .get(24..56)
                .context("temporal request context")?
                .try_into()?;
            let batch = RequestBatch::decode(&bytes, context)?;
            ensure!(batch.len() == 1, "temporal fixture request count");
            ensure!(
                contexts.insert(context),
                "request sequence/context was reset"
            );
            let reply = match batch.request(0)? {
                Request::ServiceRecordV7 => {
                    let wall = now_ms()?;
                    let (issued, expires) = match metadata_count {
                        0 => (wall + 60_000, wall + 120_000),
                        1 => (wall - 60_000, wall - 1),
                        _ => (wall - 1, wall + 60_000),
                    };
                    metadata_count += 1;
                    let mut record = sign_relay_record_v1(
                        &key(17),
                        "temporal-metadata-fixture",
                        vec![route.endpoint()?],
                        issued,
                        expires,
                        metadata_count,
                    )?;
                    if forged {
                        record.signature[0] ^= 1;
                    }
                    Reply::ServiceRecordV7 { record }
                }
                Request::ProbeBulkV1 => Reply::BulkReady {
                    max_ciphertext: novovm_network::mailbox_store::MAX_BULK_CIPHERTEXT as u32,
                },
                _ => bail!("unexpected temporal fixture request"),
            };
            let encoded = encode_replies(context, &[reply])?;
            replies.push_back(Arc::new(OutgoingMessage::with_profile(
                domain(),
                encoded.to_vec(),
                LIMITS.wire_bytes,
                FragmentProfile::NativeUdpV2,
            )?));
        }
        if let Some(reply) = replies.front() {
            if link.try_send(0, STREAM, reply, now)? {
                replies.pop_front();
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    Ok(())
}

async fn run_temporal_fixture(forged: bool) -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let route = NativeMailboxEndpointV7 {
        address: socket.local_addr()?,
        max_connections: 1,
        max_bytes_per_minute: 8 * 1024 * 1024,
    };
    let server_link = NativeObjectLink::new(
        NativeEndpoint::listen_clients(
            socket,
            key(17),
            1,
            vec![STREAM],
            NativeHandshakeBudget {
                lifetime: Duration::from_secs(5),
                retry: Duration::from_millis(50),
            },
        )?,
        vec![lane()],
        NativeSendPolicy {
            window: NovoRudpWindowConfig {
                batch_pause_ms: 0,
                ack_wait_ms: 100,
                packet_copies: 1,
                tail_packet_copies: 1,
                ..Default::default()
            },
            pacing: NovoRudpPacingProfile {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 32,
                batch_pause_ms: 0,
                ack_wait_ms: 100,
                no_progress_backoff: false,
            },
            ttl: Duration::from_secs(5),
        },
    )?;
    let stop = Arc::new(AtomicBool::new(false));
    let server = temporal_peer(server_link, route.clone(), stop.clone(), forged);
    let client = async {
        let mut link = Client::connect_link(&route, 174).await?;
        let (driver, handle) =
            NativeMailboxDriver::attach(&mut link, 0, key(17).verifying_key().to_bytes())?;
        let authority: NativeRequestAuthority = Arc::new(|| Ok(()));
        let original_deadline = Instant::now() + Duration::from_secs(5);
        let operations = async {
            if forged {
                let error = handle
                    .request(
                        3,
                        vec![Request::ServiceRecordV7],
                        original_deadline,
                        authority.clone(),
                    )
                    .await
                    .err()
                    .expect("forged metadata must fail");
                assert_eq!(error.kind, NativeRequestErrorKind::Protocol);
                let metadata_again = handle
                    .request(
                        3,
                        vec![Request::ServiceRecordV7],
                        original_deadline,
                        authority.clone(),
                    )
                    .await
                    .err()
                    .expect("forged metadata must fail");
                assert_eq!(metadata_again.kind, NativeRequestErrorKind::Protocol);
                let other_slot = handle
                    .request(
                        0,
                        vec![Request::ProbeBulkV1],
                        original_deadline,
                        authority.clone(),
                    )
                    .await
                    .err()
                    .expect("forged metadata must fail");
                assert_eq!(other_slot.kind, NativeRequestErrorKind::Protocol);
                return Ok::<(), anyhow::Error>(());
            }
            let (future, business) = tokio::join!(
                handle.request(
                    3,
                    vec![Request::ServiceRecordV7],
                    original_deadline,
                    authority.clone()
                ),
                handle.request(
                    0,
                    vec![Request::ProbeBulkV1],
                    original_deadline,
                    authority.clone()
                )
            );
            let future = future?;
            assert!(matches!(business?.as_slice(), [Reply::BulkReady { .. }]));
            let [Reply::ServiceRecordV7 { record }] = future.as_slice() else {
                bail!("missing signed future metadata");
            };
            assert!(matches!(
                validate_relay_record_v1(record, now_ms()?),
                Err(ProductDirectoryErrorV1::NotYetValid)
            ));
            // These reuse the successfully completed slot with fresh request
            // contexts, without a reconnect, slot reset, or deadline extension.
            let expired = handle
                .request(
                    3,
                    vec![Request::ServiceRecordV7],
                    original_deadline,
                    authority.clone(),
                )
                .await?;
            let [Reply::ServiceRecordV7 { record }] = expired.as_slice() else {
                bail!("missing signed expired metadata");
            };
            assert!(matches!(
                validate_relay_record_v1(record, now_ms()?),
                Err(ProductDirectoryErrorV1::Expired)
            ));
            let valid = handle
                .request(
                    3,
                    vec![Request::ServiceRecordV7],
                    original_deadline,
                    authority.clone(),
                )
                .await?;
            let [Reply::ServiceRecordV7 { record }] = valid.as_slice() else {
                bail!("missing signed current metadata");
            };
            validate_relay_record_v1(record, now_ms()?)?;
            let business = handle
                .request(
                    0,
                    vec![Request::ProbeBulkV1],
                    original_deadline,
                    authority.clone(),
                )
                .await?;
            assert!(matches!(business.as_slice(), [Reply::BulkReady { .. }]));
            Ok(())
        };
        let pump = drive_requests(link, driver);
        tokio::pin!(operations, pump);
        tokio::select! {
            result = &mut operations => result,
            result = &mut pump => {
                if !forged {
                    result?;
                    bail!("temporal metadata terminated healthy driver");
                }
                let error = result.err().expect("forged metadata must fail");
                assert_eq!(
                    error.downcast_ref::<novovm_network::mailbox_structured::native_requests::NativeRequestError>()
                        .context("expected typed protocol failure")?.kind,
                    NativeRequestErrorKind::Protocol
                );
                operations.await
            }
        }
    };
    tokio::pin!(server);
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(8), client) => result?,
        result = &mut server => { result?; bail!("temporal peer ended before client") }
    };
    stop.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(2), &mut server).await??;
    result
}

#[tokio::test]
async fn authenticated_temporal_metadata_keeps_next_sequence_and_other_slot_usable() -> Result<()> {
    run_temporal_fixture(false).await
}

#[tokio::test]
async fn forged_future_metadata_remains_terminal_for_request_driver() -> Result<()> {
    run_temporal_fixture(true).await
}
