use super::*;
use crate::native_pipeline::consensus::wire::{
    Context as VoteContext, Phase, Validator, ValidatorSet,
};
use ed25519_dalek::SigningKey;
use novovm_consensus::round_bft::test_vectors;
use novovm_network::novorudp::{
    endpoint::{NativeHandshakeBudget, NativePeer},
    object_sender::ObjectSendState,
    NovoRudpPacingProfile, NovoRudpWindowConfig,
};
use std::{net::UdpSocket, time::Duration};

#[test]
fn io_hint_ignores_duplicate_rejected_and_stalled_io() {
    let mut hint = IoProgressHint::default();
    let stalled = NativeLinkPoll {
        rejected: 4,
        backpressured: 2,
        duplicate_fragments: 16,
        feedback_sent: 16,
        handshake_send_failures: 1,
        receive_errors: 1,
        send_failures: 1,
        retired_inbound_messages: 1,
        ..Default::default()
    };
    for _ in 0..128 {
        hint.observe(&stalled);
        hint.observe(&NativeLinkPoll::default());
    }
    assert_eq!(hint.0, 0, "unchanged pending work must become idle");
}

#[test]
fn io_hint_tracks_new_fragments_sends_and_query_completion() -> Result<()> {
    use novovm_network::novorudp::object_link::NativeSendCompletion;
    let mut hint = IoProgressHint::default();
    hint.observe(&NativeLinkPoll {
        accepted_fragments: 3,
        ..Default::default()
    });
    assert_eq!(hint.0, 1);
    hint.observe(&NativeLinkPoll {
        data_sent: 2,
        ..Default::default()
    });
    assert_eq!(hint.0, 2);
    hint.observe(&NativeLinkPoll::default());
    assert_eq!(hint.0, 2, "sending once does not sustain a busy hint");
    hint.observe(&NativeLinkPoll {
        completed: vec![NativeSendCompletion {
            peer: 0,
            stream: novovm_protocol::native_query::STREAM,
            state: ObjectSendState::Delivered,
            body: Arc::new(OutgoingMessage::with_profile(
                [1; 32],
                vec![7],
                1024,
                FragmentProfile::NativeUdpV2,
            )?),
        }],
        ..Default::default()
    });
    assert_eq!(hint.0, 3, "query DONE must be visible before filtering");
    hint.observe(&NativeLinkPoll::default());
    assert_eq!(hint.0, 3);
    Ok(())
}

#[test]
fn io_hint_wrap_is_a_change_token_not_a_permit_counter() {
    let mut hint = IoProgressHint(u64::MAX);
    let before = hint.0;
    hint.advance();
    assert_ne!(hint.0, before);
    assert_eq!(hint.0, 0);
    hint.observe(&NativeLinkPoll::default());
    assert_eq!(hint.0, 0);
}

