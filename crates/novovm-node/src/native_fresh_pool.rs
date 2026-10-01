use super::*;
use rocksdb::{IteratorMode, Options, WriteBatch, WriteOptions, DB};
use std::collections::BTreeMap;

pub const MAX_RAW_BYTES: usize = 64 * 1024;
const MAX_ENTRIES: usize = 1024;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_PER_SIGNER: usize = 64;

#[derive(Clone)]
pub struct PendingTransaction {
    pub(crate) hash: [u8; 32],
    pub(crate) raw: Vec<u8>,
    pub(crate) identity: String,
    pub(crate) nonce: u64,
}

impl PendingTransaction {
    pub fn authenticate(raw: Vec<u8>, chain: u64, params: &serde_json::Value) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_RAW_BYTES {
            bail!("transaction size limit");
        }
        let transaction = decode_nov_native_tx_wire_v1(&raw)?;
        if transaction.chain_id != chain {
            bail!("transaction chain or execution kind mismatch");
        }
        native_transfer_dispatch::require_execution_capability_v1(&transaction, true)?;
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&transaction)?;
        let hash = tx_hash_array_from_ir_v1(&ir);
        verify_nov_native_auth_v1(params, &transaction, &ir, hash)?;
        let reservation = nov_native_durable_auth_reservation_v1(&transaction, &ir, hash)?;
        Ok(Self {
            hash,
            raw,
            identity: reservation.identity_key,
            nonce: reservation.nonce,
        })
    }
}

pub struct FreshTransactionPool {
    db: DB,
    entries: BTreeMap<[u8; 32], PendingTransaction>,
    bytes: usize,
}

pub(crate) struct BatchAdmission {
    pub(crate) rejected: u64,
    /// One result per original input, never a hash-only acknowledgement map.
    pub(crate) retained: Vec<bool>,
}

