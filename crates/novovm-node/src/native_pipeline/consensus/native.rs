//! Original-node protocol over the native NovoRUDP object owner. No Relay,
//! transport-owned chain head, signer, business executor or consensus rules.
//! Encoding, assembly and evidence verification belong to the assembly owner;
//! `poll_io` only progresses bounded native I/O. Its DONE is transport custody,
//! never transaction admission, a durable vote, or publication.

pub use super::message_evidence::{MessageDomain, VerifiedEvidence};
use super::{message_evidence, transport};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
#[cfg(test)]
use novovm_network::novorudp::{endpoint::NativeEndpoint, object_link::NativeObjectLink};
use novovm_network::{
    novorudp::{
        endpoint::{NativeHandshakeBudget, NativePeer},
        fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits},
        object_link::{NativeLaneConfig, NativeLinkPoll, NativeSendLimits, NativeSendPolicy},
    },
    product_overlay::peer_id_from_ed25519_public_key_v1,
    AuthenticatedUdpIngressV1, NativeUdpIngressConfigV1, NativeUdpPollV1, UdpTransport,
};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
mod query;
pub(crate) use query::lane as query_lane;
use query::{QueryDelivery, QueryInbox};

const STREAMS: [u64; 3] = [
    u64::from_be_bytes(*b"NVBFT001"),
    u64::from_be_bytes(*b"NVBODY01"),
    u64::from_be_bytes(*b"NVINPUT1"),
];
/// Application inputs have an independent reassembly budget and cannot occupy
/// validator control/body/input lanes. Payload is the existing APFL message.
pub const APPLICATION_STREAM: u64 = u64::from_be_bytes(*b"NVAPP001");

#[derive(Clone)]
pub struct LaneLimits {
    pub receive: ReassemblyLimits,
    pub send: NativeSendLimits,
}

/// Immutable bytes can be retried without re-encoding or re-hashing. The
/// caller budgets its retained handles; link admission separately accounts
/// every destination until DONE or an explicit returned expiry.
#[derive(Clone)]
pub struct PreparedMessage {
    owner: Arc<()>,
    stream: u64,
    object: Arc<OutgoingMessage>,
}
impl PreparedMessage {
    pub fn byte_len(&self) -> usize {
        self.object.byte_len()
    }
    pub fn fragment_id(&self) -> [u8; 32] {
        self.object.id()
    }
    pub(crate) fn lane(&self) -> usize {
        STREAMS
            .iter()
            .position(|stream| *stream == self.stream)
            .expect("prepared native lane")
    }
}

pub struct ReceivedMessage {
    pub peer: String,
    pub fragment_id: [u8; 32],
    pub message: transport::Message,
    pub evidence: VerifiedEvidence,
}

pub enum ReceiveEvent {
    Message(Box<ReceivedMessage>),
    /// A peer's invalid object must not stop other peers or clear a signer.
    Rejected {
        peer: String,
        error: String,
    },
}

/// Counts legacy transport observations, not authenticated transaction admissions.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeIngressObservation {
    pub protocol_observed: u64,
    pub protocol_rejected: u64,
}

#[cfg(not(test))]
type ChannelLink = AuthenticatedUdpIngressV1;

