//! Explicit V2 identity/liveness proof on the original authenticated carrier.
//!
//! The inner ECDH/signature/Finished primitives retain their original checks.
//! V2 signs a separate purpose/version and canonical protocol-relative epoch,
//! not a claim about UTC. Only this closed adapter supplies logical zero; V1
//! public entries keep their original wall-clock rules. One immutable local
//! deadline bounds the WHOLE exchange, never each message separately.
//! This proves key possession, not UTC, service freshness, TTL or entitlement.

use super::*;
use std::time::{Duration, Instant};

/// Construct before the first authentication read/write. The enclosing carrier
/// may impose an earlier deadline; it is never extended by this adapter.
#[derive(Clone, Copy)]
pub struct SessionClockDeadlineV2 {
    until: Instant,
}

impl SessionClockDeadlineV2 {
    pub fn new(original_deadline: Instant) -> Result<Self> {
        let now = Instant::now();
        let until =
            original_deadline.min(now + Duration::from_millis(BOUND_HANDSHAKE_MAX_TTL_MS_V1));
        let value = Self { until };
        value.check()?;
        Ok(value)
    }

    pub fn instant(self) -> Instant {
        self.until
    }

    pub fn check(self) -> Result<()> {
        if Instant::now() >= self.until {
            return Err(TransportBindingErrorV1::HandshakeDeadline);
        }
        Ok(())
    }
}

pub struct SessionClockInitiatorV2 {
    inner: BoundHandshakeInitiatorV1,
    deadline: SessionClockDeadlineV2,
}

impl SessionClockInitiatorV2 {
    pub fn start(
        identity: &SigningKey,
        expected_remote_key: [u8; 32],
        binding: VerifiedTransportBinding,
        deadline: SessionClockDeadlineV2,
    ) -> Result<Self> {
        deadline.check()?;
        let inner = BoundHandshakeInitiatorV1::start_with_mode(
            identity,
            expected_remote_key,
            binding,
            0,
            BOUND_HANDSHAKE_MAX_TTL_MS_V1,
            BoundClockMode::SessionV2,
        )?;
        deadline.check()?;
        Ok(Self { inner, deadline })
    }

    pub fn offer(&self) -> &BoundNodeHandshakeOfferV1 {
        self.inner.offer()
    }

    pub fn complete(
        self,
        response: &BoundNodeHandshakeResponseV1,
        replay: &mut HandshakeReplayCacheV1,
    ) -> Result<PendingSessionClockV2> {
        self.deadline.check()?;
        let inner = self.inner.complete(response, 0, replay)?;
        self.deadline.check()?;
        Ok(PendingSessionClockV2 {
            inner,
            deadline: self.deadline,
        })
    }
}

pub struct SessionClockResponderV2 {
    response: BoundNodeHandshakeResponseV1,
    pending: PendingSessionClockV2,
}

impl SessionClockResponderV2 {
    pub fn respond(
        offer: &BoundNodeHandshakeOfferV1,
        identity: &SigningKey,
        expected_remote_key: [u8; 32],
        binding: VerifiedTransportBinding,
        deadline: SessionClockDeadlineV2,
        replay: &mut HandshakeReplayCacheV1,
    ) -> Result<Self> {
        deadline.check()?;
        let responder = BoundHandshakeResponderV1::respond_with_mode(
            offer,
            identity,
            expected_remote_key,
            binding,
            0,
            BOUND_HANDSHAKE_MAX_TTL_MS_V1,
            replay,
            BoundClockMode::SessionV2,
        )?;
        deadline.check()?;
        Ok(Self {
            response: responder.response.clone(),
            pending: PendingSessionClockV2 {
                inner: responder.into_pending(),
                deadline,
            },
        })
    }

    pub fn response(&self) -> &BoundNodeHandshakeResponseV1 {
        &self.response
    }

    pub fn into_pending(self) -> PendingSessionClockV2 {
        self.pending
    }
}

/// Opaque: no V1 pending channel can escape to a caller that could reinterpret
/// the canonical relative epoch as UTC or skip its original monotonic budget.
pub struct PendingSessionClockV2 {
    inner: PendingBoundChannelV1,
    deadline: SessionClockDeadlineV2,
}

impl PendingSessionClockV2 {
    pub fn make_finished(&mut self) -> Result<SecureNovoRudpEnvelopeV1> {
        self.deadline.check()?;
        let result = self.inner.make_finished(0)?;
        self.deadline.check()?;
        Ok(result)
    }

    pub fn verify_finished(&mut self, envelope: &SecureNovoRudpEnvelopeV1) -> Result<()> {
        self.deadline.check()?;
        self.inner.verify_finished(envelope, 0)?;
        self.deadline.check()
    }

    pub fn into_channel(self) -> Result<BoundSecureChannelV1> {
        self.deadline.check()?;
        let result = self.inner.into_channel(0)?;
        self.deadline.check()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
