//! Immutable execution output packaging, not a second executor or a finality proof.
//!
//! Only `ExecutedNovBatch` enters the production preparation API. The complete
//! original plan/body, output roots, ordered receipts and new-node manifest are
//! bound together. Recovery creates `StoredCandidate`, never an authenticated or
//! executed capability. It validates this packet's content, not all inherited
//! parent history. The store owns scope isolation, trusted parent checks and the
//! single durable atomic write containing ALL records including the marker.

use crate::business::direct_nov_fee::{FeeFailure, FeeQuote, ThresholdState};
use crate::business::nov_transfer_batch::{
    program_id, receipt_codec, ExecutedNovBatch, SEMANTIC_VERSION,
};
use crate::execution::plan::{BatchContext, BatchPlan, PlanBudget};
use crate::ingress::wire::{canonical_tx_hash, decode_transfer_v3};
use crate::state::frontier::DeclaredAccess;
use crate::state::tree::{validate_state_node_bytes, NodeHash};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const DOCUMENT_MAGIC: &[u8; 8] = b"NVCAND01";
const MARKER_MAGIC: &[u8; 8] = b"NVCOMP01";
const MARKER_BYTES: usize = 8 + 32 + 8 + 4 + 32;
const CHUNK_BYTES: usize = 64 * 1024;
const READ_BATCH_KEYS: usize = 64;
const TREE_MAX_NODES: usize = 65_536;

/// Local resource limits, not protocol activation parameters. Chunk layout is
/// fixed independently of these limits so replay under a larger budget is exact.
#[derive(Clone, Copy, Debug)]
pub struct PacketBudget {
    pub max_bytes: usize,
    pub max_value_bytes: usize,
    pub max_nodes: usize,
    pub max_transactions: usize,
    pub max_transaction_bytes: usize,
    pub max_access_keys: usize,
    pub max_receipt_bytes: usize,
}

impl Default for PacketBudget {
    fn default() -> Self {
        Self {
            max_bytes: 32 * 1024 * 1024,
            max_value_bytes: 1024 * 1024,
            max_nodes: TREE_MAX_NODES,
            max_transactions: 1024,
            max_transaction_bytes: 1024 * 1024,
            max_access_keys: 4096,
            max_receipt_bytes: 1024 * 1024,
        }
    }
}

