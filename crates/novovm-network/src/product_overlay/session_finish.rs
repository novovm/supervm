//! Shared Finished frame shape; key derivation, AEAD and replay stay with the
//! original E2E channel. Carrier and native profiles keep distinct transcripts.
use crate::novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};

pub(crate) fn finished_frame(
    domain: &[u8],
    session: [u8; 16],
    transcript: [u8; 32],
    initiator: bool,
) -> NovoRudpTransportFrameV0 {
    let mut payload = Vec::with_capacity(domain.len() + 33);
    payload.extend_from_slice(domain);
    payload.push(if initiator { 1 } else { 2 });
    payload.extend_from_slice(&transcript);
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Endpoint,
        session,
        u64::MAX,
        0,
        0,
        0,
        payload,
    )
}
