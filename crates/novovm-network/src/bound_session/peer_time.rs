//! One session-bound relative-time observation on the explicit NVMBOX08 entry.
//! No new request RTT, socket, task, account or UTC authority. The server sends
//! one protected sample after Finished; the original channel (and its consumed
//! AEAD sequence) then passes unchanged into the existing native mailbox pump.
//! Waiting for this frame can add latency versus the V7 client Finished return.
use super::*;
use crate::{
    network_time::{measure_peer_clock_v1, ClockAnchorV1, PeerTimeSampleV1},
    novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0},
    product_overlay::{binary, peer_id_from_ed25519_public_key_v1},
    transport_binding::{session_clock::SessionClockDeadlineV2, TransportBindingErrorV1},
};
use anyhow::{ensure, Result};
use novovm_protocol::{
    apfl_batch::{self, Limits},
    apfl_column::Column,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

const SELECTOR: &[u8; 8] = b"NVMBOX08";
const SCHEMA: u64 = u64::from_be_bytes(*b"NVTIME01");
const STREAM: u64 = SCHEMA;
const WIRE_LIMIT: usize = 1200;
const LIMITS: Limits = Limits {
    rows: 1,
    columns: 4,
    field_bytes: 8,
    wire_bytes: 512,
    expanded_bytes: 25,
};

/// Kept by the existing client owner from before its first selector write.
/// Consuming this value makes the one-time reply state explicit.
pub struct PendingPeerTimeV8 {
    clock: Option<ClockAnchorV1>,
    deadline: SessionClockDeadlineV2,
}
impl PendingPeerTimeV8 {
    /// The selector has already consumed part of this same operation budget.
    pub fn deadline(&self) -> Instant {
        self.deadline.instant()
    }
}

/// Production client entry: selection, V2 Finished and the optional observation
/// share one immutable deadline. Return the SAME channel for native adoption.
pub async fn authenticate_mailbox_native_v8<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    expected_peer: [u8; 32],
    original_deadline: Instant,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<(BoundSecureChannelV1, AuthenticatedPeerTimeV1), BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    let pending = request_mailbox_native_v8(stream, original_deadline).await?;
    let mut channel = super::session_clock::authenticate_structured_bound_session_v2_observed(
        stream,
        identity,
        expected_peer,
        true,
        pending.deadline(),
        replay,
        observer,
    )
    .await?;
    let observation = receive_peer_time_v8(stream, &mut channel, pending, expected_peer).await?;
    Ok((channel, observation))
}

/// An authenticated declaration is not proof of accurate global UTC.
pub enum PeerTimeObservationV1 {
    Candidate(PeerTimeSampleV1),
    PeerClockUnavailable,
    LocalClockUnavailable,
    InvalidMeasurement,
}
pub struct AuthenticatedPeerTimeV1 {
    pub peer_identity_public_key: [u8; 32],
    pub observation: PeerTimeObservationV1,
}

/// Select V8 only. Unsupported peers fail; no legacy protocol is retried.
/// One immutable local deadline covers selector, V2 auth and the time frame.
pub async fn request_mailbox_native_v8(
    stream: &mut CarrierStreamV1<'_>,
    original_deadline: Instant,
) -> Result<PendingPeerTimeV8, BoundSessionError> {
    request_with_anchor(stream, original_deadline, ClockAnchorV1::capture().ok()).await
}
async fn request_with_anchor(
    stream: &mut CarrierStreamV1<'_>,
    original_deadline: Instant,
    clock: Option<ClockAnchorV1>,
) -> Result<PendingPeerTimeV8, BoundSessionError> {
    let deadline = SessionClockDeadlineV2::new(original_deadline).map_err(local)?;
    let exchange = async {
        deadline.check().map_err(local)?;
        stream
            .write_frame(SELECTOR)
            .await
            .map_err(BoundSessionError::Transport)?;
        let selected = stream
            .read_frame()
            .await
            .map_err(BoundSessionError::Transport)?;
        if selected != SELECTOR {
            return Err(authentication(anyhow::anyhow!(
                "native session-clock selector rejected"
            )));
        }
        check_live(stream, deadline)?;
        Ok(PendingPeerTimeV8 { clock, deadline })
    };
    tokio::time::timeout_at(deadline.instant().into(), exchange)
        .await
        .map_err(|_| local(TransportBindingErrorV1::HandshakeDeadline))?
}

