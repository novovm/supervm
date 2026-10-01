#![forbid(unsafe_code)]

//! Replacement NOVOVM host. This workspace does not load or depend on the
//! archived node. Modules are admitted individually, with an explicit source
//! and verification boundary. No deployable node or finality claim yet.

pub mod business;
pub mod execution;
pub mod state;
