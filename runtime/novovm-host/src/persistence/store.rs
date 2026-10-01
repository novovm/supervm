//! Single-owner candidate store. All content and the complete marker share ONE
//! AOEM atomic WAL-synchronous batch. No legacy slots, whole Store serialization,
//! per-transaction sync, intent catalog, implicit repair, or head promotion.

use super::packet::{marker_key, node_key, PacketBudget, PreparedCandidate, StoredCandidate};
use crate::execution::plan::BatchContext;
use crate::state::tree::{
    empty_root, validate_state_node_bytes, NodeHash, StagedStateUpdate, StateNodeReader,
};
use anyhow::{ensure, Context, Result};
use novovm_aoem::{StorageConfig, StorageSession, StorageWrite};
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

pub(super) const BULK_KEYS: usize = 64;
const FORMAT_KEY: &[u8] = b"novovm/replacement/candidate-store/format";
const FORMAT: &[u8] = b"novovm/replacement/candidate-store/v1\0";

/// Explicit configured scope, not an assertion that a parent is trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageDomain {
    pub chain_id: u64,
    pub genesis_config_commitment: NodeHash,
    pub protocol_commitment: NodeHash,
}

impl StorageDomain {
    fn encoding(self) -> Result<Vec<u8>> {
        ensure!(
            self.chain_id != 0
                && self.genesis_config_commitment != [0; 32]
                && self.protocol_commitment != [0; 32],
            "invalid storage domain"
        );
        let mut bytes = FORMAT.to_vec();
        bytes.extend_from_slice(&self.chain_id.to_be_bytes());
        bytes.extend_from_slice(&self.genesis_config_commitment);
        bytes.extend_from_slice(&self.protocol_commitment);
        Ok(bytes)
    }

    fn matches(self, context: &BatchContext) -> bool {
        self.chain_id == context.chain_id
            && self.genesis_config_commitment == context.genesis_config_commitment
            && self.protocol_commitment == context.protocol_commitment
    }
}

#[derive(Clone, Debug)]
pub struct StoreConfig {
    pub library: PathBuf,
    pub database: PathBuf,
    pub domain: StorageDomain,
    pub storage: StorageConfig,
    pub packet_budget: PacketBudget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// Exclusively create an absent directory. Its parent must already exist.
    CreateNew,
    /// Require the RocksDB CURRENT file and an exact in-database domain marker.
    /// The underlying ABI has no read-only/existing-only flag.
    Existing,
}

/// Observed durable local content, not chain status or an execution proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedCandidate {
    pub candidate_id: NodeHash,
    pub state_root: NodeHash,
    pub statement_commitment: NodeHash,
    pub document_digest: NodeHash,
    pub already_present: bool,
}

pub struct CandidateStore {
    pub(super) storage: RefCell<StorageSession>,
    domain: StorageDomain,
    prefix: Vec<u8>,
    budget: PacketBudget,
    storage_config: StorageConfig,
    pub(super) write_frozen: Cell<bool>,
}

impl CandidateStore {
    /// Blocking startup, on the eventual I/O-owner thread. A failure never
    /// removes or clears the directory; an incomplete initialization is not
    /// silently accepted as an existing store.
    pub fn open(config: StoreConfig, mode: OpenMode) -> Result<Self> {
        let format = config.domain.encoding()?;
        let prefix = [
            b"nv/candidate/v1/".as_slice(),
            Sha256::digest(&format).as_slice(),
        ]
        .concat();
        ensure!(
            config.packet_budget.max_bytes > 0,
            "empty candidate storage budget"
        );
        match mode {
            OpenMode::CreateNew => std::fs::create_dir(&config.database)
                .context("create new candidate database directory (must not exist)")?,
            OpenMode::Existing => ensure!(
                config.database.is_dir() && config.database.join("CURRENT").is_file(),
                "existing candidate database missing"
            ),
        }
        let mut storage = StorageSession::open(&config.library, &config.database, config.storage)?;
        match mode {
            OpenMode::CreateNew => {
                ensure!(
                    storage.get(FORMAT_KEY)?.is_none(),
                    "new store already initialized"
                );
                storage.atomic_write_batch(&[StorageWrite::Put {
                    key: FORMAT_KEY.to_vec(),
                    value: format.clone(),
                }])?;
                ensure!(
                    storage.get(FORMAT_KEY)?.as_ref() == Some(&format),
                    "store initialization readback mismatch"
                );
            }
            OpenMode::Existing => ensure!(
                storage.get(FORMAT_KEY)?.as_ref() == Some(&format),
                "candidate store format/domain missing or different"
            ),
        }
        Ok(Self {
            storage: RefCell::new(storage),
            domain: config.domain,
            prefix,
            budget: config.packet_budget,
            storage_config: config.storage,
            write_frozen: Cell::new(false),
        })
    }

