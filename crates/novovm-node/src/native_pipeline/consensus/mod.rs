//! Original-node integration of the a7db795 batch protocol. Safety rules,
//! canonical wire encoding, collection and durable signing live in the original
//! novovm-consensus crate; this module supplies execution and storage adapters.
//! This namespace is not a second node or a second authorized chain head.

mod chain;
pub use chain::{ArchiveBlock, ArchiveRead};
pub mod channel;
pub mod controller;
mod journal;
pub mod statement;
pub mod transport;

pub use journal::{DurableMessage, JournalOpening, TimeoutStep, ValidatorJournal};
pub use novovm_consensus::round_bft::{collector, pacemaker, wire};

#[cfg(test)]
mod tests;
