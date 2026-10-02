#![forbid(unsafe_code)]

//! Reviewed carrier primitives migrated from the isolated implementation.
//! No old node, protocol, business executor, or storage lifecycle is linked.
//! Transport acceptance is never an execution or finality acknowledgement.

pub mod fragments;
pub mod novorudp;
pub mod product_overlay;
pub mod product_relay;
pub mod product_relay_client;
pub mod product_relay_daemon;
mod product_relay_io;
pub mod worker;

pub use novorudp::*;
pub use product_overlay::*;
pub use product_relay::*;
