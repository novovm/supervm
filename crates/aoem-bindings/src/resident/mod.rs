//! Explicit, resident AOEM owners integrated into the original product bindings.
//!
//! Compute/storage and tests come from the measured a7db795 implementation;
//! portable receipts and rejection tests come from 595a092 (AORCP002).
//! Graph types alias this crate's existing ABI. These owners preserve explicit
//! trusted-library selection, owner-thread confinement, whole-batch durability,
//! bounded multi-get, cancellation drain and unknown-write poisoning. They do not
//! select a second business state, chain head or consensus protocol.
//!
//! Callers should use novovm_exec::resident as the product execution facade.
//! Owned CPU callbacks do not automatically become GPU operators or ZK proofs.
//! The old AORCP001 API remains an explicitly separate historical protocol;
//! resident ReceiptSession does not accept or relabel its envelopes.
//!
//! No environment mutation, fallback executor or library discovery happens here.
//! Native initialization may perform I/O; embedding processes must configure
//! their environment before startup and authenticate the selected native binary.
#![deny(unsafe_op_in_unsafe_fn)]

mod abi;
mod compute;
mod library;
mod receipt;
mod storage;

pub use compute::{ComputeReport, ComputeSession, ComputeTask};
pub use receipt::{ReceiptBackendUnavailable, ReceiptLimits, ReceiptSession};
pub use storage::{StorageConfig, StorageLimits, StorageSession, StorageWrite};