impl FreshTransactionPool {
    pub fn open(
        path: &Path,
        chain: u64,
        genesis: [u8; 32],
        params: &serde_json::Value,
    ) -> Result<Self> {
        let existing = path.join("CURRENT").exists();
        if path.exists() && !existing && std::fs::read_dir(path)?.next().is_some() {
            bail!("transaction pool directory is occupied or incomplete");
        }
        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, path)?;
        let identity = serde_json::to_vec(&("novovm-fresh-transaction-pool/v1", chain, genesis))?;
        match db.get(b"identity")? {
            Some(previous) if previous != identity => bail!("transaction pool identity mismatch"),
            None => {
                if existing || db.iterator(IteratorMode::Start).next().is_some() {
                    bail!("transaction pool identity missing");
                }
                let mut sync = WriteOptions::default();
                sync.set_sync(true);
                db.put_opt(b"identity", &identity, &sync)?;
            }
            _ => (),
        }
        let mut entries = BTreeMap::new();
        let mut bytes = 0usize;
        let mut nonces = std::collections::BTreeSet::new();
        let mut counts = BTreeMap::new();
        for item in db.iterator(IteratorMode::Start) {
            let (key, raw) = item?;
            if key.as_ref() == b"identity" {
                continue;
            }
            if key.len() != 33
                || key[0] != b't'
                || entries.len() >= MAX_ENTRIES
                || raw.len() > MAX_RAW_BYTES
            {
                bail!("invalid transaction pool record");
            }
            let entry = PendingTransaction::authenticate(raw.to_vec(), chain, params)?;
            if key[1..] != entry.hash || !nonces.insert((entry.identity.clone(), entry.nonce)) {
                bail!("transaction pool record binding mismatch");
            }
            let count = counts.entry(entry.identity.clone()).or_insert(0usize);
            *count += 1;
            bytes = bytes
                .checked_add(entry.raw.len())
                .context("pool size overflow")?;
            if bytes > MAX_BYTES || *count > MAX_PER_SIGNER {
                bail!("transaction pool capacity exceeded");
            }
            entries.insert(entry.hash, entry);
        }
        Ok(Self { db, entries, bytes })
    }

    pub(crate) fn insert(&mut self, entry: PendingTransaction) -> Result<bool> {
        if let Some(previous) = self.entries.get(&entry.hash) {
            return Ok(previous.raw == entry.raw);
        }
        if self.entries.len() >= MAX_ENTRIES
            || self.bytes + entry.raw.len() > MAX_BYTES
            || self
                .entries
                .values()
                .filter(|other| other.identity == entry.identity)
                .count()
                >= MAX_PER_SIGNER
            || self
                .entries
                .values()
                .any(|other| other.identity == entry.identity && other.nonce == entry.nonce)
        {
            return Ok(false);
        }
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        let mut key = vec![b't'];
        key.extend_from_slice(&entry.hash);
        self.db.put_opt(key, &entry.raw, &sync)?;
        self.bytes += entry.raw.len();
        self.entries.insert(entry.hash, entry);
        Ok(true)
    }

    /// Admit already authenticated and transport-bound entries. Finalized
    /// replays are filtered before any synchronous pool write, not put/deleted
    /// on every gossip round. The immutable parent only filters stale entries;
    /// candidate execution still revalidates current authority and nonce.
    #[cfg(test)]
    pub(crate) fn insert_live_batch(
        &mut self,
        entries: Vec<PendingTransaction>,
        parent: Option<&candidate_workspace::FinalizedParentViewV1>,
        params: &serde_json::Value,
    ) -> Result<u64> {
        self.insert_live_batch_with_retention(entries, parent, params)
            .map(|admission| admission.rejected)
    }

    /// Report only entries durably present with these exact raw bytes. Filtered
    /// finalized/consumed inputs are not retained. A read or write failure
    /// returns no admission result, so callers must not acknowledge a prefix.
    pub(crate) fn insert_live_batch_with_retention(
        &mut self,
        entries: Vec<PendingTransaction>,
        parent: Option<&candidate_workspace::FinalizedParentViewV1>,
        params: &serde_json::Value,
    ) -> Result<BatchAdmission> {
        let mut admission = BatchAdmission {
            rejected: 0,
            retained: vec![false; entries.len()],
        };
        let entries: Vec<_> = entries
            .into_iter()
            .enumerate()
            .filter(|(index, entry)| {
                if let Some(previous) = self.entries.get(&entry.hash) {
                    // Hash alone must never authorize a different raw payload.
                    if previous.raw == entry.raw {
                        admission.retained[*index] = true;
                    } else {
                        admission.rejected = admission.rejected.saturating_add(1);
                    }
                    false
                } else {
                    true
                }
            })
            .collect();
        if entries.is_empty() {
            return Ok(admission);
        }
        let live = if let Some(parent) = parent {
            // Finish ALL reads before insertion: damaged data must not be
            // treated as missing/zero or leave a durably admitted prefix.
            parent.with_records(params, |reader| {
                let mut live = Vec::new();
                for (index, entry) in entries {
                    if !reader.contains_receipt(&entry.hash)?
                        && reader.next_nonce(&entry.identity)? <= entry.nonce
                    {
                        live.push((index, entry));
                    }
                }
                Ok(live)
            })?
        } else {
            entries
        };
        for (index, entry) in live {
            if self.insert(entry)? {
                admission.retained[index] = true;
            } else {
                admission.rejected = admission.rejected.saturating_add(1);
            }
        }
        Ok(admission)
    }

    #[cfg(test)]
    pub(crate) fn write_sequence_for_test(&self) -> u64 {
        self.db.latest_sequence_number()
    }

    pub fn reconcile(
        &mut self,
        parent: &candidate_workspace::FinalizedGenesisParentV1,
    ) -> Result<()> {
        let retired: Vec<_> = self
            .entries
            .values()
            .filter(|entry| {
                parent.state().receipts.contains_key(&to_hex(&entry.hash))
                    || parent
                        .state()
                        .module_state
                        .native_auth_next_nonces
                        .get(&entry.identity)
                        .copied()
                        .unwrap_or(0)
                        > entry.nonce
            })
            .map(|entry| entry.hash)
            .collect();
        self.retire_entries(retired)
    }

    pub(crate) fn reconcile_rooted(
        &mut self,
        parent: &candidate_workspace::FinalizedParentViewV1,
        params: &serde_json::Value,
    ) -> Result<()> {
        if self.entries.is_empty() {
            return Ok(());
        }
        // One scoped reader for the whole batch. Read failure must not delete
        // an earlier prefix or be interpreted as a missing receipt/zero nonce.
        let retired = parent.with_records(params, |reader| {
            let mut retired = Vec::new();
            for entry in self.entries.values() {
                if reader.contains_receipt(&entry.hash)?
                    || reader.next_nonce(&entry.identity)? > entry.nonce
                {
                    retired.push(entry.hash);
                }
            }
            Ok(retired)
        })?;
        self.retire_entries(retired)
    }

    fn retire_entries(&mut self, retired: Vec<[u8; 32]>) -> Result<()> {
        if retired.is_empty() {
            return Ok(());
        }
        let mut batch = WriteBatch::default();
        for hash in &retired {
            let mut key = vec![b't'];
            key.extend_from_slice(hash);
            batch.delete(key);
        }
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        self.db.write_opt(batch, &sync)?;
        for hash in retired {
            if let Some(entry) = self.entries.remove(&hash) {
                self.bytes -= entry.raw.len();
            }
        }
        Ok(())
    }

    pub fn ordered(&self) -> Vec<PendingTransaction> {
        self.ordered_refs().into_iter().cloned().collect()
    }

    pub(crate) fn ordered_refs(&self) -> Vec<&PendingTransaction> {
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by(|left, right| {
            (&left.identity, left.nonce).cmp(&(&right.identity, right.nonce))
        });
        entries
    }

    pub fn contains(&self, hash: &[u8; 32]) -> bool {
        self.entries.contains_key(hash)
    }
    pub(crate) fn get(&self, hash: &[u8; 32]) -> Option<&PendingTransaction> {
        self.entries.get(hash)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
