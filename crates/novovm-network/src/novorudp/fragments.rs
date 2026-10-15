//! Bounded opaque message transport above the encrypted worker. A digest binds
//! bytes, not their business validity or the sender's voting authority. Frame
//! payloads never exceed the existing carrier limit. No persistence or signer.
//!
//! Encoding/hashing an outgoing whole body belongs to an assembly owner, not
//! the consensus poll loop. Receive hashing is incremental with a caller-given
//! byte-work quantum (in carrier chunks). Reservations cover logical payload
//! bytes, not allocator/native/TLS overhead. Delivery is at-least-once: callers
//! deduplicate protocol messages and retry after expiry/reconnection.

// Original large-carrier profile retained byte-for-byte; no Relay dependency.
const NETWORK_WORKER_MAX_PAYLOAD_BYTES: usize = 192 * 1024;
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const MAGIC: &[u8; 8] = b"NVFRAG01";
const UDP_MAGIC: &[u8; 8] = b"NVFRAG02";
const HEADER: usize = 80;
pub const CHUNK_BYTES: usize = NETWORK_WORKER_MAX_PAYLOAD_BYTES - HEADER;
pub const UDP_CHUNK_BYTES: usize = 768;

/// Explicit framing profiles: never reinterpret the existing large-frame wire
/// as small UDP chunks. Message identity still commits to exactly the same body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FragmentProfile {
    LargeV1,
    NativeUdpV2,
}

impl FragmentProfile {
    fn chunk_bytes(self) -> usize {
        match self {
            Self::LargeV1 => CHUNK_BYTES,
            Self::NativeUdpV2 => UDP_CHUNK_BYTES,
        }
    }
    fn magic(self) -> &'static [u8; 8] {
        match self {
            Self::LargeV1 => MAGIC,
            Self::NativeUdpV2 => UDP_MAGIC,
        }
    }
}
/// Decoder safety ceiling, not an activated mainnet block-size parameter.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
type Hash = [u8; 32];
type Key = (String, Hash);

fn digest(domain: Hash, len: usize) -> Sha256 {
    let mut hash = Sha256::new();
    // Historical wire domain, not a product-layout directive. Changing this
    // label during integration would silently change message identities.
    hash.update(b"novovm-replacement-fragments/v1\0");
    hash.update(domain);
    hash.update((len as u64).to_be_bytes());
    hash
}

/// Prepare once on the assembly owner; `frame` only copies one bounded chunk.
pub struct OutgoingMessage {
    profile: FragmentProfile,
    domain: Hash,
    id: Hash,
    bytes: Vec<u8>,
}

impl OutgoingMessage {
    pub fn new(domain: Hash, bytes: Vec<u8>, max_message_bytes: usize) -> Result<Self> {
        Self::with_profile(domain, bytes, max_message_bytes, FragmentProfile::LargeV1)
    }

    pub fn with_profile(
        domain: Hash,
        bytes: Vec<u8>,
        max_message_bytes: usize,
        profile: FragmentProfile,
    ) -> Result<Self> {
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
            profile,
            domain,
            id: hash.finalize().into(),
            bytes,
        })
    }
    pub fn id(&self) -> Hash {
        self.id
    }
    pub fn domain(&self) -> Hash {
        self.domain
    }
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
    pub fn profile(&self) -> FragmentProfile {
        self.profile
    }
    pub fn frame_count(&self) -> usize {
        self.bytes.len().div_ceil(self.profile.chunk_bytes())
    }
    pub fn frame(&self, index: usize) -> Result<Vec<u8>> {
        ensure!(index < self.frame_count(), "fragment index out of bounds");
        let start = index * self.profile.chunk_bytes();
        let end = (start + self.profile.chunk_bytes()).min(self.bytes.len());
        let mut out = Vec::with_capacity(HEADER + end - start);
        out.extend_from_slice(self.profile.magic());
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
    received: usize,
    first_missing: usize,
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
    profile: FragmentProfile,
    domain: Hash,
    limits: ReassemblyLimits,
    peers: BTreeSet<String>,
    entries: BTreeMap<Key, Entry>,
    // Optional transport-only completion history. No payload or execution result.
    completed: BTreeMap<Key, (usize, Instant)>,
    completion_cache: bool,
    reserved: usize,
    turn: usize,
}

