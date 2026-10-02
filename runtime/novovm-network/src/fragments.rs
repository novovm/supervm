//! Bounded opaque message transport above the encrypted worker. A digest binds
//! bytes, not their business validity or the sender's voting authority. Frame
//! payloads never exceed the existing carrier limit. No persistence or signer.
//!
//! Encoding/hashing an outgoing whole body belongs to an assembly owner, not
//! the consensus poll loop. Receive hashing is incremental with a caller-given
//! byte-work quantum (in carrier chunks). Reservations cover logical payload
//! bytes, not allocator/native/TLS overhead. Delivery is at-least-once: callers
//! deduplicate protocol messages and retry after expiry/reconnection.

use crate::worker::NETWORK_WORKER_MAX_PAYLOAD_BYTES;
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const MAGIC: &[u8; 8] = b"NVFRAG01";
const HEADER: usize = 80;
pub const CHUNK_BYTES: usize = NETWORK_WORKER_MAX_PAYLOAD_BYTES - HEADER;
/// Decoder safety ceiling, not an activated mainnet block-size parameter.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
type Hash = [u8; 32];
type Key = (String, Hash);

fn digest(domain: Hash, len: usize) -> Sha256 {
    let mut hash = Sha256::new();
    hash.update(b"novovm-replacement-fragments/v1\0");
    hash.update(domain);
    hash.update((len as u64).to_be_bytes());
    hash
}

/// Prepare once on the assembly owner; `frame` only copies one bounded chunk.
pub struct OutgoingMessage {
    domain: Hash,
    id: Hash,
    bytes: Vec<u8>,
}

impl OutgoingMessage {
    pub fn new(domain: Hash, bytes: Vec<u8>, max_message_bytes: usize) -> Result<Self> {
        ensure!(domain != [0; 32], "zero fragment domain");
        ensure!(
            (1..=MAX_MESSAGE_BYTES).contains(&max_message_bytes),
            "invalid message ceiling"
        );
        ensure!(
            !bytes.is_empty() && bytes.len() <= max_message_bytes,
            "message size exceeds budget"
        );
        let mut hash = digest(domain, bytes.len());
        hash.update(&bytes);
        Ok(Self {
            domain,
            id: hash.finalize().into(),
            bytes,
        })
    }
    pub fn id(&self) -> Hash {
        self.id
    }
    pub fn frame_count(&self) -> usize {
        self.bytes.len().div_ceil(CHUNK_BYTES)
    }
    pub fn frame(&self, index: usize) -> Result<Vec<u8>> {
        ensure!(index < self.frame_count(), "fragment index out of bounds");
        let start = index * CHUNK_BYTES;
        let end = (start + CHUNK_BYTES).min(self.bytes.len());
        let mut out = Vec::with_capacity(HEADER + end - start);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.domain);
        out.extend_from_slice(&self.id);
        out.extend_from_slice(&(self.bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&(index as u32).to_be_bytes());
        out.extend_from_slice(&self.bytes[start..end]);
        Ok(out)
    }
}

#[derive(Clone, Debug)]
pub struct ReassemblyLimits {
    pub max_message_bytes: usize,
    pub messages: usize,
    pub bytes: usize,
    pub peer_messages: usize,
    pub peer_bytes: usize,
    pub ttl: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FragmentAdmission {
    Accepted,
    Duplicate,
    Backpressure,
}

struct Entry {
    length: usize,
    created: Instant,
    chunks: Vec<Option<Vec<u8>>>,
    next_hash: usize,
    hash: Sha256,
}

/// Completion does not decode, concatenate or authenticate application data.
/// The body owner can read/move these chunks without flattening a second copy.
pub struct CompletedMessage {
    pub peer: String,
    pub id: Hash,
    pub chunks: Vec<Vec<u8>>,
}

pub struct Reassembler {
    domain: Hash,
    limits: ReassemblyLimits,
    peers: BTreeSet<String>,
    entries: BTreeMap<Key, Entry>,
    reserved: usize,
    turn: usize,
}

impl Reassembler {
    pub fn new(domain: Hash, peers: Vec<String>, limits: ReassemblyLimits) -> Result<Self> {
        ensure!(domain != [0; 32], "zero fragment domain");
        ensure!(
            !peers.is_empty() && peers.len() <= 1024,
            "invalid fragment peers"
        );
        ensure!(
            peers.iter().all(|p| !p.is_empty() && p.len() <= 256),
            "invalid fragment peer id"
        );
        let count = peers.len();
        let peers: BTreeSet<_> = peers.into_iter().collect();
        ensure!(peers.len() == count, "duplicate fragment peer");
        ensure!(
            (1..=MAX_MESSAGE_BYTES).contains(&limits.max_message_bytes)
                && (1..=4096).contains(&limits.messages)
                && limits.peer_messages > 0
                && limits.peer_messages <= limits.messages
                && limits.peer_bytes > 0
                && limits.peer_bytes <= limits.bytes
                && !limits.ttl.is_zero(),
            "invalid reassembly limits"
        );
        Ok(Self {
            domain,
            limits,
            peers,
            entries: BTreeMap::new(),
            reserved: 0,
            turn: 0,
        })
    }

