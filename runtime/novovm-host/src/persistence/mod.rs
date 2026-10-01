//! Local, immutable candidate content. Durable does not mean canonical, proved,
//! voted, or finalized. No head pointer or permission to sign is stored here.
//! The storage owner is separate from the computation owner and control loop.

pub mod io;
pub mod packet;
mod store;

pub use packet::{PacketBudget, PreparedCandidate, StoredCandidate};
pub use store::{CandidateStore, OpenMode, PersistedCandidate, StorageDomain, StoreConfig};