impl Reassembler {
    /// Called only by the authenticated socket owner. Reuses the same global
    /// and per-peer assembly budgets; this does not accept an unverified key.
    pub(crate) fn admit_authenticated_peer(&mut self, peer: String) -> Result<()> {
        ensure!(
            !peer.is_empty() && peer.len() <= 256,
            "invalid fragment peer id"
        );
        ensure!(
            self.peers.contains(&peer) || self.peers.len() < 1024,
            "fragment peer bound"
        );
        self.peers.insert(peer);
        Ok(())
    }
    pub fn new(domain: Hash, peers: Vec<String>, limits: ReassemblyLimits) -> Result<Self> {
        Self::with_profile(domain, peers, limits, FragmentProfile::LargeV1)
    }

    pub fn with_profile(
        domain: Hash,
        peers: Vec<String>,
        limits: ReassemblyLimits,
        profile: FragmentProfile,
    ) -> Result<Self> {
        ensure!(!peers.is_empty(), "invalid fragment peers");
        Self::build(domain, peers, limits, profile)
    }

    /// No peers are admitted until the authenticated endpoint appends them.
    /// Kept private to the carrier; public empty allowlists never mean open.
    pub(crate) fn awaiting_authenticated_peers(
        domain: Hash,
        limits: ReassemblyLimits,
        profile: FragmentProfile,
    ) -> Result<Self> {
        Self::build(domain, Vec::new(), limits, profile)
    }

    fn build(
        domain: Hash,
        peers: Vec<String>,
        limits: ReassemblyLimits,
        profile: FragmentProfile,
    ) -> Result<Self> {
        ensure!(domain != [0; 32], "zero fragment domain");
        ensure!(peers.len() <= 1024, "invalid fragment peers");
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
            profile,
            domain,
            limits,
            peers,
            entries: BTreeMap::new(),
            completed: BTreeMap::new(),
            completion_cache: false,
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

    /// Explicit native-carrier opt-in; preserve the original at-least-once API.
    /// Metadata uses the existing global/per-peer message limits and TTL. Evicted
    /// or expired identities may be delivered again; business dedup is separate.
    pub fn with_completion_history(mut self) -> Self {
        self.completion_cache = true;
        self
    }

    pub fn completed_total(&self, peer: &str, id: Hash, now: Instant) -> Option<u64> {
        self.completed
            .get(&(peer.to_owned(), id))
            .filter(|(_, created)| now.saturating_duration_since(*created) < self.limits.ttl)
            .map(|(length, _)| length.div_ceil(self.profile.chunk_bytes()) as u64)
    }

    fn remember_completion(&mut self, key: Key, length: usize, now: Instant) {
        if !self.completion_cache {
            return;
        }
        // Evict only completed metadata, never pending payload or caller work.
        let peer_full = self
            .completed
            .keys()
            .filter(|(peer, _)| peer == &key.0)
            .count()
            >= self.limits.peer_messages;
        if peer_full || self.completed.len() >= self.limits.messages {
            let oldest = self
                .completed
                .iter()
                .filter(|((peer, _), _)| !peer_full || peer == &key.0)
                .min_by_key(|(_, (_, created))| *created)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.completed.remove(&oldest);
            }
        }
        self.completed.insert(key, (length, now));
    }