    pub fn reserved_bytes(&self) -> usize {
        self.reserved
    }
    pub fn pending_messages(&self) -> usize {
        self.entries.len()
    }

    /// Reserve the declared WHOLE message before retaining its first chunk.
    /// Duplicate fragments do not prolong a peer's resource lease.
    pub fn push(&mut self, peer: &str, frame: &[u8], now: Instant) -> Result<FragmentAdmission> {
        ensure!(self.peers.contains(peer), "fragment from unconfigured peer");
        ensure!(
            frame.len() > HEADER && frame.len() <= NETWORK_WORKER_MAX_PAYLOAD_BYTES,
            "invalid fragment length"
        );
        ensure!(
            &frame[..8] == MAGIC && frame[8..40] == self.domain,
            "fragment protocol/domain mismatch"
        );
        let id: Hash = frame[40..72].try_into()?;
        let length = u32::from_be_bytes(frame[72..76].try_into()?) as usize;
        let index = u32::from_be_bytes(frame[76..80].try_into()?) as usize;
        ensure!(
            length > 0 && length <= self.limits.max_message_bytes,
            "fragment declared size exceeds budget"
        );
        let count = length.div_ceil(CHUNK_BYTES);
        ensure!(index < count, "fragment index exceeds declared message");
        let expected = (length - index * CHUNK_BYTES).min(CHUNK_BYTES);
        ensure!(
            frame.len() - HEADER == expected,
            "noncanonical fragment size"
        );
        self.expire(now);
        let key = (peer.to_owned(), id);
        if let Some(entry) = self.entries.get(&key) {
            ensure!(entry.length == length, "mixed message length");
            if let Some(old) = &entry.chunks[index] {
                ensure!(old == &frame[HEADER..], "conflicting fragment duplicate");
                return Ok(FragmentAdmission::Duplicate);
            }
        } else {
            let (peer_count, peer_bytes) = self
                .entries
                .iter()
                .filter(|((p, _), _)| p == peer)
                .fold((0usize, 0usize), |(n, bytes), (_, entry)| {
                    (n + 1, bytes + entry.length)
                });
            if self.entries.len() >= self.limits.messages
                || peer_count >= self.limits.peer_messages
                || length > self.limits.bytes.saturating_sub(self.reserved)
                || length > self.limits.peer_bytes.saturating_sub(peer_bytes)
            {
                return Ok(FragmentAdmission::Backpressure);
            }
            self.entries.insert(
                key.clone(),
                Entry {
                    length,
                    created: now,
                    chunks: vec![None; count],
                    next_hash: 0,
                    hash: digest(self.domain, length),
                },
            );
            self.reserved += length;
        }
        self.entries
            .get_mut(&key)
            .context("admitted fragment disappeared")?
            .chunks[index] = Some(frame[HEADER..].to_vec());
        Ok(FragmentAdmission::Accepted)
    }

    pub fn expire(&mut self, now: Instant) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, entry| {
            let retain = now.saturating_duration_since(entry.created) < self.limits.ttl;
            if !retain {
                self.reserved -= entry.length;
            }
            retain
        });
        before - self.entries.len()
    }

    /// Hash at most `chunk_quantum` existing chunks. Missing data does not
    /// block another peer. Returning a message transfers its payload ownership
    /// and releases this quota; downstream queues must have their own quotas.
    pub fn poll_complete(
        &mut self,
        now: Instant,
        chunk_quantum: usize,
    ) -> Result<Option<CompletedMessage>> {
        ensure!(
            (1..=64).contains(&chunk_quantum),
            "invalid reassembly quantum"
        );
        self.expire(now);
        for _ in 0..chunk_quantum {
            let count = self.entries.len();
            if count == 0 {
                return Ok(None);
            }
            let selected = (0..count).find_map(|offset| {
                let index = (self.turn + offset) % count;
                self.entries.iter().nth(index).and_then(|(key, entry)| {
                    entry.chunks[entry.next_hash]
                        .as_ref()
                        .map(|_| (index, key.clone()))
                })
            });
            let Some((index, key)) = selected else {
                return Ok(None);
            };
            self.turn = (index + 1) % count;
            let entry = self
                .entries
                .get_mut(&key)
                .context("selected fragment disappeared")?;
            entry.hash.update(
                entry.chunks[entry.next_hash]
                    .as_ref()
                    .context("fragment missing")?,
            );
            entry.next_hash += 1;
            if entry.next_hash == entry.chunks.len() {
                let entry = self
                    .entries
                    .remove(&key)
                    .context("completed fragment disappeared")?;
                self.reserved -= entry.length;
                let actual: Hash = entry.hash.finalize().into();
                ensure!(actual == key.1, "completed message hash mismatch");
                let chunks = entry
                    .chunks
                    .into_iter()
                    .map(|chunk| chunk.context("completed message hole"))
                    .collect::<Result<Vec<_>>>()?;
                return Ok(Some(CompletedMessage {
                    peer: key.0,
                    id: key.1,
                    chunks,
                }));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