pub struct NativeChannel {
    link: ChannelLink,
    domain: MessageDomain,
    codec: transport::DecodeLimits,
    owner: Arc<()>,
    receive_turn: usize,
    applications: bool,
    query_replies: std::collections::VecDeque<QueryDelivery>,
    query_inbox: QueryInbox,
    ingress_observation: NativeIngressObservation,
}
impl NativeChannel {
    // Explicit deployment budgets and domains remain distinct at owner transfer.
    #[allow(clippy::too_many_arguments)]
    pub fn from_transport(
        transport: UdpTransport,
        identity: SigningKey,
        peers: Vec<NativePeer>,
        client_slots: usize,
        domain: MessageDomain,
        codec: transport::DecodeLimits,
        lanes: [LaneLimits; 3],
        policy: NativeSendPolicy,
    ) -> Result<Self> {
        ensure!(
            transport.chain_id_v1() == domain.chain_id,
            "UDP transport chain differs from pinned native domain"
        );
        let applications = client_slots > 0;
        let configs = Self::lane_configs(&domain, codec, lanes, applications)?;
        let known = Self::validator_peers(&domain);
        ensure!(
            known.contains(&peer_id_from_ed25519_public_key_v1(
                &identity.verifying_key().to_bytes()
            )),
            "native local identity is outside pinned validator set"
        );
        ensure!(
            peers
                .iter()
                .all(|peer| known.contains(&peer_id_from_ed25519_public_key_v1(&peer.key))),
            "native peer is outside pinned validator set"
        );
        let wall_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        let link = transport.into_authenticated_objects_v1(
            NativeUdpIngressConfigV1 {
                identity,
                peers,
                client_slots,
                observe_legacy_protocol: false,
                client_streams: if applications {
                    vec![APPLICATION_STREAM, novovm_protocol::native_query::STREAM]
                } else {
                    Vec::new()
                },
                handshake: NativeHandshakeBudget {
                    lifetime: Duration::from_secs(30),
                    retry: Duration::from_millis(50),
                },
                lanes: configs,
                send: policy,
                bytes_per_minute: None,
            },
            wall_ms,
            Instant::now(),
        )?;
        #[cfg(test)]
        let link = ChannelLink::Original(link);
        Ok(Self::with_link(link, domain, codec, applications))
    }

    #[cfg(test)]
    pub fn new(
        endpoint: NativeEndpoint,
        domain: MessageDomain,
        codec: transport::DecodeLimits,
        lanes: [LaneLimits; 3],
        policy: NativeSendPolicy,
    ) -> Result<Self> {
        let applications = endpoint.accepts_clients();
        let configs = Self::lane_configs(&domain, codec, lanes, applications)?;
        let known = Self::validator_peers(&domain);
        for peer in 0..endpoint.peer_count() {
            ensure!(
                known.contains(endpoint.peer_id(peer)?),
                "native peer is outside pinned validator set"
            );
        }
        Ok(Self::with_link(
            ChannelLink::HistoricalFixture(NativeObjectLink::new(endpoint, configs, policy)?),
            domain,
            codec,
            applications,
        ))
    }

    fn validator_peers(domain: &MessageDomain) -> BTreeSet<String> {
        domain
            .validators
            .members()
            .iter()
            .map(|v| peer_id_from_ed25519_public_key_v1(v.public_key()))
            .collect()
    }

