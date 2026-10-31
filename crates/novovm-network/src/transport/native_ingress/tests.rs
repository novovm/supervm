use super::*;
use crate::novorudp::{
    fragments::{FragmentProfile, ReassemblyLimits},
    object_link::NativeSendLimits,
};
use crate::{NovoRudpPacingProfile, NovoRudpWindowConfig, Transport};
use std::{net::UdpSocket, time::Duration};

const CONTROL: u64 = 1;
const APPLICATION: u64 = 2;

fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(3),
        retry: Duration::from_millis(10),
    }
}

fn lanes() -> Vec<NativeLaneConfig> {
    [CONTROL, APPLICATION]
        .into_iter()
        .map(|stream| NativeLaneConfig {
            stream,
            domain: [stream as u8; 32],
            receive: ReassemblyLimits {
                max_message_bytes: 8192,
                messages: 2,
                bytes: 16384,
                peer_messages: 1,
                peer_bytes: 8192,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: 16384,
                peer_messages: 1,
                peer_bytes: 8192,
            },
        })
        .collect()
}

fn send_policy() -> NativeSendPolicy {
    NativeSendPolicy {
        window: NovoRudpWindowConfig::default(),
        pacing: NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 16,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        },
        ttl: Duration::from_secs(2),
    }
}

fn config() -> NativeUdpIngressConfigV1 {
    NativeUdpIngressConfigV1 {
        identity: SigningKey::from_bytes(&[81; 32]),
        peers: vec![],
        client_slots: 1,
        client_streams: vec![APPLICATION],
        observe_legacy_protocol: false,
        handshake: budget(),
        lanes: lanes(),
        send: send_policy(),
        bytes_per_minute: Some(1_000_000),
    }
}

#[test]
fn invalid_client_stream_policies_do_not_claim_the_original_receiver() -> Result<()> {
    let transport = UdpTransport::bind(NodeId(81), "127.0.0.1:0")?;
    for streams in [vec![], vec![APPLICATION, APPLICATION], vec![0], vec![3]] {
        let mut policy = config();
        policy.client_streams = streams;
        assert!(transport
            .clone()
            .into_authenticated_objects_v1(policy, 1000, Instant::now())
            .is_err());
        assert!(transport.try_recv(NodeId(81))?.is_none());
    }
    let mut policy = config();
    policy.client_slots = 0;
    assert!(transport
        .clone()
        .into_authenticated_objects_v1(policy, 1000, Instant::now())
        .is_err());
    assert!(transport.try_recv(NodeId(81))?.is_none());
    let mut policy = config();
    policy.lanes.push(policy.lanes[0].clone());
    assert!(transport
        .clone()
        .into_authenticated_objects_v1(policy, 1000, Instant::now())
        .is_err());
    let owner = transport
        .clone()
        .into_authenticated_objects_v1(config(), 1000, Instant::now())?;
    assert!(owner.accepts_clients());
    assert!(transport.try_recv(NodeId(81)).is_err());
    assert!(transport
        .clone()
        .into_authenticated_objects_v1(config(), 1000, Instant::now())
        .is_err());
    drop(owner);
    assert!(transport.try_recv_ingress(NodeId(81)).is_err());
    Ok(())
}