    /// Deterministic scope mapping, also useful for explicit offline integrity
    /// checks. Never grants mutation or publication permission.
    pub fn scoped_key(&self, relative: &[u8]) -> Vec<u8> {
        [&self.prefix, relative].concat()
    }

    pub fn is_write_frozen(&self) -> bool {
        self.write_frozen.get() || self.storage.borrow().is_poisoned()
    }

    pub(super) fn writable(&self) -> Result<()> {
        ensure!(
            !self.is_write_frozen(),
            "candidate store write outcome unknown/corrupt; explicit recovery required"
        );
        Ok(())
    }

    pub(super) fn read_relative(&self, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>> {
        ensure!(
            keys.len() <= BULK_KEYS,
            "candidate bulk read exceeds key bound"
        );
        let keys: Vec<_> = keys.iter().map(|key| self.scoped_key(key)).collect();
        self.storage.borrow_mut().multi_get(&keys)
    }

    fn check_root(&self, root: NodeHash) -> Result<()> {
        if root != empty_root() {
            let bytes = self
                .read_node(&root)?
                .context("candidate root node unavailable")?;
            validate_state_node_bytes(&root, &bytes)?;
        }
        Ok(())
    }

    /// Startup/cold recovery only: bulk-reads this candidate, not inherited
    /// history. The configured chain must independently authorize its parent.
    /// Missing complete marker is None; present-but-damaged never auto-repairs.
    pub fn recover(&self, id: NodeHash) -> Result<Option<StoredCandidate>> {
        let candidate = StoredCandidate::load(id, self.budget, |keys| self.read_relative(keys))?;
        if let Some(candidate) = &candidate {
            ensure!(
                self.domain.matches(candidate.context()),
                "recovered candidate domain mismatch"
            );
            self.check_root(candidate.parent_state_root())?;
            self.check_root(candidate.state_root())?;
        }
        Ok(candidate)
    }

    /// Blocking convenience for startup/tests, never the node control loop.
    /// The I/O service drives this same state machine cooperatively instead.
    pub fn persist(&self, packet: &PreparedCandidate) -> Result<PersistedCandidate> {
        let mut progress = PersistProgress::new(self, packet)?;
        loop {
            if let Some(done) = progress.step(self, packet)? {
                return Ok(done);
            }
        }
    }

    /// Install a content-only empty-parent state during local initialization.
    /// This is NOT genesis activation, minting, an executed batch, or a trusted
    /// parent capability. No complete-candidate marker/head is manufactured.
    pub fn install_unpublished_state(&self, update: &StagedStateUpdate) -> Result<()> {
        self.writable()?;
        ensure!(
            update.parent_root() == empty_root(),
            "initial content must start from empty state"
        );
        ensure!(
            update.nodes().len() <= self.budget.max_nodes,
            "initial state node limit exceeded"
        );
        ensure!(
            update.root() == empty_root() || update.nodes().contains_key(&update.root()),
            "initial root missing"
        );
        let mut bytes = 0usize;
        let mut records = Vec::with_capacity(update.nodes().len());
        for (hash, value) in update.nodes() {
            validate_state_node_bytes(hash, value)?;
            ensure!(
                value.len() <= self.budget.max_value_bytes,
                "initial state value limit exceeded"
            );
            let key = node_key(*hash);
            bytes = bytes
                .checked_add(key.len() + value.len())
                .context("initial state size overflow")?;
            ensure!(
                bytes <= self.budget.max_bytes,
                "initial state byte limit exceeded"
            );
            records.push((key, value));
        }
        let mut writes = Vec::new();
        for chunk in records.chunks(BULK_KEYS) {
            let keys: Vec<_> = chunk.iter().map(|(key, _)| key.clone()).collect();
            for ((key, expected), actual) in chunk.iter().zip(self.read_relative(&keys)?) {
                match actual {
                    Some(value) => {
                        ensure!(&value == *expected, "immutable initial content conflict")
                    }
                    None => writes.push(StorageWrite::Put {
                        key: self.scoped_key(key),
                        value: (*expected).clone(),
                    }),
                }
            }
        }
        if !writes.is_empty() {
            self.storage.borrow_mut().atomic_write_batch(&writes)?;
        }
        for chunk in records.chunks(BULK_KEYS) {
            let keys: Vec<_> = chunk.iter().map(|(key, _)| key.clone()).collect();
            for ((_, expected), actual) in chunk.iter().zip(self.read_relative(&keys)?) {
                if actual.as_ref() != Some(*expected) {
                    self.write_frozen.set(true);
                    anyhow::bail!("initial content readback mismatch");
                }
            }
        }
        Ok(())
    }
}

impl StateNodeReader for CandidateStore {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        let value = self.read_relative(&[node_key(*hash)])?.pop().flatten();
        if let Some(bytes) = &value {
            validate_state_node_bytes(hash, bytes)?;
        }
        Ok(value)
    }
}

enum Phase {
    Start,
    Preflight,
    Write,
    Readback,
    Done,
}

