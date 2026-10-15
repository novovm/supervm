#![forbid(unsafe_code)]

//! Duplex transport integrated from the fixed SUPERVM commit
//! `a7db795cd752791612b8b4a13c26088cf0ce554e:runtime/novovm-network/src`.
//! Existing product transport modules remain at the crate root. This namespace
//! preserves the measured NVRLY002 / `novovm.relay.binary.v2` carrier and its
//! tests without claiming relay/session compatibility with the older transport.
//! NOVRUDP0 frames now use the single original-module codec; relay/session
//! formats remain distinct and must not be silently reinterpreted.
//! The product node owns its lifecycle; transport acceptance is never execution
//! or finality acknowledgement. This is not another product or authority owner.

pub mod byte_stream;
pub mod fragments;
// Compatibility namespace only: no second frame codec or frame type.
pub mod novorudp {
    pub use crate::novorudp::{
        NovoRudpTransportFrameDecodeErrorV0, NovoRudpTransportFrameEncodeErrorV0,
        NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0, NOVORUDP_TRANSPORT_FRAME_V0_MAGIC,
        NOVORUDP_TRANSPORT_FRAME_V0_VERSION,
    };
}
pub mod product_overlay;
pub mod product_relay;
pub mod product_relay_client;
pub mod product_relay_daemon;
mod product_relay_io;
mod product_relay_wire;
pub mod worker;

pub use novorudp::*;
pub use product_overlay::*;
pub use product_relay::*;
#[cfg(any(feature = "iroh-transport", feature = "tls-transport"))]
pub use product_relay_wire::{decode_iroh_envelope_v1, encode_iroh_envelope_v1};