#[test]
fn native_only_policy_rejects_plain_protocol_before_legacy_side_effects() -> Result<()> {
    use crate::runtime_status::snapshot_network_runtime_native_pending_tx_summary_v1;
    use novovm_protocol::{encode, EvmNativeMessage, GossipMessage, ShardId};

    let chain = 9_718_082;
    let transport = UdpTransport::bind_for_chain(NodeId(83), "127.0.0.1:0", chain)?;
    let stale = transport.clone();
    let sender = UdpSocket::bind("127.0.0.1:0")?;
    let before = snapshot_network_runtime_native_pending_tx_summary_v1(chain);
    let messages = [
        ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: NodeId(84),
            shard: ShardId(1),
        }),
        ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
            from: NodeId(84),
            chain_id: chain,
            tx_hash: [84; 32],
            tx_count: 1,
            payload: vec![84; 32],
            transport_auth: None,
        }),
    ];
    for message in &messages {
        sender.send_to(&encode(message)?, transport.local_addr()?)?;
    }
    let start = Instant::now();
    let mut owner = transport.into_authenticated_objects_v1(config(), 1000, start)?;
    assert!(owner
        .send_protocol(NodeId(84), &messages[0])
        .unwrap_err()
        .to_string()
        .contains("native-only ownership"));
    let mut rejected = 0;
    while rejected < messages.len() {
        ensure!(
            start.elapsed() < Duration::from_secs(3),
            "plain receive timeout"
        );
        let report = owner.poll(1000, Instant::now(), 64)?;
        assert!(report.protocol.is_empty());
        rejected += report.rejected_protocol;
    }
    assert_eq!(rejected, messages.len());
    assert!(
        !stale.peers.contains_key(&NodeId(84)),
        "legacy peer learning ran"
    );
    let after = snapshot_network_runtime_native_pending_tx_summary_v1(chain);
    assert_eq!(after.tx_count, before.tx_count);
    assert_eq!(after.pending_count, before.pending_count);
    assert!(stale.try_recv(NodeId(83)).is_err());
    assert!(stale.try_recv_ingress(NodeId(83)).is_err());
    assert_eq!(owner.peer_count(), 0);
    Ok(())
}

#[test]
fn open_client_cannot_deliver_on_validator_lane_but_application_remains_live() -> Result<()> {
    let original = UdpTransport::bind(NodeId(82), "127.0.0.1:0")?;
    let remote = NativePeer {
        key: config().identity.verifying_key().to_bytes(),
        address: original.local_addr()?,
    };
    let start = Instant::now();
    let mut server = original.into_authenticated_objects_v1(config(), 1000, start)?;
    let endpoint = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        SigningKey::from_bytes(&[82; 32]),
        remote,
        budget(),
        1000,
        start,
    )?;
    let mut client = NativeObjectLink::new(endpoint, lanes(), send_policy())?;
    while client.session_id(0).is_none() || server.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(3),
            "handshake timeout"
        );
        let _ = client.poll(1000, Instant::now(), 64)?;
        let _ = server.poll(1000, Instant::now(), 64)?;
    }
    let object = |stream| -> Result<_> {
        Ok(Arc::new(OutgoingMessage::with_profile(
            [stream as u8; 32],
            vec![stream as u8; 200],
            8192,
            FragmentProfile::NativeUdpV2,
        )?))
    };
    let forbidden = object(CONTROL)?;
    let allowed = object(APPLICATION)?;
    assert!(client.try_send(0, CONTROL, &forbidden, Instant::now())?);
    assert!(client.try_send(0, APPLICATION, &allowed, Instant::now())?);
    let mut rejected = 0;
    let mut completed = false;
    let start = Instant::now();
    while !completed || rejected == 0 {
        ensure!(start.elapsed() < Duration::from_secs(3), "delivery timeout");
        let _ = client.poll(1000, Instant::now(), 64)?;
        rejected += server.poll(1000, Instant::now(), 64)?.native.rejected;
        assert!(server.poll_complete(CONTROL, Instant::now(), 64)?.is_none());
        if let Some(message) = server.poll_complete(APPLICATION, Instant::now(), 64)? {
            assert_eq!(message.id, allowed.id());
            completed = true;
        }
    }
    assert_eq!(server.peer_count(), 1);
    assert!(server.session_id(0).is_some());
    // The forwarding API retains the original link's deadline checks.
    let now = Instant::now();
    assert!(server
        .try_send_until(0, APPLICATION, &allowed, now, now)
        .is_err());
    assert!(server.try_send_until(0, APPLICATION, &allowed, now, now + Duration::from_secs(1))?);
    Ok(())
}
