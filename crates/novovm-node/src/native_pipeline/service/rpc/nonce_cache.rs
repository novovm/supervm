//! Bounded hints for a state's nonce subjects, not an all-chain identity table.
//! Only a verified published receipt or a read at the exact published parent can
//! populate a value. A cache miss is never authoritative absence or nonce zero.

use super::{Hash, IoTicket, ParentPoint};
use anyhow::{ensure, Context, Result};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_READS: usize = 8;

/// The production implementation is the original I/O ticket. The small test
/// seam lets regressions keep a read pending or fail it without another store,
/// AOEM owner, executor, or 65K-process transaction fixture.
pub(super) trait NonceReadTicket {
    fn try_take_nonce(&mut self) -> Result<Option<Option<Vec<u8>>>>;
}

impl NonceReadTicket for IoTicket<Option<Vec<u8>>> {
    fn try_take_nonce(&mut self) -> Result<Option<Option<Vec<u8>>>> {
        self.try_take()
    }
}

struct Read<T> {
    subject: Hash,
    parent: ParentPoint,
    ticket: T,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lookup {
    Unknown,
    Reading,
    Absent,
    Present(u64),
}

pub(super) struct NonceCache<T = IoTicket<Option<Vec<u8>>>> {
    // Keep zero-valued reads separate from proved absence for diagnostics and
    // tests. Both are a resolved nonce of zero; neither is a cache miss.
    pub(super) values: BTreeMap<Hash, u64>,
    absent: BTreeSet<Hash>,
    insertion_order: VecDeque<Hash>,
    parent: Option<ParentPoint>,
    reads: VecDeque<Read<T>>,
    capacity: usize,
}

impl<T: NonceReadTicket> NonceCache<T> {
    pub(super) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "nonce cache capacity must be positive");
        Self {
            values: BTreeMap::new(),
            absent: BTreeSet::new(),
            insertion_order: VecDeque::new(),
            parent: None,
            reads: VecDeque::new(),
            capacity,
        }
    }

    fn clear(&mut self) {
        self.values.clear();
        self.absent.clear();
        self.insertion_order.clear();
        // Abandon notifications, not accepted I/O. The existing I/O owner
        // retains its permits until accepted reads finish.
        self.reads.clear();
    }

    pub(super) fn bind_current_parent(&mut self, parent: ParentPoint) {
        if self.parent != Some(parent) {
            self.clear();
            self.parent = Some(parent);
        }
    }

    /// Called once for the next verified published archive/packet, before all
    /// of that packet's receipt transitions are applied in the same control
    /// turn. No selection or admission can observe the intermediate projection.
    /// Unchanged subjects retain their value across that verified full prefix;
    /// candidates, private lookahead and transport acknowledgements never call it.
    pub(super) fn begin_published_parent(&mut self, parent: ParentPoint) {
        if self
            .parent
            .is_none_or(|old| old.height.checked_add(1) != Some(parent.height))
        {
            self.clear();
        } else {
            // An older-root completion cannot overwrite this publication's
            // nonce transition, including when its after-value was evicted.
            self.reads.clear();
        }
        self.parent = Some(parent);
    }

    pub(super) fn get(&self, subject: &Hash) -> Option<&u64> {
        self.values.get(subject)
    }

    pub(super) fn has_pending_reads(&self) -> bool {
        !self.reads.is_empty()
    }

    fn lookup(&self, subject: &Hash) -> Lookup {
        if let Some(value) = self.values.get(subject) {
            if self.absent.contains(subject) {
                Lookup::Absent
            } else {
                Lookup::Present(*value)
            }
        } else if self.reads.iter().any(|read| read.subject == *subject) {
            Lookup::Reading
        } else {
            Lookup::Unknown
        }
    }

    fn insert(&mut self, subject: Hash, value: u64, absent: bool) {
        if !self.values.contains_key(&subject) {
            if self.values.len() == self.capacity {
                let oldest = self.insertion_order.pop_front().expect("cached nonce");
                self.values.remove(&oldest);
                self.absent.remove(&oldest);
            }
            self.insertion_order.push_back(subject);
        }
        self.values.insert(subject, value);
        if absent {
            self.absent.insert(subject);
        } else {
            self.absent.remove(&subject);
        }
    }

    pub(super) fn published(&mut self, subject: Hash, nonce_after: u64) {
        self.insert(subject, nonce_after, false);
    }

    fn accept_read(
        &mut self,
        parent: ParentPoint,
        subject: Hash,
        value: Option<Vec<u8>>,
    ) -> Result<()> {
        // This check is also required after notification: dropping an old
        // ticket or changing roots must not reinterpret its output as current.
        ensure!(self.parent == Some(parent), "nonce read parent changed");
        let (nonce, absent) = match value {
            None => (0, true),
            Some(bytes) => (
                u64::from_le_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("invalid authoritative nonce width"))?,
                ),
                false,
            ),
        };
        self.insert(subject, nonce, absent);
        Ok(())
    }

    /// Uses the original asynchronous AOEM state reader, with at most eight
    /// outstanding reads. Backpressure and not-ready do not create zero values.
    /// Unknown I/O outcomes and malformed records stay errors, never absence.
    pub(super) fn poll(
        &mut self,
        parent: ParentPoint,
        subjects: impl Iterator<Item = Hash>,
        mut submit: impl FnMut(Hash, Hash) -> Result<Option<T>>,
    ) -> Result<bool> {
        self.bind_current_parent(parent);
        let mut progressed = false;
        let count = self.reads.len();
        for _ in 0..count {
            let mut read = self.reads.pop_front().context("pending nonce read")?;
            let value = match read.ticket.try_take_nonce() {
                Ok(None) => {
                    self.reads.push_back(read);
                    continue;
                }
                Ok(Some(value)) => value,
                Err(error) => return Err(error.context("authoritative nonce read failed")),
            };
            self.accept_read(read.parent, read.subject, value)?;
            progressed = true;
        }
        for subject in subjects {
            if self.reads.len() == MAX_READS {
                break;
            }
            if self.lookup(&subject) != Lookup::Unknown {
                continue;
            }
            let Some(ticket) = submit(parent.state_root, subject)? else {
                // The shared owner is busy; another subject cannot bypass it.
                break;
            };
            self.reads.push_back(Read {
                subject,
                parent,
                ticket,
            });
            progressed = true;
        }
        Ok(progressed)
    }
}

#[cfg(test)]
mod tests;
