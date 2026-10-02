//! Minimal, domain-neutral AOEM boundaries for the replacement runtime.
//!
//! This crate owns the native FFI boundary; the Host need not use unsafe code.
//! It does not import legacy crates or bind business policy to a writer,
//! modify process environment, or provide a fallback executor. The caller supplies a
//! trusted AOEM library path and owned computations. Production deployment must
//! authenticate that binary separately; ABI checks are not binary authentication.
//! [`ComputeSession::open`] rejects a nonempty `AOEM_PERSISTENCE_PATH`: the current native create
//! ABI can otherwise implicitly open storage. This is not a guarantee of no
//! third-party initialization I/O. The embedding process must configure AOEM
//! before startup and must not concurrently mutate its process environment.
//!
//! [`ComputeSession::execute`] blocks its designated computation owner, not the
//! control loop. This component alone is neither an asynchronous node pipeline
//! nor evidence of mainchain throughput, finality, privacy or execution proofs.
//! [`ReceiptSession`] is a separate synchronous portable-receipt boundary, not
//! an execution/finality permission and not a cancellable prover scheduler.

#![deny(unsafe_op_in_unsafe_fn)]

mod abi;
mod compute;
mod library;
mod receipt;
mod storage;

pub use compute::{ComputeReport, ComputeSession, ComputeTask};
pub use receipt::{ReceiptBackendUnavailable, ReceiptLimits, ReceiptSession};
pub use storage::{StorageConfig, StorageLimits, StorageSession, StorageWrite};
