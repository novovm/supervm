//! Native authenticated object delivery into the original candidate pipeline.
//! No transaction is executed or admitted by receiving fragments. Completed
//! APFL is decoded by the assembly owner and handed to CandidatePipeline's
//! existing bounded authentication API, which retains it under backpressure.

use super::{apfl::ApflLimits, delivery::prepare_completed_apfl_body};
use crate::native_pipeline::{
    business::direct_nov_fee::DirectNovFeePolicy, pipeline::AuthenticationRequest,
};
use anyhow::{ensure, Result};
use novovm_network::novorudp::{
    datagram::NativeUdpSession,
    endpoint::NativeReceivedFrame,
    fragments::{CompletedMessage, FragmentAdmission, OutgoingMessage, ReassemblyLimits},
    object_receiver::NativeObjectReceiver,
    NovoRudpAckFrame, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
};
use std::{net::SocketAddr, time::Instant};

#[cfg(test)]
use novovm_network::novorudp::{fragments::FragmentProfile, NovoRudpWindowConfig};

pub mod socket;

/// Explicit APFL-object stream, distinct from future vote/control streams.
pub const APFL_STREAM_V1: u64 = u64::from_be_bytes(*b"NVAPFL01");

pub fn apfl_fragment_frame(
    message: &OutgoingMessage,
    session: [u8; 16],
    index: usize,
) -> Result<NovoRudpTransportFrameV0> {
    let payload = message.frame(index)?;
    ensure!(
        payload.starts_with(b"NVFRAG02"),
        "APFL native stream requires UDP fragment profile"
    );
    Ok(NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        session,
        APFL_STREAM_V1,
        u64::from_be_bytes(message.id()[..8].try_into()?),
        index as u64,
        0,
        payload,
    ))
}

pub struct NativeApflInput {
    pub peer: String,
    pub object_id: [u8; 32],
    pub fragments: u64,
    pub request: AuthenticationRequest,
}

pub struct NativeApflReceiver {
    objects: NativeObjectReceiver,
    wire_limit: usize,
    expanded: ApflLimits,
    policy: DirectNovFeePolicy,
}
impl NativeApflReceiver {
    pub fn new(
        domain: [u8; 32],
        peers: Vec<String>,
        limits: ReassemblyLimits,
        expanded: ApflLimits,
        policy: DirectNovFeePolicy,
    ) -> Result<Self> {
        policy.validate()?;
        let wire_limit = limits.max_message_bytes;
        Ok(Self {
            objects: NativeObjectReceiver::new(APFL_STREAM_V1, domain, peers, limits)?,
            wire_limit,
            expanded,
            policy,
        })
    }
    pub fn accept_datagram(
        &mut self,
        session: &mut NativeUdpSession,
        source: SocketAddr,
        wire: &[u8],
        now: Instant,
    ) -> Result<FragmentAdmission> {
        self.objects.accept_datagram(session, source, wire, now)
    }
    pub fn accept_received(
        &mut self,
        received: &NativeReceivedFrame,
        now: Instant,
    ) -> Result<FragmentAdmission> {
        self.objects.accept_received(received, now)
    }
    pub fn poll_request(
        &mut self,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<NativeApflInput>> {
        self.objects
            .poll_complete(now, quantum)?
            .map(|complete| {
                decode_complete(
                    complete,
                    self.wire_limit,
                    self.expanded,
                    self.policy.clone(),
                )
            })
            .transpose()
    }
    pub fn pending_messages(&self) -> usize {
        self.objects.pending_messages()
    }
    pub fn reserved_bytes(&self) -> usize {
        self.objects.reserved_bytes()
    }
    pub fn expire(&mut self, now: Instant) -> usize {
        self.objects.expire(now)
    }
    pub fn transport_feedback(
        &mut self,
        peer: &str,
        id: [u8; 32],
        session: [u8; 16],
        epoch: u64,
        now: Instant,
    ) -> Result<Option<NovoRudpAckFrame>> {
        self.objects
            .transport_feedback(peer, id, session, epoch, now)
    }
}
fn decode_complete(
    complete: CompletedMessage,
    wire_limit: usize,
    expanded: ApflLimits,
    policy: DirectNovFeePolicy,
) -> Result<NativeApflInput> {
    let length = complete
        .chunks
        .iter()
        .try_fold(0usize, |n, c| n.checked_add(c.len()))
        .filter(|n| *n <= wire_limit)
        .ok_or_else(|| anyhow::anyhow!("completed APFL exceeds budget"))?;
    let fragments = complete.chunks.len() as u64;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length)?;
    for chunk in complete.chunks {
        bytes.extend_from_slice(&chunk);
    }
    let request = prepare_completed_apfl_body(&bytes, wire_limit, expanded, policy)?;
    Ok(NativeApflInput {
        peer: complete.peer,
        object_id: complete.id,
        fragments,
        request,
    })
}
#[cfg(test)]
pub(crate) mod tests;