pub(crate) struct Mesh {
    pub channels: Vec<NativeChannel>,
    start: Instant,
}
pub(crate) fn codec() -> transport::DecodeLimits {
    transport::DecodeLimits {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
        message_bytes: 16_384,
    }
}
impl Mesh {
    pub(crate) fn new(keys: &[SigningKey], domain: MessageDomain) -> Result<Self> {
        let sockets = (0..keys.len())
            .map(|_| UdpSocket::bind("127.0.0.1:0"))
            .collect::<std::io::Result<Vec<_>>>()?;
        let addresses = sockets
            .iter()
            .map(UdpSocket::local_addr)
            .collect::<std::io::Result<Vec<_>>>()?;
        let start = Instant::now();
        let mut channels = Vec::new();
        for (i, socket) in sockets.into_iter().enumerate() {
            let peers = (0..keys.len())
                .filter(|&j| j != i)
                .map(|j| NativePeer {
                    key: keys[j].verifying_key().to_bytes(),
                    address: addresses[j],
                })
                .collect();
            let endpoint = NativeEndpoint::bind(
                socket,
                keys[i].clone(),
                peers,
                NativeHandshakeBudget {
                    lifetime: Duration::from_secs(30),
                    retry: Duration::from_millis(10),
                },
                1000,
                start,
            )?;
            let lanes = std::array::from_fn(|_| LaneLimits {
                receive: ReassemblyLimits {
                    max_message_bytes: codec().message_bytes,
                    messages: 12,
                    bytes: 12 * codec().message_bytes,
                    peer_messages: 4,
                    peer_bytes: 4 * codec().message_bytes,
                    ttl: Duration::from_secs(30),
                },
                send: NativeSendLimits {
                    messages: 12,
                    bytes: 12 * codec().message_bytes,
                    peer_messages: 4,
                    peer_bytes: 4 * codec().message_bytes,
                },
            });
            channels.push(NativeChannel::new(
                endpoint,
                domain.clone(),
                codec(),
                lanes,
                NativeSendPolicy {
                    window: NovoRudpWindowConfig {
                        packet_copies: 1,
                        tail_packet_copies: 1,
                        batch_size: 16,
                        batch_pause_ms: 0,
                        ack_wait_ms: 10,
                        no_progress_backoff: false,
                        ..Default::default()
                    },
                    pacing: NovoRudpPacingProfile {
                        packet_copies: 1,
                        tail_packet_copies: 1,
                        batch_size: 16,
                        batch_pause_ms: 0,
                        ack_wait_ms: 10,
                        no_progress_backoff: false,
                    },
                    ttl: Duration::from_secs(30),
                },
            )?);
        }
        let mut mesh = Self { channels, start };
        while mesh
            .channels
            .iter()
            .any(|c| (0..c.peer_count()).any(|p| c.session_id(p).is_none()))
        {
            ensure!(
                start.elapsed() < Duration::from_secs(10),
                "native mesh handshake deadline"
            );
            mesh.pump()?;
        }
        Ok(mesh)
    }
    pub(crate) fn pump(&mut self) -> Result<usize> {
        let mut finished = 0;
        for c in &mut self.channels {
            let r = c.poll_io(
                1000 + self.start.elapsed().as_millis() as u64,
                Instant::now(),
                64,
            )?;
            ensure!(
                r.rejected == 0 && r.backpressured == 0,
                "valid native fixture rejected/backpressured packets"
            );
            for done in r.completed {
                ensure!(
                    done.state == ObjectSendState::Delivered,
                    "native fixture send expired"
                );
                finished += 1;
            }
        }
        Ok(finished)
    }
    /// A real socket broadcast, including independent remote verification.
    /// The local origin does not send its own message through UDP.
    pub(crate) fn broadcast(
        &mut self,
        sender: usize,
        message: &transport::Message,
    ) -> Result<Vec<transport::Message>> {
        let prepared = self.channels[sender].prepare(message)?;
        for peer in 0..self.channels[sender].peer_count() {
            ensure!(
                self.channels[sender].try_send(peer, &prepared, Instant::now())?,
                "fixture send backpressure"
            );
        }
        let mut result = vec![None; self.channels.len()];
        result[sender] = Some(message.clone());
        let mut done = 0;
        let deadline = Instant::now() + Duration::from_secs(10);
        while result.iter().any(Option::is_none) || done < self.channels.len() - 1 {
            ensure!(
                Instant::now() < deadline,
                "native protocol broadcast deadline"
            );
            done += self.pump()?;
            for (i, c) in self.channels.iter_mut().enumerate() {
                if let Some(event) = c.poll_receive(Instant::now(), 64)? {
                    match event {
                        ReceiveEvent::Message(received) => {
                            ensure!(
                                i != sender && result[i].is_none(),
                                "duplicate native application handoff"
                            );
                            ensure!(
                                received.fragment_id == prepared.fragment_id(),
                                "wrong object completed"
                            );
                            ensure!(
                                &received.message == message,
                                "native codec changed signed content"
                            );
                            result[i] = Some(received.message);
                        }
                        ReceiveEvent::Rejected { error, .. } => {
                            anyhow::bail!("valid native message rejected: {error}")
                        }
                    }
                }
            }
            std::thread::yield_now();
        }
        Ok(result.into_iter().map(Option::unwrap).collect())
    }
}

