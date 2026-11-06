//! Existing candidate/storage adapter for the original consensus-owned rules.
//! Source: main 58e3aade, previously reviewed a7db795 lineage. Protocol bytes,
//! conditional durability and recovery are unchanged. No relay/controller is
//! introduced here; the node owns its native transport and lifecycle.
// Active adapters are NativeChannel -> NativeLifecycle / RoundProgress below.
// The former WSS channel/controller and their private fixtures are retired.
// Required source binding, quorum and recovery checks exercise these native
// owners; archived passes/failures do not qualify the current deployment.
pub mod bodies;
mod chain;
pub mod crypto;
mod journal;
pub(crate) mod message_evidence;
pub mod native;
pub mod native_lifecycle;
pub mod progress;
pub mod statement;
pub mod transport;
pub use chain::{ArchiveBlock, ArchiveRead};
pub use journal::{DurableMessage, JournalOpening, TimeoutStep, ValidatorJournal};
pub use novovm_consensus::round_bft::{collector, pacemaker, wire};

#[cfg(test)]
mod tests;
