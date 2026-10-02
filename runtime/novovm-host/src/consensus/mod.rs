//! Replacement-local consensus integration. The signed format is explicitly
//! new: it must never reinterpret legacy prepare/decision signatures.
//!
//! A cryptographically valid certificate is not execution validity, local data
//! availability, a current parent capability, or a durable canonical head.
//! No production configuration, validator key, genesis or network is activated.

mod chain;
pub use chain::{ArchiveBlock, ArchiveRead};
pub mod channel;
pub mod collector;
pub mod controller;
mod journal;
pub mod pacemaker;
pub mod transport;
pub use journal::{DurableMessage, JournalOpening, TimeoutStep, ValidatorJournal};
pub(crate) mod round;
pub mod statement;
pub mod wire;

#[cfg(test)]
mod tests;