fn setup() -> Result<(Mesh, Vec<SigningKey>, VoteContext)> {
    let keys = (101..105)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let set = Arc::new(ValidatorSet::new(
        292,
        1,
        1,
        keys.iter()
            .map(|k| Validator::new(k.verifying_key().to_bytes(), 1))
            .collect::<Result<_>>()?,
    )?);
    let context = VoteContext {
        chain_id: 292,
        genesis_config_commitment: [0x71; 32],
        protocol_commitment: [0x72; 32],
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let mesh = Mesh::new(
        &keys,
        MessageDomain {
            chain_id: 292,
            genesis: [0x71; 32],
            protocol: [0x72; 32],
            validators: set,
        },
    )?;
    Ok((mesh, keys, context))
}

#[test]
fn unconsumed_native_input_lane_does_not_block_control_completion() -> Result<()> {
    let (mut mesh, _, context) = setup()?;
    let input = transport::Message::Transactions {
        scope: transport::TransactionsScope {
            chain_id: context.chain_id,
            genesis: context.genesis_config_commitment,
            protocol: context.protocol_commitment,
            epoch: context.epoch,
            validator_set_hash: context.validator_set_hash,
            session: [8; 32],
            sequence: 1,
        },
        raw_transactions: vec![vec![5; 1024]; 8],
    };
    let input = mesh.channels[0].prepare(&input)?;
    let control =
        mesh.channels[0].prepare(&transport::Message::RequestBody { body_id: [9; 32] })?;
    assert!(mesh.channels[0].try_send(0, &input, Instant::now())?);
    assert!(mesh.channels[0].try_send(0, &control, Instant::now())?);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        ensure!(
            Instant::now() < deadline,
            "control blocked behind unconsumed input"
        );
        mesh.pump()?;
        // Deliberately do not poll lane 2; it grants no wallet admission.
        if let Some(ReceiveEvent::Message(message)) =
            mesh.channels[1].poll_receive_lane(0, Instant::now(), 64)?
        {
            assert_eq!(message.fragment_id, control.fragment_id());
            break;
        }
    }
    assert!(mesh.channels[1]
        .poll_receive_lane(3, Instant::now(), 64)
        .is_err());
    loop {
        ensure!(Instant::now() < deadline, "retained input disappeared");
        mesh.pump()?;
        if let Some(ReceiveEvent::Message(message)) =
            mesh.channels[1].poll_receive_lane(2, Instant::now(), 64)?
        {
            assert_eq!(message.fragment_id, input.fragment_id());
            break;
        }
    }
    Ok(())
}

#[test]
fn original_signed_vote_crosses_native_channel_without_relabeling() -> Result<()> {
    let (mut mesh, keys, context) = setup()?;
    let vote = test_vectors::sign_vote(
        context,
        0,
        Phase::Prevote,
        Some([7; 32]),
        &mesh.channels[0].domain.validators,
        &keys[0],
    )?;
    let msg = transport::Message::Vote(vote);
    mesh.broadcast(0, &msg)?;
    let prepared = mesh.channels[0].prepare(&msg)?;
    assert!(mesh.channels[1]
        .try_send(0, &prepared, Instant::now())
        .is_err());
    Ok(())
}

#[test]
fn hostile_signed_domain_wrong_lane_and_bad_signature_cannot_become_evidence() -> Result<()> {
    let (mut mesh, keys, context) = setup()?;
    let mut wrong = context;
    wrong.genesis_config_commitment = [9; 32];
    let foreign = transport::Message::Vote(test_vectors::sign_vote(
        wrong,
        0,
        Phase::Prevote,
        None,
        &mesh.channels[0].domain.validators,
        &keys[0],
    )?);
    assert!(mesh.channels[0].prepare(&foreign).is_err());
    let vote = test_vectors::sign_vote(
        context,
        0,
        Phase::Prevote,
        None,
        &mesh.channels[0].domain.validators,
        &keys[0],
    )?;
    let mut bad = vote.clone();
    bad.signature[0] ^= 1;
    assert!(mesh.channels[0]
        .prepare(&transport::Message::Vote(bad.clone()))
        .is_err());
    // Bypass only LOCAL preparation to emulate an authenticated hostile peer.
    // Remote message qualification still rejects its lane, signature or domain.
    for (message, lane) in [
        (foreign, 0),
        (transport::Message::Vote(bad), 0),
        (transport::Message::Vote(vote.clone()), 1),
    ] {
        let bytes = transport::encode(&message, codec())?;
        let object = Arc::new(OutgoingMessage::with_profile(
            transport::fragment_domain(
                context.chain_id,
                context.genesis_config_commitment,
                context.protocol_commitment,
            ),
            bytes,
            codec().message_bytes,
            FragmentProfile::NativeUdpV2,
        )?);
        assert!(mesh.channels[0]
            .link
            .try_send(0, STREAMS[lane], &object, Instant::now())?);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut rejected = false;
        let mut done = 0;
        while !rejected || done == 0 {
            ensure!(Instant::now() < deadline, "hostile object test deadline");
            done += mesh.pump()?;
            if let Some(event) = mesh.channels[1].poll_receive(Instant::now(), 64)? {
                assert!(matches!(event, ReceiveEvent::Rejected { .. }));
                rejected = true;
            }
        }
    }
    // Invalid messages do not tear down the healthy bound session.
    mesh.broadcast(0, &transport::Message::Vote(vote))?;
    Ok(())
}
