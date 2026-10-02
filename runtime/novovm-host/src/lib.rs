#![forbid(unsafe_code)]

//! Replacement NOVOVM host. This workspace does not load or depend on the
//! archived node. Modules are admitted individually, with an explicit source
//! and verification boundary. No deployable node or finality claim yet.

pub mod business;
#[cfg(feature = "native")]
pub mod consensus;
pub mod execution;
pub mod ingress;
#[cfg(feature = "native")]
pub mod persistence;
#[cfg(feature = "native")]
pub mod pipeline;
pub mod proof;
pub mod state;
