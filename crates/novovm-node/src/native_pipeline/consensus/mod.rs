//! Existing candidate/storage adapter for the original consensus-owned rules.
//! Source: main 58e3aade, previously reviewed a7db795 lineage. Protocol bytes,
//! conditional durability and recovery are unchanged. No relay/controller is
//! introduced here; the node owns its native transport and lifecycle.
// Active adapters are NativeChannel -> NativeLifecycle / RoundProgress below.
// channel.rs and controller.rs (including their test sources) are retained for
// historical comparison, NOT compiled as a parallel product/signing owner.
// See docs/NOVOVM_JULY_MAIN_INTEGRATION_MAP.md for replacement/coverage limits.
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
