//! Explicit structured V2 identity/liveness proof on the existing carrier.
//!
//! The original operation deadline is local monotonic state, never a remote
//! timestamp. This entry does not establish UTC, refresh a locator, extend a
//! mailbox TTL, or grant an application capability. Any error is terminal for
//! this stream attempt; the owner must drop it rather than retry or downgrade.

use super::*;
use crate::transport_binding::{
    session_clock::{
        PendingSessionClockV2, SessionClockDeadlineV2, SessionClockInitiatorV2,
        SessionClockResponderV2,
    },
    structured, TransportBindingErrorV1,
};
use std::time::Instant;

fn check_deadline(deadline: SessionClockDeadlineV2) -> Result<(), BoundSessionError> {
    deadline.check().map_err(local)
}

fn proof_error(error: TransportBindingErrorV1) -> BoundSessionError {
    if matches!(error, TransportBindingErrorV1::HandshakeDeadline) {
        local(error)
    } else {
        authentication(error)
    }
}

fn observe_v2<O>(
    stream: &CarrierStreamV1<'_>,
    observer: &mut O,
    stage: BoundSessionStage,
    deadline: SessionClockDeadlineV2,
) -> Result<(), BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    check_deadline(deadline)?;
    observe(stream, observer, stage)?;
    // timeout_at cannot preempt a synchronous observer. Do not let a callback
    // crossing the deadline expose a channel or begin another whole frame.
    check_deadline(deadline)
}

fn check_return(
    stream: &CarrierStreamV1<'_>,
    deadline: SessionClockDeadlineV2,
) -> Result<(), BoundSessionError> {
    check_deadline(deadline)?;
    stream
        .selected_path()
        .map_err(BoundSessionError::Transport)?;
    check_deadline(deadline)
}

/// Authenticate a pinned peer using the explicitly selected structured V2
/// service. Both ends must choose this entry; V1 and JSON are never tried.
/// `original_deadline` includes time already consumed by the enclosing action.
/// The same immutable deadline covers the first frame through final return;
/// the carrier's own earlier authority/deadline checks remain in force.
pub async fn authenticate_structured_bound_session_v2_observed<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    expected_peer: [u8; 32],
    initiator: bool,
    original_deadline: Instant,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<BoundSecureChannelV1, BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    let deadline = SessionClockDeadlineV2::new(original_deadline).map_err(local)?;
    let exchange = async {
        check_deadline(deadline)?;
        let binding = stream.binding().map_err(authentication)?;
        let pending = if initiator {
            let start = SessionClockInitiatorV2::start(identity, expected_peer, binding, deadline)
                .map_err(proof_error)?;
            let offer = structured::encode_offer_v2(start.offer()).map_err(local)?;
            check_deadline(deadline)?;
            stream
                .write_frame(&offer)
                .await
                .map_err(BoundSessionError::Transport)?;
            observe_v2(stream, observer, BoundSessionStage::OfferWritten, deadline)?;
            let response = stream
                .read_frame()
                .await
                .map_err(BoundSessionError::Transport)?;
            check_deadline(deadline)?;
            let response = structured::decode_response_v2(&response).map_err(authentication)?;
            let pending = start.complete(&response, replay).map_err(proof_error)?;
            observe_v2(
                stream,
                observer,
                BoundSessionStage::ResponseVerified,
                deadline,
            )?;
            pending
        } else {
            let offer = stream
                .read_frame()
                .await
                .map_err(BoundSessionError::Transport)?;
            check_deadline(deadline)?;
            let offer = structured::decode_offer_v2(&offer).map_err(authentication)?;
            let response = SessionClockResponderV2::respond(
                &offer,
                identity,
                expected_peer,
                binding,
                deadline,
                replay,
            )
            .map_err(proof_error)?;
            observe_v2(stream, observer, BoundSessionStage::OfferVerified, deadline)?;
            let encoded = structured::encode_response_v2(response.response()).map_err(local)?;
            check_deadline(deadline)?;
            stream
                .write_frame(&encoded)
                .await
                .map_err(BoundSessionError::Transport)?;
            observe_v2(
                stream,
                observer,
                BoundSessionStage::ResponseWritten,
                deadline,
            )?;
            response.into_pending()
        };
        finish_session_v2(stream, pending, observer, deadline).await
    };
    let channel = tokio::time::timeout_at(deadline.instant().into(), exchange)
        .await
        .map_err(|_| local(TransportBindingErrorV1::HandshakeDeadline))??;
    check_return(stream, deadline)?;
    Ok(channel)
}