impl PacketBudget {
    fn plan(self) -> PlanBudget {
        PlanBudget {
            transactions: self.max_transactions,
            transaction_bytes: self.max_transaction_bytes,
            body_bytes: self.max_bytes,
            access_keys: self.max_access_keys,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Summary {
    context: BatchContext,
    candidate_id: NodeHash,
    state_root: NodeHash,
    receipt_batch_commitment: NodeHash,
    statement_commitment: NodeHash,
    document_digest: NodeHash,
}

/// Private, immutable records. There is no decoder/constructor from arbitrary
/// staged tree patches or caller-paired receipt/body metadata.
pub struct PreparedCandidate {
    summary: Summary,
    records: BTreeMap<Vec<u8>, Vec<u8>>,
    resources: Resources,
}

struct Resources {
    transactions: usize,
    max_transaction_bytes: usize,
    access_keys: usize,
    nodes: usize,
    max_receipt_bytes: usize,
    max_value_bytes: usize,
    total_bytes: usize,
}

impl PreparedCandidate {
    pub fn from_executed(batch: ExecutedNovBatch, budget: PacketBudget) -> Result<Self> {
        Self::prepare_executed(&batch, budget)
    }

    pub(crate) fn from_executed_with_seed(
        batch: ExecutedNovBatch,
        budget: PacketBudget,
        capture: crate::state::frontier::CaptureBudget,
    ) -> Result<(Self, Option<crate::state::frontier::PostStateSeed>)> {
        let packet = Self::prepare_executed(&batch, budget)?;
        let seed = batch.into_poststate_seed(capture)?;
        Ok((packet, seed))
    }

    fn prepare_executed(batch: &ExecutedNovBatch, budget: PacketBudget) -> Result<Self> {
        let effects = batch.effects();
        ensure!(
            effects.update().parent_root() == effects.context().parent_state_root,
            "executed candidate parent root mismatch"
        );
        let receipts = batch
            .receipts()
            .iter()
            .map(postcard::to_allocvec)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        prepare(
            effects.plan(),
            effects.update().root(),
            &receipts,
            effects.update().nodes(),
            batch.receipt_batch_commitment(),
            batch.statement_commitment(),
            budget,
        )
    }

    /// Keys are storage-neutral. The store MUST prepend its configured scope;
    /// `n` node keys share immutable content while `d`/`c` keys bind a candidate.
    pub fn records(&self) -> &BTreeMap<Vec<u8>, Vec<u8>> {
        &self.records
    }
    /// Storage-neutral keys plus values; a store must additionally account for
    /// any prefix bytes that it adds to each key.
    pub fn record_bytes(&self) -> usize {
        self.resources.total_bytes
    }
    pub fn transaction_count(&self) -> usize {
        self.resources.transactions
    }
    pub fn context(&self) -> &BatchContext {
        &self.summary.context
    }
    pub fn candidate_id(&self) -> NodeHash {
        self.summary.candidate_id
    }
    pub fn plan_commitment(&self) -> NodeHash {
        self.summary.candidate_id
    }
    pub fn state_root(&self) -> NodeHash {
        self.summary.state_root
    }
    pub fn parent_state_root(&self) -> NodeHash {
        self.summary.context.parent_state_root
    }
    pub fn receipt_batch_commitment(&self) -> NodeHash {
        self.summary.receipt_batch_commitment
    }
    pub fn statement_commitment(&self) -> NodeHash {
        self.summary.statement_commitment
    }
    pub fn document_digest(&self) -> NodeHash {
        self.summary.document_digest
    }
    pub fn matches(&self, stored: &StoredCandidate) -> bool {
        self.summary == stored.summary
    }
    /// Recheck a store's potentially stricter limits without decoding, executing
    /// or rehashing immutable content. Counts were measured during preparation.
    pub fn validate_budget(&self, budget: PacketBudget) -> Result<()> {
        let r = &self.resources;
        ensure!(
            r.transactions <= budget.max_transactions
                && r.max_transaction_bytes <= budget.max_transaction_bytes
                && r.access_keys <= budget.max_access_keys
                && r.nodes <= budget.max_nodes.min(TREE_MAX_NODES)
                && r.max_receipt_bytes <= budget.max_receipt_bytes
                && r.max_value_bytes <= budget.max_value_bytes
                && r.total_bytes <= budget.max_bytes,
            "prepared candidate exceeds store budget"
        );
        Ok(())
    }
}

/// Independently read local content, not proof of business execution or current
/// chain authority. No API turns this into ExecutedNovBatch/SignatureCheckedInput.
pub struct StoredCandidate {
    summary: Summary,
    plan: BatchPlan,
    receipts: Vec<Vec<u8>>,
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    record_bytes: usize,
}

impl StoredCandidate {
    /// Fresh bulk reads (at most 64 keys per callback); no per-key thread RPC,
    /// cached readback, whole-history scan, repair or business re-execution.
    /// None means no marker only. A present marker with missing content is Err.
    pub fn load(
        candidate_id: NodeHash,
        budget: PacketBudget,
        mut read_batch: impl FnMut(&[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>>,
    ) -> Result<Option<Self>> {
        let key = marker_key(candidate_id);
        let mut read = read_exact_batch(&mut read_batch, std::slice::from_ref(&key))?;
        let Some(marker) = read.pop().flatten() else {
            return Ok(None);
        };
        let header = Marker::decode(&marker, candidate_id, budget)?;
        let mut total = 0;
        account_record(&mut total, &key, &marker, budget)?;
        let keys: Vec<_> = (0..header.chunks)
            .map(|index| document_key(candidate_id, index))
            .collect();
        let mut document = Vec::with_capacity(header.len);
        for batch in keys.chunks(READ_BATCH_KEYS) {
            for (key, value) in batch.iter().zip(read_exact_batch(&mut read_batch, batch)?) {
                let value = value.context("completed candidate document chunk missing")?;
                ensure!(
                    value.len() == CHUNK_BYTES.min(header.len - document.len()),
                    "candidate chunk length mismatch"
                );
                account_record(&mut total, key, &value, budget)?;
                document.extend_from_slice(&value);
            }
        }
        ensure!(
            document.len() == header.len && document_hash(&document) == header.digest,
            "candidate document length/digest mismatch"
        );
        // A candidate's immutable namespace cannot contain an unbound tail page.
        let tail = document_key(candidate_id, header.chunks);
        ensure!(
            read_exact_batch(&mut read_batch, &[tail])?
                .pop()
                .flatten()
                .is_none(),
            "candidate document has an unexpected tail chunk"
        );
        let decoded = decode_document(&document, candidate_id, budget)?;
        let mut nodes = BTreeMap::new();
        for hashes in decoded.node_hashes.chunks(READ_BATCH_KEYS) {
            let keys: Vec<_> = hashes.iter().map(|hash| node_key(*hash)).collect();
            for ((hash, key), value) in hashes
                .iter()
                .zip(&keys)
                .zip(read_exact_batch(&mut read_batch, &keys)?)
            {
                let bytes = value.context("completed candidate state node missing")?;
                validate_state_node_bytes(hash, &bytes)?;
                account_record(&mut total, key, &bytes, budget)?;
                nodes.insert(*hash, bytes);
            }
        }
        Ok(Some(Self {
            summary: Summary {
                context: *decoded.plan.context(),
                candidate_id,
                state_root: decoded.state_root,
                receipt_batch_commitment: decoded.receipt_commitment,
                statement_commitment: decoded.statement_commitment,
                document_digest: header.digest,
            },
            plan: decoded.plan,
            receipts: decoded.receipts,
            nodes,
            record_bytes: total,
        }))
    }

    pub fn context(&self) -> &BatchContext {
        &self.summary.context
    }
    pub fn candidate_id(&self) -> NodeHash {
        self.summary.candidate_id
    }
    pub fn plan_commitment(&self) -> NodeHash {
        self.summary.candidate_id
    }
    pub fn state_root(&self) -> NodeHash {
        self.summary.state_root
    }
    pub fn parent_state_root(&self) -> NodeHash {
        self.summary.context.parent_state_root
    }
    pub fn receipt_batch_commitment(&self) -> NodeHash {
        self.summary.receipt_batch_commitment
    }
    pub fn statement_commitment(&self) -> NodeHash {
        self.summary.statement_commitment
    }
    pub fn document_digest(&self) -> NodeHash {
        self.summary.document_digest
    }
    pub fn raw_transactions(&self) -> &[Vec<u8>] {
        self.plan.raw_transactions()
    }
    pub fn declared_access(&self) -> &[DeclaredAccess] {
        self.plan.declared_access()
    }
    pub fn receipt_bytes(&self) -> &[Vec<u8>] {
        &self.receipts
    }
    pub fn nodes(&self) -> &BTreeMap<NodeHash, Vec<u8>> {
        &self.nodes
    }
    /// Cached sum of the independently loaded record keys and values. This
    /// conservative logical-content accounting is not allocator/RSS usage.
    /// Retaining or retiring an archive need not rescan every raw transaction,
    /// receipt and state node on a control thread.
    pub fn record_bytes(&self) -> usize {
        self.record_bytes
    }
}

pub fn marker_key(candidate_id: NodeHash) -> Vec<u8> {
    [b"c".as_slice(), &candidate_id].concat()
}
pub fn node_key(hash: NodeHash) -> Vec<u8> {
    [b"n".as_slice(), &hash].concat()
}
pub(crate) fn document_key(candidate_id: NodeHash, index: u32) -> Vec<u8> {
    [b"d".as_slice(), &candidate_id, &index.to_be_bytes()].concat()
}

fn read_exact_batch(
    read: &mut impl FnMut(&[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>>,
    keys: &[Vec<u8>],
) -> Result<Vec<Option<Vec<u8>>>> {
    ensure!(
        keys.len() <= READ_BATCH_KEYS,
        "candidate read exceeds bulk key bound"
    );
    let values = read(keys)?;
    ensure!(
        values.len() == keys.len(),
        "candidate bulk read result count mismatch"
    );
    Ok(values)
}

fn account_record(total: &mut usize, key: &[u8], value: &[u8], budget: PacketBudget) -> Result<()> {
    ensure!(
        value.len() <= budget.max_value_bytes,
        "candidate record exceeds value budget"
    );
    *total = total
        .checked_add(key.len())
        .and_then(|n| n.checked_add(value.len()))
        .context("candidate record byte count overflow")?;
    ensure!(
        *total <= budget.max_bytes,
        "candidate records exceed total byte budget"
    );
    Ok(())
}

struct Marker {
    len: usize,
    chunks: u32,
    digest: NodeHash,
}

impl Marker {
    fn encode(&self, candidate_id: NodeHash) -> Vec<u8> {
        let mut bytes = MARKER_MAGIC.to_vec();
        bytes.extend_from_slice(&candidate_id);
        bytes.extend_from_slice(&(self.len as u64).to_be_bytes());
        bytes.extend_from_slice(&self.chunks.to_be_bytes());
        bytes.extend_from_slice(&self.digest);
        bytes
    }
    fn decode(bytes: &[u8], id: NodeHash, budget: PacketBudget) -> Result<Self> {
        ensure!(
            bytes.len() == MARKER_BYTES && bytes.starts_with(MARKER_MAGIC),
            "invalid candidate completion codec"
        );
        let mut reader = Reader::new(&bytes[8..]);
        ensure!(
            reader.hash()? == id,
            "candidate completion identity mismatch"
        );
        let len = usize::try_from(reader.u64()?)?;
        let chunks = reader.u32()?;
        let digest = reader.hash()?;
        ensure!(
            len > 0
                && len <= budget.max_bytes
                && usize::try_from(chunks)? == len.div_ceil(CHUNK_BYTES),
            "candidate completion length/chunk bounds mismatch"
        );
        ensure!(
            CHUNK_BYTES.min(len) <= budget.max_value_bytes,
            "candidate document chunk exceeds value budget"
        );
        reader.finish()?;
        Ok(Self {
            len,
            chunks,
            digest,
        })
    }
}

fn document_hash(bytes: &[u8]) -> NodeHash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/candidate-document/v1\0");
    hash.update(bytes);
    hash.finalize().into()
}

fn receipt_hash(codec: NodeHash, receipts: &[Vec<u8>]) -> NodeHash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/nov-receipt-batch/v1\0");
    hash.update(codec);
    hash.update((receipts.len() as u64).to_be_bytes());
    for receipt in receipts {
        hash.update((receipt.len() as u64).to_be_bytes());
        hash.update(receipt);
    }
    hash.finalize().into()
}

fn statement_hash(plan: NodeHash, root: NodeHash, receipts: NodeHash) -> NodeHash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/nov-executed-batch/v1\0");
    hash.update(plan);
    hash.update(root);
    hash.update(receipts);
    hash.finalize().into()
}

fn prepare(
    plan: &BatchPlan,
    root: NodeHash,
    receipts: &[Vec<u8>],
    nodes: &BTreeMap<NodeHash, Vec<u8>>,
    receipt_commitment: NodeHash,
    statement_commitment: NodeHash,
    budget: PacketBudget,
) -> Result<PreparedCandidate> {
    ensure!(
        nodes.len() <= budget.max_nodes.min(TREE_MAX_NODES),
        "candidate node count exceeds budget"
    );
    let mut document = Encoder::new(budget.max_bytes);
    document.put(DOCUMENT_MAGIC)?;
    document.put(&plan.commitment())?;
    encode_plan(&mut document, plan, budget)?;
    document.put(&root)?;
    document.put(&receipt_commitment)?;
    document.put(&statement_commitment)?;
    document.u64(nodes.len() as u64)?;
    for (hash, bytes) in nodes {
        validate_state_node_bytes(hash, bytes)?;
        document.put(hash)?;
    }
    document.u64(receipts.len() as u64)?;
    ensure!(
        receipts.len() == plan.raw_transactions().len(),
        "candidate receipt count mismatch"
    );
    for bytes in receipts {
        ensure!(
            !bytes.is_empty() && bytes.len() <= budget.max_receipt_bytes,
            "candidate receipt exceeds budget"
        );
        document.frame(bytes)?;
    }
    let document = document.bytes;
    // The codec's independent recovery validator also checks structural receipt
    // relations, not business execution. Preparation never manufactures success.
    decode_document(&document, plan.commitment(), budget)?;
    let digest = document_hash(&document);
    let mut records = BTreeMap::new();
    for (hash, bytes) in nodes {
        records.insert(node_key(*hash), bytes.clone());
    }
    for (index, bytes) in document.chunks(CHUNK_BYTES).enumerate() {
        records.insert(
            document_key(plan.commitment(), u32::try_from(index)?),
            bytes.to_vec(),
        );
    }
    records.insert(
        marker_key(plan.commitment()),
        Marker {
            len: document.len(),
            chunks: u32::try_from(document.len().div_ceil(CHUNK_BYTES))?,
            digest,
        }
        .encode(plan.commitment()),
    );
    let mut total = 0;
    for (key, value) in &records {
        account_record(&mut total, key, value, budget)?;
    }
    let resources = Resources {
        transactions: plan.raw_transactions().len(),
        max_transaction_bytes: plan
            .raw_transactions()
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(0),
        access_keys: plan.declared_access().len(),
        nodes: nodes.len(),
        max_receipt_bytes: receipts.iter().map(Vec::len).max().unwrap_or(0),
        max_value_bytes: records.values().map(Vec::len).max().unwrap_or(0),
        total_bytes: total,
    };
    Ok(PreparedCandidate {
        summary: Summary {
            context: *plan.context(),
            candidate_id: plan.commitment(),
            state_root: root,
            receipt_batch_commitment: receipt_commitment,
            statement_commitment,
            document_digest: digest,
        },
        records,
        resources,
    })
}

struct Encoder {
    bytes: Vec<u8>,
    limit: usize,
}
impl Encoder {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            self.bytes
                .len()
                .checked_add(bytes.len())
                .is_some_and(|len| len <= self.limit),
            "candidate document exceeds budget"
        );
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn u64(&mut self, value: u64) -> Result<()> {
        self.put(&value.to_be_bytes())
    }
    fn frame(&mut self, bytes: &[u8]) -> Result<()> {
        self.u64(u64::try_from(bytes.len())?)?;
        self.put(bytes)
    }
}

struct Reader<'a> {
    remaining: &'a [u8],
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        ensure!(len <= self.remaining.len(), "candidate document truncated");
        let (bytes, remaining) = self.remaining.split_at(len);
        self.remaining = remaining;
        Ok(bytes)
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn hash(&mut self) -> Result<NodeHash> {
        Ok(self.take(32)?.try_into()?)
    }
    fn count(&mut self, bound: usize, minimum_bytes: usize) -> Result<usize> {
        let count = usize::try_from(self.u64()?)?;
        ensure!(
            count <= bound && count <= self.remaining.len() / minimum_bytes,
            "candidate count exceeds budget or remaining content"
        );
        Ok(count)
    }
    fn frame(&mut self, bound: usize) -> Result<&'a [u8]> {
        let len = usize::try_from(self.u64()?)?;
        ensure!(
            len > 0 && len <= bound,
            "candidate field length exceeds budget or empty"
        );
        self.take(len)
    }
    fn finish(&self) -> Result<()> {
        ensure!(
            self.remaining.is_empty(),
            "candidate document trailing bytes"
        );
        Ok(())
    }
}

fn encode_plan(out: &mut Encoder, plan: &BatchPlan, budget: PacketBudget) -> Result<()> {
    let c = plan.context();
    ensure!(
        plan.raw_transactions().len() <= budget.max_transactions
            && plan.declared_access().len() <= budget.max_access_keys,
        "candidate plan counts exceed budget"
    );
    out.u64(c.chain_id)?;
    for hash in [
        c.genesis_config_commitment,
        c.protocol_commitment,
        c.business_program,
    ] {
        out.put(&hash)?;
    }
    out.put(&c.semantic_version.to_be_bytes())?;
    out.put(&c.effect_contract)?;
    out.put(&c.parent_block_hash)?;
    out.u64(c.parent_height)?;
    out.put(&c.parent_state_root)?;
    out.put(&c.parent_receipt_root)?;
    out.u64(c.parent_state_version)?;
    out.put(&c.receipt_codec)?;
    out.u64(c.height)?;
    out.u64(c.slot)?;
    out.u64(c.timestamp_unix_ms)?;
    out.u64(plan.raw_transactions().len() as u64)?;
    for raw in plan.raw_transactions() {
        ensure!(
            raw.len() <= budget.max_transaction_bytes,
            "candidate transaction exceeds byte budget"
        );
        out.frame(raw)?;
    }
    out.u64(plan.declared_access().len() as u64)?;
    for access in plan.declared_access() {
        out.frame(&access.key)?;
        out.put(&[u8::from(access.may_put) | (u8::from(access.may_delete) << 1)])?;
    }
    Ok(())
}

fn decode_plan(input: &mut Reader<'_>, budget: PacketBudget) -> Result<BatchPlan> {
    let context = BatchContext {
        chain_id: input.u64()?,
        genesis_config_commitment: input.hash()?,
        protocol_commitment: input.hash()?,
        business_program: input.hash()?,
        semantic_version: input.u32()?,
        effect_contract: input.hash()?,
        parent_block_hash: input.hash()?,
        parent_height: input.u64()?,
        parent_state_root: input.hash()?,
        parent_receipt_root: input.hash()?,
        parent_state_version: input.u64()?,
        receipt_codec: input.hash()?,
        height: input.u64()?,
        slot: input.u64()?,
        timestamp_unix_ms: input.u64()?,
    };
    ensure!(
        context.business_program == program_id()
            && context.receipt_codec == receipt_codec()
            && context.semantic_version == SEMANTIC_VERSION,
        "unsupported candidate business/receipt profile"
    );
    let count = input.count(budget.max_transactions, 9)?;
    let mut raw = Vec::with_capacity(count);
    for _ in 0..count {
        raw.push(input.frame(budget.max_transaction_bytes)?.to_vec());
    }
    let count = input.count(budget.max_access_keys, 10)?;
    let mut access = Vec::with_capacity(count);
    for _ in 0..count {
        let key = input.frame(256)?.to_vec();
        if let Some(previous) = access.last() {
            let previous: &DeclaredAccess = previous;
            ensure!(
                previous.key < key,
                "candidate access keys not canonical unique order"
            );
        }
        let flags = input.take(1)?[0];
        ensure!(flags & !3 == 0, "candidate access flags invalid");
        access.push(DeclaredAccess {
            key,
            may_put: flags & 1 != 0,
            may_delete: flags & 2 != 0,
        });
    }
    BatchPlan::new(context, raw, access, budget.plan())
}

struct Decoded {
    plan: BatchPlan,
    state_root: NodeHash,
    receipt_commitment: NodeHash,
    statement_commitment: NodeHash,
    node_hashes: Vec<NodeHash>,
    receipts: Vec<Vec<u8>>,
}

fn decode_document(bytes: &[u8], id: NodeHash, budget: PacketBudget) -> Result<Decoded> {
    ensure!(
        bytes.len() <= budget.max_bytes,
        "candidate document exceeds budget"
    );
    let mut input = Reader::new(bytes);
    ensure!(
        input.take(8)? == DOCUMENT_MAGIC,
        "unsupported candidate document codec"
    );
    ensure!(input.hash()? == id, "candidate document identity mismatch");
    let plan = decode_plan(&mut input, budget)?;
    ensure!(
        plan.commitment() == id,
        "candidate reconstructed plan commitment mismatch"
    );
    let state_root = input.hash()?;
    ensure!(
        state_root != [0; 32],
        "candidate output root must be nonzero"
    );
    let receipt_commitment = input.hash()?;
    let statement_commitment = input.hash()?;
    let count = input.count(budget.max_nodes.min(TREE_MAX_NODES), 32)?;
    let mut node_hashes = Vec::with_capacity(count);
    for _ in 0..count {
        let hash = input.hash()?;
        ensure!(
            node_hashes.last().is_none_or(|previous| *previous < hash),
            "candidate node manifest is not sorted unique"
        );
        node_hashes.push(hash);
    }
    let count = input.count(budget.max_transactions, 9)?;
    ensure!(
        count == plan.raw_transactions().len(),
        "candidate receipt/body count mismatch"
    );
    let mut receipts = Vec::with_capacity(count);
    for _ in 0..count {
        receipts.push(input.frame(budget.max_receipt_bytes)?.to_vec());
    }
    input.finish()?;
    validate_receipts(&plan, &receipts, budget)?;
    ensure!(
        receipt_hash(plan.context().receipt_codec, &receipts) == receipt_commitment,
        "candidate ordered receipt commitment mismatch"
    );
    ensure!(
        statement_hash(id, state_root, receipt_commitment) == statement_commitment,
        "candidate execution statement mismatch"
    );
    Ok(Decoded {
        plan,
        state_root,
        receipt_commitment,
        statement_commitment,
        node_hashes,
        receipts,
    })
}

// Borrow the variable-length account/string fields before allocating them.
// These private mirrors describe the explicit nov-receipt v1 codec only; they
// cannot construct public executed/authenticated capabilities.
#[derive(Serialize, Deserialize)]
struct BalanceRef<'a> {
    #[serde(borrow)]
    account: &'a [u8],
    before: u128,
    after: u128,
}
#[derive(Serialize, Deserialize)]
struct DeltaRef<'a> {
    tx_hash: NodeHash,
    #[serde(borrow)]
    payer: BalanceRef<'a>,
    #[serde(borrow)]
    recipient: BalanceRef<'a>,
    #[serde(borrow)]
    nonce_identity: &'a str,
    nonce_before: u64,
    nonce_after: u64,
    fee_funding_delta: u128,
}
#[derive(Serialize, Deserialize)]
enum ErrorRef {
    MissingNonceIdentity,
    NonceMismatch { expected: u64, provided: u64 },
    NonceExhausted,
    FeeCapExceeded { approved_fee: u128, fee_cap: u128 },
    DebitOverflow,
    InsufficientFunds { available: u128, required: u128 },
    RecipientOverflow,
    InconsistentSelfBalance,
    FeeFundingOverflow,
}
#[derive(Serialize, Deserialize)]
enum FailureRef<'a> {
    Fee(#[serde(borrow)] &'a str),
    Business(ErrorRef),
}
#[derive(Serialize, Deserialize)]
struct JournalRef<'a> {
    seq: u64,
    unix_ms: u128,
    tx_hash: NodeHash,
    #[serde(borrow)]
    payer: &'a [u8],
    source_amount: u128,
    settled_nov: u128,
    reserve_bucket_delta_nov: u128,
    fee_bucket_delta_nov: u128,
    risk_buffer_delta_nov: u128,
    policy_version: u32,
    #[serde(borrow)]
    policy_source: &'a str,
    #[serde(borrow)]
    policy_contract_id: &'a str,
    policy_threshold_state: ThresholdState,
    #[serde(borrow)]
    policy_constrained_strategy: &'a str,
}
#[derive(Serialize, Deserialize)]
struct ReceiptRef<'a> {
    tx_hash: NodeHash,
    signer_identity: NodeHash,
    #[serde(borrow)]
    delta: DeltaRef<'a>,
    #[serde(borrow)]
    failure: Option<FailureRef<'a>>,
    quote: Option<FeeQuote>,
    fee_failure: Option<FeeFailure>,
    #[serde(borrow)]
    journal: Option<JournalRef<'a>>,
    clear_clearing_candidates: bool,
}