/// Exactly one active writer per store. A step performs at most one bounded
/// content multi_get or one atomic write (Start also checks two root records).
/// The latter is deliberately indivisible and may take device-dependent time.
pub(super) struct PersistProgress {
    keys: Vec<Vec<u8>>,
    writes: Vec<StorageWrite>,
    offset: usize,
    phase: Phase,
    existed: bool,
}

impl PersistProgress {
    pub(super) fn new(store: &CandidateStore, packet: &PreparedCandidate) -> Result<Self> {
        store.writable()?;
        packet.validate_budget(store.budget)?;
        ensure!(
            store.domain.matches(packet.context()),
            "candidate storage domain mismatch"
        );
        let mut total = 0usize;
        let mut wire = 12usize + 12;
        for (key, value) in packet.records() {
            total = total
                .checked_add(key.len())
                .and_then(|n| n.checked_add(value.len()))
                .context("candidate record size overflow")?;
            let scoped_len = store.prefix.len() + key.len();
            ensure!(
                scoped_len <= store.storage_config.limits.max_key_bytes
                    && value.len() <= store.budget.max_value_bytes
                    && value.len() <= store.storage_config.limits.max_value_bytes,
                "candidate storage record bound exceeded"
            );
            wire = wire
                .checked_add(9 + scoped_len)
                .and_then(|n| n.checked_add(value.len()))
                .context("candidate write wire size overflow")?;
        }
        ensure!(
            total <= store.budget.max_bytes
                && packet.records().len() <= store.storage_config.limits.max_items
                && wire <= store.storage_config.limits.max_request_bytes,
            "candidate atomic batch budget exceeded"
        );
        // Packet construction already validates all codec/semantic bounds. A
        // smaller store profile must not silently accept its larger packet.
        ensure!(packet.context().height > 0, "invalid candidate height");
        Ok(Self {
            keys: packet.records().keys().cloned().collect(),
            writes: Vec::new(),
            offset: 0,
            phase: Phase::Start,
            existed: false,
        })
    }

    pub(super) fn step(
        &mut self,
        store: &CandidateStore,
        packet: &PreparedCandidate,
    ) -> Result<Option<PersistedCandidate>> {
        store.writable()?;
        match self.phase {
            Phase::Start => {
                store.check_root(packet.parent_state_root())?;
                if !packet
                    .records()
                    .contains_key(&node_key(packet.state_root()))
                {
                    store.check_root(packet.state_root())?;
                }
                let marker = marker_key(packet.candidate_id());
                if let Some(actual) = store
                    .read_relative(std::slice::from_ref(&marker))?
                    .pop()
                    .flatten()
                {
                    ensure!(
                        Some(&actual) == packet.records().get(&marker),
                        "candidate identity has a different completion"
                    );
                    self.existed = true;
                }
                // Even exact replay may not ignore an unexpected document tail.
                let documents = self
                    .keys
                    .iter()
                    .filter(|key| key.first() == Some(&b'd'))
                    .count();
                let tail =
                    super::packet::document_key(packet.candidate_id(), u32::try_from(documents)?);
                ensure!(
                    store.read_relative(&[tail])?.pop().flatten().is_none(),
                    "candidate has an unexpected document tail"
                );
                self.phase = Phase::Preflight;
            }
            Phase::Preflight | Phase::Readback => {
                let readback = matches!(self.phase, Phase::Readback);
                let end = (self.offset + BULK_KEYS).min(self.keys.len());
                let keys = &self.keys[self.offset..end];
                for (key, actual) in keys.iter().zip(store.read_relative(keys)?) {
                    let expected = &packet.records()[key];
                    if readback {
                        if actual.as_ref() != Some(expected) {
                            store.write_frozen.set(true);
                            anyhow::bail!("completed candidate content readback mismatch");
                        }
                    } else {
                        match actual {
                            Some(value) => {
                                ensure!(&value == expected, "immutable candidate content conflict")
                            }
                            None => {
                                ensure!(
                                    !self.existed,
                                    "completed candidate content missing; refusing repair"
                                );
                                self.writes.push(StorageWrite::Put {
                                    key: store.scoped_key(key),
                                    value: expected.clone(),
                                });
                            }
                        }
                    }
                }
                self.offset = end;
                if end == self.keys.len() {
                    self.offset = 0;
                    self.phase = if readback { Phase::Done } else { Phase::Write };
                }
            }
            Phase::Write => {
                if !self.writes.is_empty() {
                    store
                        .storage
                        .borrow_mut()
                        .atomic_write_batch(&self.writes)?;
                }
                self.writes.clear();
                self.phase = Phase::Readback;
            }
            Phase::Done => {
                return Ok(Some(PersistedCandidate {
                    candidate_id: packet.candidate_id(),
                    state_root: packet.state_root(),
                    statement_commitment: packet.statement_commitment(),
                    document_digest: packet.document_digest(),
                    already_present: self.existed,
                }))
            }
        }
        Ok(None)
    }
}