fn check_live(
    stream: &CarrierStreamV1<'_>,
    deadline: SessionClockDeadlineV2,
) -> Result<(), BoundSessionError> {
    deadline.check().map_err(local)?;
    stream
        .selected_path()
        .map_err(BoundSessionError::Transport)?;
    deadline.check().map_err(local)
}
fn check_channel(
    stream: &CarrierStreamV1<'_>,
    channel: &BoundSecureChannelV1,
) -> Result<(), BoundSessionError> {
    if !channel.uses_session_clock_v2() {
        return Err(authentication(anyhow::anyhow!(
            "session-clock V2 channel required"
        )));
    }
    channel
        .verify_transport_binding(&stream.binding().map_err(authentication)?)
        .map_err(authentication)
}
fn context(channel: &BoundSecureChannelV1) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"novonet.session-time.observation.v1\0");
    hash.update(channel.session_id());
    hash.finalize().into()
}
fn payload(channel: &BoundSecureChannelV1, times: Option<(u64, u64, u64)>) -> Result<Vec<u8>> {
    let (tag, receive, send, residence) = times.map_or((0u8, 0, 0, 0), |(a, b, c)| (1, a, b, c));
    let tag = [tag];
    let receive = receive.to_le_bytes();
    let send = send.to_le_bytes();
    let residence = residence.to_le_bytes();
    let fields: [&[u8]; 4] = [&tag, &receive, &send, &residence];
    let columns: Vec<_> = fields
        .iter()
        .map(|f| Column::collect(std::iter::once(*f)))
        .collect();
    apfl_batch::encode(SCHEMA, context(channel), 1, &columns, LIMITS).map_err(anyhow::Error::msg)
}
fn decode_payload(channel: &BoundSecureChannelV1, bytes: &[u8]) -> Result<Option<(u64, u64, u64)>> {
    let batch = apfl_batch::decode(bytes, SCHEMA, LIMITS).map_err(anyhow::Error::msg)?;
    ensure!(
        batch.rows() == 1 && batch.columns() == 4 && batch.context == context(channel),
        "time observation context/shape"
    );
    let mut fields = Vec::with_capacity(4);
    for (i, width) in [1, 8, 8, 8].into_iter().enumerate() {
        let value = batch.field(0, i).map_err(anyhow::Error::msg)?;
        ensure!(value.len() == width, "time observation field width");
        fields.push(value);
    }
    let receive = u64::from_le_bytes(fields[1].try_into()?);
    let send = u64::from_le_bytes(fields[2].try_into()?);
    let residence = u64::from_le_bytes(fields[3].try_into()?);
    match fields[0] {
        [0] => {
            ensure!(
                receive == 0 && send == 0 && residence == 0,
                "noncanonical unavailable clock"
            );
            Ok(None)
        }
        [1] => Ok(Some((receive, send, residence))),
        _ => anyhow::bail!("time observation status"),
    }
}
fn encode_sample(
    channel: &mut BoundSecureChannelV1,
    times: Option<(u64, u64, u64)>,
) -> Result<Vec<u8>> {
    let body = payload(channel, times)?;
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        channel.session_id(),
        STREAM,
        1,
        1,
        0,
        body,
    );
    let envelope = channel.seal_novorudp_frame(&frame)?;
    ensure!(
        envelope.sequence == 1,
        "time observation must be first server business frame"
    );
    binary::encode(
        &crate::product_overlay::SecureNovoRudpEnvelopeV1 {
            version: envelope.version,
            session_id: envelope.session_id,
            sender_peer_id: envelope.sender_peer_id,
            recipient_peer_id: envelope.recipient_peer_id,
            sequence: envelope.sequence,
            nonce: envelope.nonce,
            ciphertext: envelope.ciphertext,
        },
        WIRE_LIMIT,
    )
}
fn open_sample(channel: &mut BoundSecureChannelV1, wire: &[u8]) -> Result<Option<(u64, u64, u64)>> {
    let envelope = binary::decode(wire, WIRE_LIMIT)?;
    ensure!(envelope.sequence == 1, "time observation sequence");
    let frame = channel.open_novorudp_frame(&SecureNovoRudpEnvelopeV1 {
        version: envelope.version,
        session_id: envelope.session_id,
        sender_peer_id: envelope.sender_peer_id,
        recipient_peer_id: envelope.recipient_peer_id,
        sequence: envelope.sequence,
        nonce: envelope.nonce,
        ciphertext: envelope.ciphertext,
    })?;
    ensure!(
        frame.kind == NovoRudpTransportFrameKindV0::Data
            && frame.session_id == channel.session_id()
            && frame.stream_id == STREAM
            && frame.object_id == 1
            && frame.sequence == 1
            && frame.ack_epoch == 0,
        "time observation frame route"
    );
    decode_payload(channel, &frame.payload)
}