    fn lane_configs(
        domain: &MessageDomain,
        codec: transport::DecodeLimits,
        lanes: [LaneLimits; 3],
        applications: bool,
    ) -> Result<Vec<NativeLaneConfig>> {
        ensure!(
            domain.chain_id == domain.validators.chain_id()
                && domain.genesis != [0; 32]
                && domain.protocol != [0; 32],
            "invalid pinned native protocol domain"
        );
        transport::encode(&transport::Message::RequestBody { body_id: [1; 32] }, codec)?;
        let fragment_domain =
            transport::fragment_domain(domain.chain_id, domain.genesis, domain.protocol);
        let application_limits = lanes[2].clone();
        let mut configs = lanes
            .into_iter()
            .zip(STREAMS)
            .map(|(limits, stream)| {
                ensure!(
                    limits.receive.max_message_bytes <= codec.message_bytes,
                    "native lane exceeds protocol decoder budget"
                );
                Ok(NativeLaneConfig {
                    stream,
                    domain: fragment_domain,
                    receive: limits.receive,
                    send: limits.send,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if applications {
            configs.push(query::lane(fragment_domain));
            configs.push(NativeLaneConfig {
                stream: APPLICATION_STREAM,
                domain: fragment_domain,
                receive: application_limits.receive,
                send: application_limits.send,
            });
        }
        Ok(configs)
    }

    fn with_link(
        link: ChannelLink,
        domain: MessageDomain,
        codec: transport::DecodeLimits,
        applications: bool,
    ) -> Self {
        Self {
            link,
            domain,
            codec,
            owner: Arc::new(()),
            receive_turn: 0,
            applications,
            query_replies: Default::default(),
            query_inbox: QueryInbox::default(),
            ingress_observation: NativeIngressObservation::default(),
        }
    }
    pub fn ingress_observation(&self) -> NativeIngressObservation {
        self.ingress_observation
    }
    pub fn peer_count(&self) -> usize {
        self.link.peer_count()
    }
    pub fn peer_id(&self, peer: usize) -> Result<&str> {
        self.link.peer_id(peer)
    }
    pub fn session_id(&self, peer: usize) -> Option<[u8; 16]> {
        self.link.session_id(peer)
    }

    /// Full-body work, not the consensus tick. Reuses the reviewed canonical
    /// message and evidence contracts; only the fragment profile is native.
    pub fn prepare(&self, message: &transport::Message) -> Result<PreparedMessage> {
        message_evidence::verify(message, &self.domain)?;
        let bytes = transport::encode(message, self.codec)?;
        let stream = STREAMS[transport::message_lane(&bytes)?];
        let object = Arc::new(OutgoingMessage::with_profile(
            transport::fragment_domain(
                self.domain.chain_id,
                self.domain.genesis,
                self.domain.protocol,
            ),
            bytes,
            self.codec.message_bytes,
            FragmentProfile::NativeUdpV2,
        )?);
        Ok(PreparedMessage {
            owner: self.owner.clone(),
            stream,
            object,
        })
    }
    /// False is backpressure/handshake pending, not acceptance or loss.
    pub fn try_send(
        &mut self,
        peer: usize,
        message: &PreparedMessage,
        now: Instant,
    ) -> Result<bool> {
        ensure!(
            Arc::ptr_eq(&message.owner, &self.owner),
            "foreign native channel prepared handle"
        );
        self.link
            .try_send(peer, message.stream, &message.object, now)
    }
    pub fn poll_io(
        &mut self,
        wall_ms: u64,
        now: Instant,
        quantum: usize,
    ) -> Result<NativeLinkPoll> {
        let NativeUdpPollV1 {
            native,
            protocol,
            rejected_protocol,
        } = self.link.poll(wall_ms, now, quantum)?;
        // The original UdpTransport has already applied its protocol observer.
        // These bounded observations do not acquire wallet/validator authority.
        self.ingress_observation.protocol_observed = self
            .ingress_observation
            .protocol_observed
            .saturating_add(protocol.len() as u64);
        self.ingress_observation.protocol_rejected = self
            .ingress_observation
            .protocol_rejected
            .saturating_add(rejected_protocol as u64);
        drop(protocol);
        self.poll_query_deliveries(native, now)
    }
    /// The caller must have a bounded free output slot BEFORE this call.
    /// Separate lanes are polled fairly; large body hashing never turns into
    /// a requirement to finish a body before receiving a control message.
    pub fn poll_receive(&mut self, now: Instant, quantum: usize) -> Result<Option<ReceiveEvent>> {
        let lane = self.receive_turn;
        self.receive_turn = (lane + 1) % STREAMS.len();
        self.poll_receive_lane(lane, now, quantum)
    }

    /// A full application input lane must not hold control/body completion.
    /// Caller reserves a bounded handoff independently for each lane.
    pub fn poll_receive_lane(
        &mut self,
        lane: usize,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<ReceiveEvent>> {
        ensure!(lane < STREAMS.len(), "invalid native receive lane");
        self.receive_stream(STREAMS[lane], lane, false, now, quantum)
    }

    pub fn poll_application(
        &mut self,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<ReceiveEvent>> {
        if !self.applications {
            return Ok(None);
        }
        self.receive_stream(APPLICATION_STREAM, 2, true, now, quantum)
    }

    fn receive_stream(
        &mut self,
        stream: u64,
        lane: usize,
        application: bool,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<ReceiveEvent>> {
        let Some(complete) = self.link.poll_complete(stream, now, quantum)? else {
            return Ok(None);
        };
        let parsed = (|| -> Result<_> {
            let size = complete
                .chunks
                .iter()
                .try_fold(0usize, |n, c| n.checked_add(c.len()))
                .filter(|n| *n <= self.codec.message_bytes)
                .context("native completed message exceeds codec budget")?;
            // The prefix fits in the first native fragment. Check its lane
            // before allocating/decoding a potentially large body.
            ensure!(
                transport::message_lane(
                    complete.chunks.first().context("empty completed message")?
                )? == lane,
                "protocol message arrived on wrong native lane"
            );
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(size)?;
            for chunk in &complete.chunks {
                bytes.extend_from_slice(chunk);
            }
            let message = transport::decode(&bytes, self.codec)?;
            ensure!(
                !application || matches!(&message, transport::Message::ApflTransactions { .. }),
                "application lane accepts only signed APFL transaction inputs"
            );
            let evidence = message_evidence::verify(&message, &self.domain)?;
            Ok((message, evidence))
        })();
        Ok(Some(match parsed {
            Ok((message, evidence)) => ReceiveEvent::Message(Box::new(ReceivedMessage {
                peer: complete.peer,
                fragment_id: complete.id,
                message,
                evidence,
            })),
            Err(error) => ReceiveEvent::Rejected {
                peer: complete.peer,
                error: error.to_string().chars().take(512).collect(),
            },
        }))
    }
}

/// Only old unit fixtures may own a standalone NativeObjectLink. Production
/// uses the original UdpTransport owner directly, without this adapter enum.
#[cfg(test)]
enum ChannelLink {
    Original(AuthenticatedUdpIngressV1),
    HistoricalFixture(NativeObjectLink),
}
#[cfg(test)]
impl ChannelLink {
    fn peer_count(&self) -> usize {
        match self {
            Self::Original(link) => link.peer_count(),
            Self::HistoricalFixture(link) => link.peer_count(),
        }
    }
    fn peer_id(&self, peer: usize) -> Result<&str> {
        match self {
            Self::Original(link) => link.peer_id(peer),
            Self::HistoricalFixture(link) => link.peer_id(peer),
        }
    }
    fn session_id(&self, peer: usize) -> Option<[u8; 16]> {
        match self {
            Self::Original(link) => link.session_id(peer),
            Self::HistoricalFixture(link) => link.session_id(peer),
        }
    }
    fn try_send(
        &mut self,
        peer: usize,
        stream: u64,
        body: &Arc<OutgoingMessage>,
        now: Instant,
    ) -> Result<bool> {
        match self {
            Self::Original(link) => link.try_send(peer, stream, body, now),
            Self::HistoricalFixture(link) => link.try_send(peer, stream, body, now),
        }
    }
    fn try_send_until(
        &mut self,
        peer: usize,
        stream: u64,
        body: &Arc<OutgoingMessage>,
        now: Instant,
        deadline: Instant,
    ) -> Result<bool> {
        match self {
            Self::Original(link) => link.try_send_until(peer, stream, body, now, deadline),
            Self::HistoricalFixture(link) => link.try_send_until(peer, stream, body, now, deadline),
        }
    }
    fn poll(&mut self, wall_ms: u64, now: Instant, quantum: usize) -> Result<NativeUdpPollV1> {
        match self {
            Self::Original(link) => link.poll(wall_ms, now, quantum),
            Self::HistoricalFixture(link) => Ok(NativeUdpPollV1 {
                native: link.poll(wall_ms, now, quantum)?,
                protocol: Vec::new(),
                rejected_protocol: 0,
            }),
        }
    }
    fn poll_complete(
        &mut self,
        stream: u64,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<novovm_network::novorudp::fragments::CompletedMessage>> {
        match self {
            Self::Original(link) => link.poll_complete(stream, now, quantum),
            Self::HistoricalFixture(link) => link.poll_complete(stream, now, quantum),
        }
    }
}

#[cfg(test)]
pub(super) mod tests;