/// Open or pinned provider policy on the explicit structured V2 entry.
/// Open admission proves possession once for this stream; recipient
/// capabilities, quotas, revocation and business TTL remain separate checks.
/// An empty pinned list fails closed and never becomes open admission.
pub async fn authenticate_structured_service_client_v2_observed<O>(
    stream: &mut CarrierStreamV1<'_>,
    identity: &SigningKey,
    policy: ServicePeerPolicyV1<'_>,
    original_deadline: Instant,
    replay: &mut HandshakeReplayCacheV1,
    observer: &mut O,
) -> Result<(BoundSecureChannelV1, [u8; 32]), BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    let deadline = SessionClockDeadlineV2::new(original_deadline).map_err(local)?;
    if let ServicePeerPolicyV1::Pinned(allowed) = policy {
        if allowed.is_empty() || allowed.len() > 16 {
            return Err(local(anyhow::anyhow!("invalid service allowlist size")));
        }
    }
    let exchange = async {
        check_deadline(deadline)?;
        let binding = stream.binding().map_err(authentication)?;
        let received = stream
            .read_frame()
            .await
            .map_err(BoundSessionError::Transport)?;
        check_deadline(deadline)?;
        let offer = structured::decode_offer_v2(&received).map_err(authentication)?;
        let claimed = offer.context.initiator_identity_public_key;
        if claimed == identity.verifying_key().to_bytes()
            || matches!(policy, ServicePeerPolicyV1::Pinned(allowed) if !allowed.contains(&claimed))
        {
            return Err(authentication(anyhow::anyhow!(
                "service client not permitted"
            )));
        }
        let response =
            SessionClockResponderV2::respond(&offer, identity, claimed, binding, deadline, replay)
                .map_err(proof_error)?;
        observe_v2(stream, observer, BoundSessionStage::OfferVerified, deadline)?;
        let encoded = structured::encode_response_v2(response.response()).map_err(local)?;
        check_deadline(deadline)?;
        stream
            .write_frame(&encoded)
            .await
            .map_err(BoundSessionError::Transport)?;
        observe_v2(
            stream,
            observer,
            BoundSessionStage::ResponseWritten,
            deadline,
        )?;
        let channel =
            finish_session_v2(stream, response.into_pending(), observer, deadline).await?;
        Ok((channel, claimed))
    };
    let authenticated = tokio::time::timeout_at(deadline.instant().into(), exchange)
        .await
        .map_err(|_| local(TransportBindingErrorV1::HandshakeDeadline))??;
    check_return(stream, deadline)?;
    Ok(authenticated)
}

async fn finish_session_v2<O>(
    stream: &mut CarrierStreamV1<'_>,
    mut pending: PendingSessionClockV2,
    observer: &mut O,
    deadline: SessionClockDeadlineV2,
) -> Result<BoundSecureChannelV1, BoundSessionError>
where
    O: FnMut(BoundSessionStage, &'static str) -> anyhow::Result<()> + Send + ?Sized,
{
    check_deadline(deadline)?;
    let finished = pending.make_finished().map_err(local)?;
    let encoded = structured::encode_finished(&finished).map_err(local)?;
    check_deadline(deadline)?;
    stream
        .write_frame(&encoded)
        .await
        .map_err(BoundSessionError::Transport)?;
    observe_v2(
        stream,
        observer,
        BoundSessionStage::FinishedWritten,
        deadline,
    )?;
    let remote_finished = stream
        .read_frame()
        .await
        .map_err(BoundSessionError::Transport)?;
    check_deadline(deadline)?;
    let remote_finished = structured::decode_finished(&remote_finished).map_err(authentication)?;
    pending
        .verify_finished(&remote_finished)
        .map_err(proof_error)?;
    let channel = pending.into_channel().map_err(proof_error)?;
    observe_v2(
        stream,
        observer,
        BoundSessionStage::FinishedVerified,
        deadline,
    )?;
    check_return(stream, deadline)?;
    Ok(channel)
}