/// Called only by the V8 dispatcher after V2 Finished. t2 is captured on
/// selector receipt; t3 advances that SAME anchor. Residence includes handshake
/// network waits and cannot be reported as CPU processing time.
pub async fn send_peer_time_v8(
    stream: &mut CarrierStreamV1<'_>,
    channel: &mut BoundSecureChannelV1,
    received: Option<ClockAnchorV1>,
    original_deadline: Instant,
    authority: impl Fn() -> Result<()>,
) -> Result<(), BoundSessionError> {
    let deadline = SessionClockDeadlineV2::new(original_deadline).map_err(local)?;
    let exchange = async {
        check_live(stream, deadline)?;
        check_channel(stream, channel)?;
        if channel.local_is_initiator() {
            return Err(authentication(anyhow::anyhow!(
                "time observation sender must be service responder"
            )));
        }
        authority().map_err(local)?;
        let now = Instant::now();
        let times = received.and_then(|anchor| {
            let elapsed = now.checked_duration_since(anchor.instant())?;
            Some((
                anchor.wall_ms(),
                anchor.at(now).ok()?,
                u64::try_from(elapsed.as_nanos()).ok()?,
            ))
        });
        let wire = encode_sample(channel, times).map_err(local)?;
        check_live(stream, deadline)?;
        authority().map_err(local)?;
        stream
            .write_frame(&wire)
            .await
            .map_err(BoundSessionError::Transport)?;
        authority().map_err(local)?;
        check_live(stream, deadline)
    };
    tokio::time::timeout_at(deadline.instant().into(), exchange)
        .await
        .map_err(|_| local(TransportBindingErrorV1::HandshakeDeadline))?
}

/// Read the one server frame, then adopt this SAME channel for mailbox traffic.
/// A valid but unusable numeric sample does not revoke an authenticated peer.
/// A malformed/authentication-failing protocol frame terminates this attempt.
pub async fn receive_peer_time_v8(
    stream: &mut CarrierStreamV1<'_>,
    channel: &mut BoundSecureChannelV1,
    pending: PendingPeerTimeV8,
    expected_peer: [u8; 32],
) -> Result<AuthenticatedPeerTimeV1, BoundSessionError> {
    let deadline = pending.deadline;
    let exchange = async {
        check_live(stream, deadline)?;
        check_channel(stream, channel)?;
        if !channel.local_is_initiator() {
            return Err(authentication(anyhow::anyhow!(
                "time observation receiver must be client initiator"
            )));
        }
        if channel.remote_peer_id() != peer_id_from_ed25519_public_key_v1(&expected_peer) {
            return Err(authentication(anyhow::anyhow!(
                "time observation peer pin mismatch"
            )));
        }
        let wire = stream
            .read_frame()
            .await
            .map_err(BoundSessionError::Transport)?;
        let received_at = Instant::now();
        check_live(stream, deadline)?;
        let times = open_sample(channel, &wire).map_err(authentication)?;
        let observation = match (pending.clock, times) {
            (_, None) => PeerTimeObservationV1::PeerClockUnavailable,
            (None, Some(_)) => PeerTimeObservationV1::LocalClockUnavailable,
            (Some(local_clock), Some((receive, send, residence))) => {
                match measure_peer_clock_v1(local_clock, received_at, receive, send, residence) {
                    Ok(sample) => PeerTimeObservationV1::Candidate(sample),
                    Err(_) => PeerTimeObservationV1::InvalidMeasurement,
                }
            }
        };
        check_live(stream, deadline)?;
        Ok(AuthenticatedPeerTimeV1 {
            peer_identity_public_key: expected_peer,
            observation,
        })
    };
    tokio::time::timeout_at(deadline.instant().into(), exchange)
        .await
        .map_err(|_| local(TransportBindingErrorV1::HandshakeDeadline))?
}

#[cfg(all(test, feature = "iroh-transport"))]
mod tests;
