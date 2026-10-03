#![forbid(unsafe_code)]

//! Resident pipeline inside the original novovm-node product. Source: a7db795.
//! The node owns lifecycle and business policy, novovm-consensus owns durable
//! signing rules, and novovm-exec/aoem-bindings own the AOEM interface. Historical
//! wire domains are preserved, not relabelled to imply protocol compatibility.

pub mod business;
pub mod consensus;
pub mod execution;
pub mod ingress;
pub mod persistence;
pub mod pipeline;
pub mod proof;
pub mod service;
pub mod state;
