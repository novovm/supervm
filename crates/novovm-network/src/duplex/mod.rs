#![forbid(unsafe_code)]

//! Duplex transport integrated from the fixed SUPERVM commit
//! `a7db795cd752791612b8b4a13c26088cf0ce554e:runtime/novovm-network/src`.
//! Existing product transport modules remain at the crate root. This namespace
//! preserves the measured NVRLY002 / `novovm.relay.binary.v2` carrier and its
//! tests without claiming wire compatibility with the older root transport.
//! The product node owns its lifecycle; transport acceptance is never execution
//! or finality acknowledgement. This is not another product or authority owner.

pub mod fragments;
pub mod novorudp;
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
