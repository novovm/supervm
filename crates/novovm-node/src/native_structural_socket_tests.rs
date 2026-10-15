//! Original socket + original frame + shared structural consumer. This verifies
//! real signed input handoff, not economic execution, persistence or finality.
use crate::native_pipeline::ingress::{
    apfl::{ApflLimits, ApflTransferBatch},
    wire::{encode_transfer_v3, signing_message, FeePolicy, TransferV3},
};
use crate::native_structural_ingress::{
    authenticate_structured_v3, NativeStructuralReceiver, ReceiveDisposition,
    StructuralReceiveLimits,
};
use ed25519_dalek::{Signer, SigningKey};
use novovm_network::{
    NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0, UdpIngressV1, UdpTransport,
};
use novovm_protocol::{decode, encode, EvmNativeMessage, NodeId, ProtocolMessage};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

const CHAIN: u64 = 9_991_711;
const SESSION: [u8; 16] = [17; 16];

fn limits() -> ApflLimits {
    ApflLimits {
        transactions: 4,
        transaction_bytes: 4096,
        body_bytes: 16384,
    }
}

fn signed(nonce: u64, corrupt: bool) -> Vec<u8> {
    let key = SigningKey::from_bytes(&[91; 32]);
    let mut tx = TransferV3 {
        chain_id: CHAIN,
        from: key.verifying_key().to_bytes().to_vec(),
        to: vec![11; 32],
        asset: "NOV".into(),
        amount: 100,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 5,
            slippage_bps: 0,
        },
        signature: key.verifying_key().to_bytes().to_vec(),
    };
    tx.signature
        .extend_from_slice(&key.sign(&signing_message(&tx).unwrap()).to_bytes());
    if corrupt {
        tx.amount += 1;
    }
    encode_transfer_v3(&tx).unwrap()
}

fn frame(sequence: u64, corrupt: bool) -> NovoRudpTransportFrameV0 {
    let apfl = ApflTransferBatch::from_raw(&[signed(sequence, corrupt)], limits())
        .unwrap()
        .encode()
        .unwrap();
    let payload = encode(&ProtocolMessage::EvmNative(
        EvmNativeMessage::Transactions {
            from: NodeId(41),
            chain_id: CHAIN,
            tx_hash: [0; 32],
            tx_count: 1,
            payload: apfl,
            transport_auth: None,
        },
    ))
    .unwrap();
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        SESSION,
        1,
        sequence,
        sequence,
        0,
        payload,
    )
}

fn send_receive(
    socket: &UdpSocket,
    owner: &UdpTransport,
    input: &NovoRudpTransportFrameV0,
) -> NovoRudpTransportFrameV0 {
    socket
        .send_to(&input.encode(), owner.local_addr().unwrap())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(event) = owner.try_recv_ingress(NodeId(40)).unwrap() {
            match event {
                UdpIngressV1::UntrustedNovoRudpFrame { src, frame } => {
                    assert_eq!(src, socket.local_addr().unwrap());
                    return frame;
                }
                UdpIngressV1::Protocol(_) => {
                    panic!("native frame was misclassified as legacy protocol")
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "original socket failed to deliver native frame"
        );
        std::thread::yield_now();
    }
}

#[test]
fn same_original_socket_preserves_structural_order_and_verifies_wallet_signatures() {
    let owner = UdpTransport::bind_for_chain(NodeId(40), "127.0.0.1:0", CHAIN).unwrap();
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut consumer = NativeStructuralReceiver::new(
        SESSION,
        1,
        3,
        StructuralReceiveLimits {
            max_total_frames: 3,
            max_buffered_frames: 3,
            max_buffered_bytes: 3 * 4096,
            max_payload_bytes: 4096,
            bulk_size: 2,
        },
    )
    .unwrap();
    let mut foreign = frame(0, false);
    foreign.session_id = [18; 16];
    assert_eq!(
        consumer
            .accept(send_receive(&sender, &owner, &foreign))
            .unwrap(),
        ReceiveDisposition::IgnoredSession
    );
    assert_eq!(consumer.received_count(), 0);
    let second = frame(1, false);
    assert_eq!(
        consumer
            .accept(send_receive(&sender, &owner, &second))
            .unwrap(),
        ReceiveDisposition::Accepted
    );
    assert!(consumer.take_ready_bulk().is_none());
    let mut repair = second.clone();
    repair.kind = NovoRudpTransportFrameKindV0::Repair;
    assert_eq!(
        consumer
            .accept(send_receive(&sender, &owner, &repair))
            .unwrap(),
        ReceiveDisposition::Duplicate
    );
    let conflict = frame(1, true);
    assert!(consumer
        .accept(send_receive(&sender, &owner, &conflict))
        .is_err());
    assert_eq!(consumer.received_count(), 1);
    let first = frame(0, false);
    consumer
        .accept(send_receive(&sender, &owner, &first))
        .unwrap();
    let bulk = consumer.take_ready_bulk().unwrap();
    assert_eq!(
        bulk.iter()
            .map(|(sequence, _)| *sequence)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    for (sequence, body) in bulk {
        let ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
            payload, tx_count, ..
        }) = decode(&body).unwrap()
        else {
            panic!("unexpected body")
        };
        let checked =
            authenticate_structured_v3(&payload, tx_count as usize, CHAIN, limits()).unwrap();
        assert_eq!(checked.len(), 1);
        assert_eq!(checked[0].transfer().nonce, sequence);
        assert_eq!(checked[0].transfer().amount, 100);
    }
    // Transport completeness and well-formed APFL do not grant business auth.
    consumer
        .accept(send_receive(&sender, &owner, &frame(2, true)))
        .unwrap();
    assert!(consumer.is_complete());
    let tail = consumer.take_ready_bulk().unwrap();
    let ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
        payload, tx_count, ..
    }) = decode(&tail[0].1).unwrap()
    else {
        panic!("unexpected body")
    };
    assert!(authenticate_structured_v3(&payload, tx_count as usize, CHAIN, limits()).is_err());
    assert_eq!(consumer.buffered_bytes(), 0);
}