fn identity_hex(identity: &NodeHash) -> String {
    use std::fmt::Write;
    let mut value = String::with_capacity(64);
    for byte in identity {
        write!(&mut value, "{byte:02x}").expect("String write cannot fail");
    }
    value
}

fn validate_receipts(plan: &BatchPlan, receipts: &[Vec<u8>], budget: PacketBudget) -> Result<()> {
    let mut hashes = BTreeSet::new();
    let mut nonces = BTreeMap::new();
    let mut journal_seq: Option<u64> = None;
    for (raw, bytes) in plan.raw_transactions().iter().zip(receipts) {
        let transaction = decode_transfer_v3(raw, budget.max_transaction_bytes)?;
        ensure!(
            transaction.chain_id == plan.context().chain_id,
            "candidate transaction chain mismatch"
        );
        for asset in [&transaction.asset, &transaction.fee_policy.pay_asset] {
            ensure!(
                asset.trim().is_empty() || asset.trim().eq_ignore_ascii_case("NOV"),
                "unsupported persisted candidate asset"
            );
        }
        let tx_hash = canonical_tx_hash(&transaction)?;
        ensure!(
            hashes.insert(tx_hash),
            "candidate duplicate canonical transaction"
        );
        let public_key = &transaction.signature[..32];
        ensure!(
            if transaction.from.len() == 20 {
                transaction.from == Sha256::digest(public_key)[12..]
            } else {
                transaction.from == public_key
            },
            "candidate signer/account relation mismatch"
        );
        let mut identity = Sha256::new();
        identity.update(b"novovm-native-auth-nonce-identity-v1");
        identity.update(transaction.chain_id.to_be_bytes());
        identity.update(b"novovm-native-auth/ed25519-public-key/v2\0");
        identity.update(public_key);
        let identity: NodeHash = identity.finalize().into();
        let (receipt, trailing): (ReceiptRef<'_>, _) =
            postcard::take_from_bytes(bytes).context("decode candidate receipt")?;
        ensure!(
            trailing.is_empty() && postcard::to_allocvec(&receipt)? == *bytes,
            "noncanonical candidate receipt bytes"
        );
        ensure!(
            receipt.tx_hash == tx_hash
                && receipt.delta.tx_hash == tx_hash
                && receipt.signer_identity == identity
                && receipt.delta.nonce_identity == identity_hex(&identity),
            "candidate receipt transaction/signer binding mismatch"
        );
        ensure!(
            receipt.delta.payer.account == transaction.from
                && receipt.delta.recipient.account == transaction.to,
            "candidate receipt account binding mismatch"
        );
        let after = transaction
            .nonce
            .checked_add(1)
            .context("candidate signer nonce exhausted")?;
        ensure!(
            receipt.delta.nonce_before == transaction.nonce && receipt.delta.nonce_after == after,
            "candidate receipt nonce transition mismatch"
        );
        if let Some(expected) = nonces.insert(identity, after) {
            ensure!(
                expected == transaction.nonce,
                "candidate signer nonce order mismatch"
            );
        }
        if let Some(quote) = &receipt.quote {
            quote.validate()?;
        }
        match (&receipt.failure, &receipt.fee_failure) {
            (Some(FailureRef::Fee(reason)), Some(fee)) => {
                ensure!(
                    *reason == fee.reason && reason.len() <= 512,
                    "candidate fee rejection mismatch"
                );
            }
            (Some(FailureRef::Fee(_)), None) | (_, Some(_)) => {
                anyhow::bail!("candidate fee rejection incomplete")
            }
            _ => {}
        }
        ensure!(
            receipt.clear_clearing_candidates == receipt.journal.is_some(),
            "candidate fee journal/clear mismatch"
        );
        if let Some(journal) = &receipt.journal {
            ensure!(
                receipt.fee_failure.is_none()
                    && journal.tx_hash == tx_hash
                    && journal.payer == transaction.from
                    && journal.unix_ms == u128::from(plan.context().timestamp_unix_ms)
                    && journal.seq > 0
                    && journal.policy_version > 0,
                "candidate journal identity mismatch"
            );
            ensure!(
                journal.policy_source.len() <= 128
                    && journal.policy_constrained_strategy.len() <= 128
                    && journal.policy_contract_id.len() <= 2048,
                "candidate journal metadata exceeds bound"
            );
            ensure!(
                journal.source_amount == journal.settled_nov
                    && journal.settled_nov == receipt.delta.fee_funding_delta
                    && receipt
                        .quote
                        .as_ref()
                        .is_some_and(|quote| quote.nov_amount == journal.settled_nov),
                "candidate journal fee amount mismatch"
            );
            ensure!(
                journal
                    .reserve_bucket_delta_nov
                    .checked_add(journal.fee_bucket_delta_nov)
                    .and_then(|sum| sum.checked_add(journal.risk_buffer_delta_nov))
                    == Some(journal.settled_nov),
                "candidate journal split mismatch"
            );
            if let Some(previous) = journal_seq {
                ensure!(
                    previous.checked_add(1) == Some(journal.seq),
                    "candidate fee journal sequence mismatch"
                );
            }
            journal_seq = Some(journal.seq);
        } else {
            ensure!(
                receipt.delta.fee_funding_delta == 0 && receipt.fee_failure.is_some(),
                "candidate missing charged-fee journal or rejection"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