    /// Bounded ACK work: the missing prefix advances once per admitted chunk;
    /// only the existing current window is scanned, never the whole object on
    /// every feedback poll. Hash completion remains a separate requirement.
    pub fn window_snapshot(
        &self,
        peer: &str,
        id: Hash,
        config: &super::NovoRudpWindowConfig,
    ) -> Result<Option<(u64, u64, Option<super::NovoRudpRepairWindow>)>> {
        ensure!(
            (1..=64).contains(&config.window_size),
            "invalid bounded feedback window"
        );
        let Some(entry) = self.entries.get(&(peer.to_owned(), id)) else {
            return Ok(None);
        };
        let total = entry.chunks.len();
        let missing = total - entry.received;
        let mut ranges = Vec::new();
        let mut start = None;
        let end = (entry.first_missing + config.window_size as usize).min(total);
        for index in entry.first_missing..end {
            match (start, entry.chunks[index].is_none()) {
                (None, true) => start = Some(index as u64),
                (Some(first), false) => {
                    ranges.push(super::NovoRudpRange::new(first, index as u64 - 1));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(first) = start {
            ranges.push(super::NovoRudpRange::new(first, end as u64 - 1));
        }
        Ok(Some((
            total as u64,
            missing as u64,
            super::select_first_missing_window(&ranges, total as u64, config),
        )))
    }

    /// Snapshot for the original window/repair selector, based only on received
    /// transport fragments. Never consults AOEM receipts or chain publication.
    /// None means no retained object (unknown, expired or already handed off),
    /// NOT a successful transport ACK. Expiry must be serviced before taking it.
    pub fn missing_ranges(&self, peer: &str, id: Hash) -> Option<(u64, Vec<super::NovoRudpRange>)> {
        let entry = self.entries.get(&(peer.to_owned(), id))?;
        let mut ranges = Vec::new();
        let mut start = None;
        for (index, chunk) in entry.chunks.iter().enumerate() {
            match (start, chunk.is_none()) {
                (None, true) => start = Some(index as u64),
                (Some(first), false) => {
                    ranges.push(super::NovoRudpRange::new(first, index as u64 - 1));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(first) = start {
            ranges.push(super::NovoRudpRange::new(
                first,
                entry.chunks.len() as u64 - 1,
            ));
        }
        Some((entry.chunks.len() as u64, ranges))
    }

    /// Reserve the declared WHOLE message before retaining its first chunk.
    /// Duplicate fragments do not prolong a peer's resource lease.
    pub fn push(&mut self, peer: &str, frame: &[u8], now: Instant) -> Result<FragmentAdmission> {
        ensure!(self.peers.contains(peer), "fragment from unconfigured peer");
        ensure!(
            frame.len() > HEADER && frame.len() <= HEADER + self.profile.chunk_bytes(),
            "invalid fragment length"
        );
        ensure!(
            &frame[..8] == self.profile.magic() && frame[8..40] == self.domain,
            "fragment protocol/domain mismatch"
        );
        let id: Hash = frame[40..72].try_into()?;
        let length = u32::from_be_bytes(frame[72..76].try_into()?) as usize;
        let index = u32::from_be_bytes(frame[76..80].try_into()?) as usize;
        ensure!(
            length > 0 && length <= self.limits.max_message_bytes,
            "fragment declared size exceeds budget"
        );
        let count = length.div_ceil(self.profile.chunk_bytes());
        ensure!(index < count, "fragment index exceeds declared message");
        let expected =
            (length - index * self.profile.chunk_bytes()).min(self.profile.chunk_bytes());
        ensure!(
            frame.len() - HEADER == expected,
            "noncanonical fragment size"
        );
        self.expire(now);
        let key = (peer.to_owned(), id);
        if let Some((completed_length, _)) = self.completed.get(&key) {
            ensure!(
                *completed_length == length,
                "completed object shape mismatch"
            );
            return Ok(FragmentAdmission::Duplicate);
        }
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
                    received: 0,
                    first_missing: 0,
                    hash: digest(self.domain, length),
                },
            );
            self.reserved += length;
        }
        let entry = self
            .entries
            .get_mut(&key)
            .context("admitted fragment disappeared")?;
        entry.chunks[index] = Some(frame[HEADER..].to_vec());
        entry.received += 1;
        while entry.first_missing < entry.chunks.len()
            && entry.chunks[entry.first_missing].is_some()
        {
            entry.first_missing += 1;
        }
        Ok(FragmentAdmission::Accepted)
    }

    pub fn expire(&mut self, now: Instant) -> usize {
        self.completed
            .retain(|_, (_, created)| now.saturating_duration_since(*created) < self.limits.ttl);
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
                self.remember_completion(key.clone(), entry.length, now);
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