#[test]
fn original_socket_authenticates_reassembles_and_checks_signed_apfl_without_raw_admission() {
    use novovm_network::novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
        fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits},
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        object_sender::ObjectSendState,
    };
    use novovm_network::{NativeUdpIngressConfigV1, NovoRudpPacingProfile, NovoRudpWindowConfig};
    use novovm_protocol::{GossipMessage, ShardId};
    use std::sync::Arc;

    let limits = ApflLimits {
        transactions: 16,
        transaction_bytes: 4096,
        body_bytes: 16384,
    };
    let lanes = || {
        vec![NativeLaneConfig {
            stream: 1,
            domain: [73; 32],
            receive: ReassemblyLimits {
                max_message_bytes: limits.body_bytes,
                messages: 2,
                bytes: 32768,
                peer_messages: 2,
                peer_bytes: 32768,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: 32768,
                peer_messages: 2,
                peer_bytes: 32768,
            },
        }]
    };
    let policy = || NativeSendPolicy {
        window: NovoRudpWindowConfig::default(),
        pacing: NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 16,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        },
        ttl: Duration::from_secs(5),
    };
    let budget = || NativeHandshakeBudget {
        lifetime: Duration::from_secs(5),
        retry: Duration::from_millis(10),
    };
    let node_key = SigningKey::from_bytes(&[74; 32]);
    let client_key = SigningKey::from_bytes(&[75; 32]);
    let original = UdpTransport::bind_for_chain(NodeId(40), "127.0.0.1:0", CHAIN + 1).unwrap();
    let address = original.local_addr().unwrap();
    let stale_reader = original.clone();
    let now = Instant::now();
    let config = || NativeUdpIngressConfigV1 {
        identity: node_key.clone(),
        peers: vec![],
        client_slots: 2,
        client_streams: vec![1],
        observe_legacy_protocol: true,
        handshake: budget(),
        lanes: lanes(),
        send: policy(),
        bytes_per_minute: Some(1024 * 1024),
    };
    let small =
        UdpTransport::bind_with_packet_size_for_chain(NodeId(40), "127.0.0.1:0", 1024, CHAIN + 2)
            .unwrap();
    let small_clone = small.clone();
    assert!(small
        .into_authenticated_objects_v1(config(), 1000, now)
        .is_err());
    assert!(small_clone.try_recv_ingress(NodeId(40)).unwrap().is_none());
    let mut owner = original
        .into_authenticated_objects_v1(config(), 1000, now)
        .unwrap();
    assert!(stale_reader
        .clone()
        .into_authenticated_objects_v1(config(), 1000, now)
        .is_err());
    assert_eq!(owner.local_addr().unwrap(), address);
    assert!(stale_reader.try_recv_ingress(NodeId(40)).is_err());
    let endpoint = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0").unwrap(),
        client_key,
        NativePeer {
            key: node_key.verifying_key().to_bytes(),
            address,
        },
        budget(),
        1000,
        now,
    )
    .unwrap();
    let mut client = NativeObjectLink::new(endpoint, lanes(), policy()).unwrap();
    let noise = UdpSocket::bind("127.0.0.1:0").unwrap();
    // Plain frames may have valid checksums, but can never grant admission.
    noise.send_to(&frame(0, false).encode(), address).unwrap();
    noise.send_to(b"NVHSHK00bad-version", address).unwrap();
    noise
        .send_to(
            &encode(&ProtocolMessage::Gossip(GossipMessage::Heartbeat {
                from: NodeId(42),
                shard: ShardId(7),
            }))
            .unwrap(),
            address,
        )
        .unwrap();
    let mut raw_rejections = 0;
    let mut crypto_rejections = 0;
    let mut legacy_messages = 0;
    while owner.session_id(0).is_none() || client.session_id(0).is_none() {
        let wall = 1000 + now.elapsed().as_millis() as u64;
        let c = client.poll(wall, Instant::now(), 8).unwrap();
        assert!(c.completed.is_empty());
        let n = owner.poll(wall, Instant::now(), 8).unwrap();
        raw_rejections += n.rejected_protocol;
        crypto_rejections += n.native.rejected;
        legacy_messages += n.protocol.len();
        assert!(owner.poll_complete(1, Instant::now(), 8).unwrap().is_none());
        assert!(
            now.elapsed() < Duration::from_secs(5),
            "original socket handshake"
        );
        std::thread::yield_now();
    }
    assert_eq!(raw_rejections, 1);
    assert_eq!(crypto_rejections, 1);
    assert_eq!(legacy_messages, 1);
    for corrupt in [false, true] {
        let raw: Vec<_> = (0..16).map(|i| signed(i, corrupt && i == 8)).collect();
        let bytes = ApflTransferBatch::from_raw(&raw, limits)
            .unwrap()
            .encode()
            .unwrap();
        assert!(bytes.len() > 768, "must exercise multi-fragment assembly");
        let body = Arc::new(
            OutgoingMessage::with_profile(
                [73; 32],
                bytes.clone(),
                limits.body_bytes,
                FragmentProfile::NativeUdpV2,
            )
            .unwrap(),
        );
        assert!(client.try_send(0, 1, &body, Instant::now()).unwrap());
        let mut received = false;
        let mut acked = false;
        let started = Instant::now();
        while !received || !acked {
            let wall = 1000 + now.elapsed().as_millis() as u64;
            let n = owner.poll(wall, Instant::now(), 16).unwrap();
            assert!(n.native.completed.is_empty());
            if let Some(object) = owner.poll_complete(1, Instant::now(), 16).unwrap() {
                assert!(!received);
                assert_eq!(object.id, body.id());
                assert_eq!(object.peer, owner.peer_id(0).unwrap());
                let assembled: Vec<u8> = object.chunks.into_iter().flatten().collect();
                assert_eq!(assembled, bytes);
                let checked = authenticate_structured_v3(&assembled, 16, CHAIN, limits);
                if corrupt {
                    assert!(checked.is_err());
                } else {
                    let checked = checked.unwrap();
                    assert_eq!(checked.len(), 16);
                    for (i, tx) in checked.iter().enumerate() {
                        assert_eq!(tx.transfer().nonce, i as u64);
                        assert_eq!(tx.transfer().amount, 100);
                    }
                }
                received = true;
            }
            let c = client.poll(wall, Instant::now(), 16).unwrap();
            for completion in c.completed {
                assert_eq!(completion.body.id(), body.id());
                assert_eq!(completion.state, ObjectSendState::Delivered);
                acked = true;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "structured object/ACK"
            );
            std::thread::yield_now();
        }
    }
    // Dropping authenticated state must not reactivate the raw reader clone.
    drop(owner);
    assert!(stale_reader.try_recv_ingress(NodeId(40)).is_err());
}
