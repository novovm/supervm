//! In-process network components use explicitly branded consensus fixtures.
//! Transport authentication is real Ed25519; this is not ML-DSA runtime or
//! independent validator-process acceptance.
use super::*;
use crate::native_pipeline::consensus::wire::{
    Context as VoteContext, Phase, Validator, ValidatorSet,
};
use ed25519_dalek::SigningKey;
use novovm_consensus::round_bft::test_vectors::{self, FixtureSigningKey};
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
    wall_base: u64,
}
pub(crate) fn codec() -> transport::DecodeLimits {
    transport::DecodeLimits {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
        message_bytes: 64 * 1024,
    }
}
impl Mesh {
    pub(crate) fn new(keys: &[FixtureSigningKey], domain: MessageDomain) -> Result<Self> {
        let transport_keys = (0..keys.len())
            .map(component_transport_key)
            .collect::<Vec<_>>();
        let bindings = split_bindings(&domain, keys, &transport_keys)?;
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
                    key: transport_keys[j].verifying_key().to_bytes(),
                    address: addresses[j],
                })
                .collect();
            let endpoint = NativeEndpoint::bind(
                socket,
                transport_keys[i].clone(),
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
                bindings.clone(),
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
        Self {
            channels,
            start,
            wall_base: 1000,
        }
        .establish()
    }

    fn establish(self) -> Result<Self> {
        let mut mesh = self;
        let start = mesh.start;
        // Configured responders retain a provisional transcript until ordinary
        // AEAD traffic proves the fresh secret. Production uses RequestDecision;
        // this transport-only fixture exchanges/drains an existing exact request.
        let mut probe_id = rand::random::<[u8; 32]>();
        probe_id[0] = 0xf5;
        let probe = transport::Message::RequestBody { body_id: probe_id };
        let probes = mesh
            .channels
            .iter()
            .map(|c| c.prepare(&probe))
            .collect::<Result<Vec<_>>>()?;
        let mut sent = mesh
            .channels
            .iter()
            .map(|c| vec![false; c.peer_count()])
            .collect::<Vec<_>>();
        let expected = sent.iter().map(Vec::len).sum::<usize>();
        let mut completed = 0;
        while completed < expected
            || mesh
                .channels
                .iter()
                .any(|c| (0..c.peer_count()).any(|p| c.session_id(p).is_none()))
        {
            ensure!(
                start.elapsed() < Duration::from_secs(10),
                "native mesh handshake deadline"
            );
            for (index, channel) in mesh.channels.iter_mut().enumerate() {
                for (peer, admitted) in sent[index].iter_mut().enumerate() {
                    if !*admitted {
                        *admitted = channel.try_send(peer, &probes[index], Instant::now())?;
                    }
                }
            }
            completed += mesh.pump()?;
            for channel in &mut mesh.channels {
                while let Some(event) = channel.poll_receive_lane(0, Instant::now(), 64)? {
                    match event {
                        ReceiveEvent::Message(value) => {
                            ensure!(value.message == probe, "unexpected handshake probe")
                        }
                        ReceiveEvent::Rejected { error, .. } => {
                            anyhow::bail!("handshake probe rejected: {error}")
                        }
                    }
                }
            }
        }
        Ok(mesh)
    }
    pub(crate) fn pump(&mut self) -> Result<usize> {
        let mut finished = 0;
        for c in &mut self.channels {
            let r = c.poll_io(
                self.wall_base + self.start.elapsed().as_millis() as u64,
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

// Test transport identity is independently generated, never truncated or
// inferred from a validator public key. This helper is not a production key API.
pub(crate) fn component_transport_key(index: usize) -> SigningKey {
    SigningKey::from_bytes(&[151 + index as u8; 32])
}

type SeparatedMaterial = (
    Vec<FixtureSigningKey>,
    Vec<SigningKey>,
    MessageDomain,
    VoteContext,
);

fn separated_material() -> Result<SeparatedMaterial> {
    let signing = (101..105)
        .map(|seed| FixtureSigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let transport = (151..155)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let validators = Arc::new(ValidatorSet::new_for_test(
        292,
        1,
        1,
        signing
            .iter()
            .map(|key| Validator::new(key.verifying_key().to_bytes(), 1))
            .collect::<Result<Vec<_>>>()?,
    )?);
    let context = VoteContext {
        chain_id: 292,
        genesis_config_commitment: [0x71; 32],
        protocol_commitment: [0x72; 32],
        epoch: 1,
        validator_set_hash: validators.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    Ok((
        signing,
        transport,
        MessageDomain {
            chain_id: 292,
            genesis: context.genesis_config_commitment,
            protocol: context.protocol_commitment,
            validators,
        },
        context,
    ))
}

fn split_bindings(
    domain: &MessageDomain,
    signing: &[FixtureSigningKey],
    transport: &[SigningKey],
) -> Result<ValidatorTransportBindings> {
    ValidatorTransportBindings::new(
        &domain.validators,
        signing.iter().zip(transport).map(|(signing, transport)| {
            (
                signing.verifying_key().to_bytes(),
                transport.verifying_key().to_bytes(),
            )
        }),
    )
}

fn split_lanes() -> [LaneLimits; 3] {
    std::array::from_fn(|_| LaneLimits {
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
    })
}

fn split_policy() -> NativeSendPolicy {
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
        ttl: Duration::from_secs(30),
    }
}

#[test]
fn validator_transport_bindings_are_bijective_separated_and_canonical() -> Result<()> {
    let (signing, transport, domain, _) = separated_material()?;
    let pairs = signing
        .iter()
        .zip(&transport)
        .map(|(s, t)| (s.verifying_key().to_bytes(), t.verifying_key().to_bytes()))
        .collect::<Vec<_>>();
    let bindings = ValidatorTransportBindings::new(&domain.validators, pairs.clone())?;
    let reversed =
        ValidatorTransportBindings::new(&domain.validators, pairs.iter().rev().cloned())?;
    assert_eq!(
        bindings.canonical_commitment(),
        reversed.canonical_commitment()
    );
    for (validator, transport) in &pairs {
        assert_eq!(bindings.transport_public_key(validator)?, *transport);
        assert_eq!(
            bindings.validator_for_peer(&peer_id_from_ed25519_public_key_v1(transport))?,
            Validator::new(validator.clone(), 1)?.id()
        );
    }
    assert!(bindings
        .validator_for_peer(&peer_id_from_ed25519_public_key_v1(
            &SigningKey::from_bytes(&[200; 32])
                .verifying_key()
                .to_bytes()
        ))
        .is_err());
    let mut swapped = pairs.clone();
    swapped[0].1 = pairs[1].1;
    swapped[1].1 = pairs[0].1;
    assert_ne!(
        bindings.canonical_commitment(),
        ValidatorTransportBindings::new(&domain.validators, swapped)?.canonical_commitment()
    );
    assert!(
        ValidatorTransportBindings::new(&domain.validators, pairs[..3].iter().cloned()).is_err()
    );
    for variant in 0..6 {
        let mut bad = pairs.clone();
        match variant {
            0 => bad[1].0 = bad[0].0.clone(),
            1 => bad[1].1 = bad[0].1,
            2 => bad[0].0 = bad[0].1.to_vec(),
            3 => bad[0].0 = bad[1].1.to_vec(),
            4 => {
                bad[0].0 = FixtureSigningKey::from_bytes(&[200; 32])
                    .verifying_key()
                    .to_bytes()
            }
            _ => bad[0].1 = [0; 32],
        }
        assert!(
            ValidatorTransportBindings::new(&domain.validators, bad).is_err(),
            "variant {variant}"
        );
    }
    let another_epoch =
        ValidatorSet::new_for_test(292, 2, 1, domain.validators.members().to_vec())?;
    assert!(bindings.validate_for(&another_epoch).is_err());
    assert!(bindings.transport_public_key(&[0; 32]).is_err());
    Ok(())
}

#[test]
fn original_transport_rejects_unbound_local_and_remote_identities() -> Result<()> {
    let (signing, transport, domain, _) = separated_material()?;
    let foreign = SigningKey::from_bytes(&[200; 32]);
    let bindings = split_bindings(&domain, &signing, &transport)?;
    for variant in 0..4 {
        let socket = UdpTransport::bind_for_chain(
            novovm_protocol::NodeId(1),
            "127.0.0.1:0",
            domain.chain_id,
        )?;
        let identity = if variant == 0 {
            foreign.clone()
        } else {
            transport[0].clone()
        };
        let peers = match variant {
            0 => Vec::new(),
            1 => vec![NativePeer {
                key: foreign.verifying_key().to_bytes(),
                address: "127.0.0.1:19001".parse()?,
            }],
            2 => vec![NativePeer {
                key: transport[0].verifying_key().to_bytes(),
                address: "127.0.0.1:19001".parse()?,
            }],
            _ => vec![
                NativePeer {
                    key: transport[1].verifying_key().to_bytes(),
                    address: "127.0.0.1:19001".parse()?,
                },
                NativePeer {
                    key: transport[1].verifying_key().to_bytes(),
                    address: "127.0.0.1:19002".parse()?,
                },
            ],
        };
        assert!(
            NativeChannel::from_transport(
                socket,
                identity,
                bindings.clone(),
                peers,
                0,
                domain.clone(),
                codec(),
                split_lanes(),
                split_policy(),
            )
            .is_err(),
            "variant {variant}"
        );
    }
    Ok(())
}

#[test]
fn original_udp_split_keys_preserve_signed_evidence_and_reject_transport_forgery() -> Result<()> {
    use ed25519_dalek::Signer;
    let (signing, transport_keys, domain, context) = separated_material()?;
    let bindings = split_bindings(&domain, &signing, &transport_keys)?;
    let transports = (0..signing.len())
        .map(|i| {
            UdpTransport::bind_for_chain(
                novovm_protocol::NodeId(i as u64 + 1),
                "127.0.0.1:0",
                domain.chain_id,
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let addresses = transports
        .iter()
        .map(UdpTransport::local_addr)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let start = Instant::now();
    let wall_base = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let channels = transports
        .into_iter()
        .enumerate()
        .map(|(i, transport)| {
            NativeChannel::from_transport(
                transport,
                transport_keys[i].clone(),
                bindings.clone(),
                (0..signing.len())
                    .filter(|&j| i != j)
                    .map(|j| NativePeer {
                        key: transport_keys[j].verifying_key().to_bytes(),
                        address: addresses[j],
                    })
                    .collect(),
                0,
                domain.clone(),
                codec(),
                split_lanes(),
                split_policy(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut mesh = Mesh {
        channels,
        start,
        wall_base,
    }
    .establish()?;
    for (local, channel) in mesh.channels.iter().enumerate() {
        for (peer, remote) in (0..signing.len()).filter(|&i| i != local).enumerate() {
            assert_eq!(
                channel.peer_id(peer)?,
                peer_id_from_ed25519_public_key_v1(
                    &transport_keys[remote].verifying_key().to_bytes()
                )
            );
            assert_eq!(
                channel.validator_for_peer(peer)?,
                Validator::new(signing[remote].verifying_key().to_bytes(), 1)?.id()
            );
        }
    }
    let vote = test_vectors::sign_vote(
        context,
        0,
        Phase::Prevote,
        Some([7; 32]),
        &domain.validators,
        &signing[0],
    )?;
    mesh.broadcast(0, &transport::Message::Vote(vote.clone()))?;
    // A valid vote can be relayed by a different configured transport owner;
    // the original validator's signature, not the forwarding key, authorizes it.
    let relayed = test_vectors::sign_vote(
        context,
        1,
        Phase::Prevote,
        Some([7; 32]),
        &domain.validators,
        &signing[0],
    )?;
    mesh.broadcast(1, &transport::Message::Vote(relayed))?;
    let mut forged = vote.clone();
    assert!(
        test_vectors::sign_vote(
            context,
            0,
            Phase::Prevote,
            Some([7; 32]),
            &domain.validators,
            &FixtureSigningKey::from_bytes(&[151; 32])
        )
        .is_err(),
        "transport key must not acquire validator signing membership"
    );
    let transport_signature = transport_keys[0]
        .sign(b"transport possession is not a vote")
        .to_bytes();
    forged.signature = vec![0; novovm_consensus::round_bft::crypto::MLDSA65_SIGNATURE_BYTES];
    forged.signature[..transport_signature.len()].copy_from_slice(&transport_signature);
    assert!(mesh.channels[0]
        .prepare(&transport::Message::Vote(forged.clone()))
        .is_err());
    // Authenticated hostile peer bypasses local preparation only. Remote
    // qualification must still reject, and then keep the valid session usable.
    let object = Arc::new(OutgoingMessage::with_profile(
        transport::fragment_domain(
            context.chain_id,
            context.genesis_config_commitment,
            context.protocol_commitment,
        ),
        transport::encode(&transport::Message::Vote(forged), codec())?,
        codec().message_bytes,
        FragmentProfile::NativeUdpV2,
    )?);
    assert!(mesh.channels[0]
        .link
        .try_send(0, STREAMS[0], &object, Instant::now())?);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut rejected = false;
    let mut completed = 0;
    while !rejected || completed == 0 {
        ensure!(
            Instant::now() < deadline,
            "split-key hostile object deadline"
        );
        completed += mesh.pump()?;
        if let Some(event) = mesh.channels[1].poll_receive_lane(0, Instant::now(), 64)? {
            assert!(matches!(event, ReceiveEvent::Rejected { .. }));
            rejected = true;
        }
    }
    let after_rejection = test_vectors::sign_vote(
        context,
        2,
        Phase::Prevote,
        Some([7; 32]),
        &domain.validators,
        &signing[0],
    )?;
    mesh.broadcast(0, &transport::Message::Vote(after_rejection))?;
    Ok(())
}

fn setup() -> Result<(Mesh, Vec<FixtureSigningKey>, VoteContext)> {
    let keys = (101..105)
        .map(|seed| FixtureSigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let set = Arc::new(ValidatorSet::new_for_test(
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
