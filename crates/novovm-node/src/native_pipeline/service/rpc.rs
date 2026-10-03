//! Product RPC adapter for the resident core. No RPC handler mutates balances,
//! publishes a head, constructs votes or substitutes a receipt for execution.
//! This explicit experimental profile has a local bounded pool. Automatic
//! cross-node mempool gossip is not implemented here: clients can submit to the
//! scheduled proposer (or fan out identical signed bytes). Consensus bodies
//! always traverse the same authenticated duplex network as the load fixture.

use super::{ResidentConfig, ResidentNode, StartMode};
use crate::native_fresh_rpc::FreshRpcServer;
use crate::native_pipeline::business::nov_transfer_batch::balance_key;
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::consensus::transport::Message;
use crate::native_pipeline::consensus::wire::Hash;
use crate::native_pipeline::consensus::ArchiveRead;
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
use crate::native_pipeline::persistence::io::IoTicket;
use crate::native_pipeline::persistence::packet::ReceiptView;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_POOL: usize = 65_536;
const MAX_POOL_BYTES: usize = 64 * 1024 * 1024;
const RECENT_RECEIPTS: usize = 65_536;
const MAX_SIGNERS: usize = 65_536;

struct BalanceQuery {
    account: Account,
    root: Hash,
    height: u64,
    ticket: Option<IoTicket<Option<Vec<u8>>>>,
    result: Option<Value>,
}

struct Pending {
    raw: Vec<u8>,
    signer: Hash,
    nonce: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Position {
    height: u64,
    round: u64,
    parent: Hash,
    successor: bool,
}

/// Derived query/admission projection only. Rebuilt from verified archive on
/// restart; never saved as a second authoritative state/head. A full cache does
/// not drop accepted input or manufacture finality.
pub struct RpcLifecycle {
    pub node: ResidentNode,
    batch_size: usize,
    pending: BTreeMap<Hash, Pending>,
    order: VecDeque<Hash>,
    reservations: BTreeSet<(Hash, u64)>,
    pending_bytes: usize,
    nonces: BTreeMap<Hash, u64>,
    receipts: BTreeMap<Hash, Value>,
    recent: VecDeque<Hash>,
    indexed_height: u64,
    archive: Option<ArchiveRead>,
    offer: Option<(Position, Arc<Message>)>,
    offered: Option<Position>,
    projection_error: Option<String>,
    balances: VecDeque<BalanceQuery>,
}

impl RpcLifecycle {
    pub fn new(node: ResidentNode, batch_size: usize) -> Result<Self> {
        ensure!((1..=1024).contains(&batch_size), "invalid RPC batch limit");
        Ok(Self {
            node,
            batch_size,
            pending: BTreeMap::new(),
            order: VecDeque::new(),
            reservations: BTreeSet::new(),
            pending_bytes: 0,
            nonces: BTreeMap::new(),
            receipts: BTreeMap::new(),
            recent: VecDeque::new(),
            indexed_height: 0,
            archive: None,
            offer: None,
            offered: None,
            projection_error: None,
            balances: VecDeque::new(),
        })
    }

    pub fn poll(&mut self, now: Instant) -> Result<()> {
        self.node.poll(now)?;
        self.poll_balances()?;
        if self.projection_error.is_none() {
            if let Err(error) = self.poll_projection() {
                self.projection_error = Some(format!("{error:#}"));
            }
        }
        if self.projection_error.is_some() || self.node.controller.is_recovering() {
            return Ok(());
        }
        // Catch up query/nonces before proposing current-height user work. The
        // core still prepares a successor from its private durable parent while
        // that parent's consensus waits; no future signing permission is granted.
        if self.indexed_height != self.node.controller.parent().height {
            return Ok(());
        }
        let successor = self.node.controller.successor_parent()?;
        let (parent, position) = if self.node.controller.is_local_leader()? {
            let parent = self.node.controller.parent();
            (
                parent,
                Position {
                    height: parent.height + 1,
                    round: self.node.controller.round(),
                    parent: parent.block_hash,
                    successor: false,
                },
            )
        } else if let Some(parent) = successor {
            (
                parent,
                Position {
                    height: parent.height + 1,
                    round: 0,
                    parent: parent.block_hash,
                    successor: true,
                },
            )
        } else {
            return Ok(());
        };
        if self.offered == Some(position) {
            return Ok(());
        }
        if self.offer.as_ref().is_none_or(|(old, _)| *old != position) {
            let mut next = BTreeMap::new();
            if position.successor {
                let Some(body) = self.node.controller.executed_body(parent.block_hash) else {
                    return Ok(());
                };
                if let Message::Body {
                    raw_transactions, ..
                } = body.as_ref()
                {
                    for raw in raw_transactions {
                        let checked =
                            authenticate_transfer_v3(raw, self.node.template.chain_id, 1024)?;
                        next.insert(checked.nonce_identity(), checked.transfer().nonce + 1);
                    }
                } else {
                    return Ok(());
                }
            }
            let raw_transactions = select_pending_batch(
                &self.order,
                &self.pending,
                &self.nonces,
                next,
                self.batch_size,
            );
            if raw_transactions.is_empty() {
                return Ok(());
            }
            let mut context = self.node.template;
            context.height = position.height;
            context.slot = position.height;
            context.timestamp_unix_ms = context
                .timestamp_unix_ms
                .checked_add(position.height)
                .context("resident proposed timestamp overflow")?;
            context.parent_height = parent.height;
            context.parent_block_hash = parent.block_hash;
            context.parent_state_root = parent.state_root;
            context.parent_receipt_root = parent.receipt_batch_commitment;
            context.parent_state_version = parent.state_version;
            self.offer = Some((
                position,
                Arc::new(Message::Body {
                    context,
                    raw_transactions,
                }),
            ));
        }
        let body = &self.offer.as_ref().expect("prepared RPC body").1;
        let accepted = if position.successor {
            self.node.controller.try_submit_successor_body(body)?
        } else {
            self.node.controller.try_submit_body(body)?
        };
        if accepted {
            self.offered = Some(position);
        }
        // Retain original signed inputs until an actual archive receipt confirms
        // them, including re-proposals after changed rounds/parents.
        Ok(())
    }

    fn poll_projection(&mut self) -> Result<()> {
        let Some(head) = self.node.controller.head() else {
            return Ok(());
        };
        if self.indexed_height >= head.height {
            return Ok(());
        }
        if self.archive.is_none() {
            self.archive = Some(ArchiveRead::new(
                self.indexed_height + 1,
                head,
                self.node.controller.context(),
                self.node.validators.clone(),
            )?);
        }
        let Some(block) = self.archive.as_mut().unwrap().poll(&self.node.pipeline)? else {
            return Ok(());
        };
        let block = block.context("decided block missing during RPC projection")?;
        for index in 0..block.stored().receipt_bytes().len() {
            let receipt = block.stored().receipt_view(index)?;
            ensure!(
                self.nonces.contains_key(&receipt.signer_identity)
                    || self.nonces.len() < MAX_SIGNERS,
                "experimental RPC signer projection capacity reached; consensus remains active"
            );
            self.nonces
                .insert(receipt.signer_identity, receipt.nonce_after);
            if let Some(entry) = self.pending.remove(&receipt.tx_hash) {
                self.pending_bytes -= entry.raw.len();
                self.reservations.remove(&(entry.signer, entry.nonce));
            }
            let hash = receipt.tx_hash;
            self.receipts.insert(
                hash,
                receipt_json(receipt, block.point().height, block.point().block_hash),
            );
            self.recent.push_back(hash);
            while self.recent.len() > RECENT_RECEIPTS {
                self.receipts
                    .remove(&self.recent.pop_front().expect("old receipt"));
            }
        }
        // Conflicting same-nonce input is not reported as an executed failure.
        let stale: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(hash, entry)| {
                (entry.nonce < self.nonces.get(&entry.signer).copied().unwrap_or(0))
                    .then_some(*hash)
            })
            .collect();
        for hash in stale {
            let entry = self.pending.remove(&hash).expect("stale entry");
            self.pending_bytes -= entry.raw.len();
            self.reservations.remove(&(entry.signer, entry.nonce));
            self.receipts.insert(
                hash,
                json!({"tx_hash":hex(&hash),"state":"rejected_nonce",
                "finalized":false,"executed":false}),
            );
            self.recent.push_back(hash);
        }
        while self.recent.len() > RECENT_RECEIPTS {
            self.receipts.remove(&self.recent.pop_front().unwrap());
        }
        self.order.retain(|hash| self.pending.contains_key(hash));
        self.indexed_height = block.point().height;
        self.archive = None;
        Ok(())
    }

    fn submit(&mut self, raw: Vec<u8>) -> Result<Value> {
        ensure!(
            self.projection_error.is_none(),
            "RPC projection unavailable"
        );
        ensure!(
            self.indexed_height == self.node.controller.head().map_or(0, |h| h.height),
            "RPC recovery projection catching up; retry signed input"
        );
        let checked = authenticate_transfer_v3(&raw, self.node.template.chain_id, 1024)?;
        for asset in [
            &checked.transfer().asset,
            &checked.transfer().fee_policy.pay_asset,
        ] {
            ensure!(
                asset.trim().is_empty() || asset.trim().eq_ignore_ascii_case("NOV"),
                "resident transfer profile supports direct NOV only"
            );
        }
        let hash = checked.tx_hash();
        if let Some(result) = self.receipts.get(&hash) {
            return Ok(result.clone());
        }
        if self.pending.contains_key(&hash) {
            return Ok(self.pending_status(hash));
        }
        let signer = checked.nonce_identity();
        let nonce = checked.transfer().nonce;
        ensure!(
            nonce >= self.nonces.get(&signer).copied().unwrap_or(0),
            "nonce replay"
        );
        ensure!(
            !self.reservations.contains(&(signer, nonce)),
            "signer nonce already reserved"
        );
        ensure!(
            self.pending.len() < MAX_POOL
                && raw.len() <= MAX_POOL_BYTES.saturating_sub(self.pending_bytes),
            "RPC pool backpressure; transaction not accepted"
        );
        self.pending_bytes += raw.len();
        self.pending.insert(hash, Pending { raw, signer, nonce });
        self.reservations.insert((signer, nonce));
        self.order.push_back(hash);
        Ok(self.pending_status(hash))
    }

    fn pending_status(&self, hash: Hash) -> Value {
        json!({"tx_hash":hex(&hash),"state":"received","signature_verified":true,
            "admission_durable":false,"nonce_checked_at_execution":true,
            "executed":false,"finalized":false,"proof_verified":false})
    }

    fn poll_balances(&mut self) -> Result<()> {
        for query in &mut self.balances {
            if query.result.is_some() {
                continue;
            }
            if query.ticket.is_none() {
                query.ticket = self
                    .node
                    .pipeline
                    .try_read_value(query.root, balance_key(&query.account))?;
            }
            let Some(ticket) = &mut query.ticket else {
                continue;
            };
            let Some(value) = ticket.try_take()? else {
                continue;
            };
            let amount = match value {
                None => 0,
                Some(bytes) => u128::from_le_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("invalid authoritative NOV balance width"))?,
                ),
            };
            query.ticket = None;
            query.result = Some(
                json!({"account":hex(query.account.as_bytes()),"asset":"NOV",
                "balance":amount.to_string(),"state_root":hex(&query.root),
                "block_height":query.height,"finalized":query.height>0,"query_complete":true}),
            );
        }
        Ok(())
    }

    fn balance(&mut self, params: &Value) -> Result<Value> {
        let params = params
            .as_object()
            .context("balance params object required")?;
        for (key, value) in params {
            match key.as_str() {
                "account" => (),
                "asset" | "asset_id" => ensure!(
                    value
                        .as_str()
                        .is_some_and(|a| a.eq_ignore_ascii_case("NOV")),
                    "resident balance supports public NOV only"
                ),
                _ => anyhow::bail!("unsupported balance parameter"),
            }
        }
        let text = params
            .get("account")
            .and_then(Value::as_str)
            .context("account required")?;
        let bytes = super::config::decode_hex(text)?;
        let account = Account::try_from(bytes).map_err(anyhow::Error::msg)?;
        if let Some(position) = self.balances.iter().position(|q| q.account == account) {
            if self.balances[position].result.is_some() {
                return Ok(self.balances.remove(position).unwrap().result.unwrap());
            }
            return Ok(json!({"query_complete":false,"state":"query_pending"}));
        }
        reserve_balance_slot(&mut self.balances)?;
        let (root, height) = self
            .node
            .controller
            .head()
            .map(|h| (h.state_root, h.height))
            .unwrap_or((self.node.template.parent_state_root, 0));
        self.balances.push_back(BalanceQuery {
            account,
            root,
            height,
            ticket: None,
            result: None,
        });
        Ok(json!({"query_complete":false,"state":"query_pending"}))
    }

    pub fn status(&self) -> Value {
        let controller = &self.node.controller;
        let stats = controller.stats();
        let leader = self
            .node
            .validators
            .leader(controller.context().height, controller.round())
            .ok();
        json!({"profile":"native-resident-v1","experimental":true,
            "core":"novovm-node/native_pipeline -> novovm-exec/resident -> AOEM",
            "consensus":"novovm-consensus/round_bft/journal",
            "chain_id":self.node.template.chain_id,"head":controller.head(),
            "current_height":controller.context().height,"round":controller.round(),
            "scheduled_proposer":leader.map(|h|hex(&h)),"local_proposer":controller.is_local_leader().ok(),
            "pending":self.pending.len(),"pending_bytes":self.pending_bytes,
            "rpc_indexed_height":self.indexed_height,"projection_error":self.projection_error,
            "recovery_in_progress":controller.is_recovering(),
            "executed_batches":stats.executed_batches,"durable_decisions":stats.durable_decisions,
            "successor_reused":stats.successor_reused,"capture_seed_nodes":stats.capture_seed_nodes,
            "execution_failures":stats.execution_failures,"last_error":stats.last_error,
            "execution_backend":"aoem_semantic_v2_cpu_callbacks",
            "business_gpu_active":false,"business_proof_required":false,"business_proof_verified":false,
            "mempool_gossip":false,"pending_survives_restart":false,
            "receipt_query_scope":"recent 65536 entries, rebuilt from durable archive"})
    }

    pub fn handle(&mut self, request: Value) -> Value {
        if let Value::Array(requests) = request {
            if requests.is_empty()
                || requests.len() > 1024
                || requests.iter().any(|value| !value.is_object())
            {
                return error(Value::Null, "JSON-RPC batch count exceeds bound");
            }
            return Value::Array(
                requests
                    .into_iter()
                    .map(|request| self.handle(request))
                    .collect(),
            );
        }
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let result = (|| -> Result<Value> {
            ensure!(
                request["jsonrpc"] == "2.0" && (id.is_null() || id.is_string() || id.is_number()),
                "invalid JSON-RPC request"
            );
            match request["method"].as_str() {
                Some("nov_chainStatus") => Ok(self.status()),
                Some("nov_sendRawTransaction") => self.submit(one_hex_param(&request, 1024)?),
                Some("nov_getAssetBalance") => self.balance(&request["params"]),
                Some("nov_getTransactionStatus") => {
                    let hash: Hash = one_hex_param(&request, 32)?
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("32-byte hash required"))?;
                    if let Some(result) = self.receipts.get(&hash) {
                        return Ok(result.clone());
                    }
                    if self.pending.contains_key(&hash) {
                        return Ok(self.pending_status(hash));
                    }
                    Ok(json!({"tx_hash":hex(&hash),"state":"not_in_recent_index",
                        "finalized":false,"lookup_complete":false,"indexed_height":self.indexed_height}))
                }
                _ => anyhow::bail!("method not connected to resident profile; no legacy fallback"),
            }
        })();
        match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(err) => error(id, &err.to_string()),
        }
    }
}

fn select_pending_batch(
    order: &VecDeque<Hash>,
    pending: &BTreeMap<Hash, Pending>,
    nonces: &BTreeMap<Hash, u64>,
    mut next: BTreeMap<Hash, u64>,
    batch_size: usize,
) -> Vec<Vec<u8>> {
    let mut raw_transactions = Vec::new();
    let mut raw_bytes = 0usize;
    let body_limit = super::body_byte_limit(batch_size);
    for hash in order {
        let Some(entry) = pending.get(hash) else {
            continue;
        };
        let expected = next
            .entry(entry.signer)
            .or_insert_with(|| nonces.get(&entry.signer).copied().unwrap_or(0));
        if entry.nonce == *expected {
            if raw_bytes + entry.raw.len() > body_limit {
                break;
            }
            raw_bytes += entry.raw.len();
            raw_transactions.push(entry.raw.clone());
            *expected += 1;
            if raw_transactions.len() == batch_size {
                break;
            }
        }
    }
    raw_transactions
}

fn reserve_balance_slot(balances: &mut VecDeque<BalanceQuery>) -> Result<()> {
    if balances.len() == 8 {
        if let Some(old) = balances.iter().position(|q| q.result.is_some()) {
            balances.remove(old);
        }
    }
    ensure!(balances.len() < 8, "balance query backpressure; retry");
    Ok(())
}

fn receipt_json(receipt: ReceiptView, height: u64, block: Hash) -> Value {
    json!({"tx_hash":hex(&receipt.tx_hash),"state":if receipt.success {"finalized_success"} else {"finalized_business_failure"},
        "executed":true,"success":receipt.success,"nonce_after":receipt.nonce_after,
        "charged_fee":receipt.charged_fee,"block_height":height,"block_hash":hex(&block),
        "finalized":true,"finality_kind":"BFT_durable","proof_verified":false})
}

fn one_hex_param(request: &Value, max: usize) -> Result<Vec<u8>> {
    let params = request["params"]
        .as_array()
        .context("params array required")?;
    ensure!(params.len() == 1, "one parameter required");
    let text = params[0].as_str().context("hex string required")?;
    ensure!(text.len() <= max * 2 + 2, "hex parameter exceeds bound");
    let text = text.strip_prefix("0x").unwrap_or(text);
    ensure!(
        !text.is_empty() && text.is_ascii() && text.len().is_multiple_of(2),
        "invalid hex parameter"
    );
    let bytes = text
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= max,
        "empty or oversized parameter"
    );
    Ok(bytes)
}

fn error(id: Value, text: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":text}})
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::from("0x");
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("String write");
    }
    text
}

pub fn run(path: &Path, mode: StartMode, run_for: Option<Duration>) -> Result<()> {
    let config = ResidentConfig::load(path)?;
    // Reserve the actual product endpoint before opening any signer/storage.
    let mut rpc = FreshRpcServer::bind_with_max_request(config.rpc_addr, 4 * 1024 * 1024)?;
    let batch_size = config.batch_size;
    let mut lifecycle = RpcLifecycle::new(ResidentNode::start(config, mode)?, batch_size)?;
    let started = Instant::now();
    println!("resident_rpc_listening={}", rpc.local_addr()?);
    let result = (|| -> Result<()> {
        loop {
            let now = Instant::now();
            lifecycle.poll(now)?;
            rpc.poll_with(|request| lifecycle.handle(request))?;
            if run_for.is_some_and(|limit| started.elapsed() >= limit) {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    })();
    let drain = lifecycle.node.shutdown();
    result.and(drain)
}

/// Explicit mode selection in the original novovm-node executable. No flag
/// can silently select a legacy executor or upgrade an existing database.
pub fn run_from_env() -> Result<()> {
    let path = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_NATIVE_RESIDENT_CONFIG")
            .context("NOVOVM_NATIVE_RESIDENT_CONFIG required")?,
    );
    ensure!(
        std::env::var("NOVOVM_ALLOW_LEGACY_HOST_EXECUTION")
            .ok()
            .as_deref()
            != Some("1"),
        "resident profile must not be combined with legacy execution permission"
    );
    let mode = match std::env::var("NOVOVM_NATIVE_RESIDENT_START")
        .ok()
        .as_deref()
    {
        Some("create") => StartMode::CreateNew,
        Some("existing") => StartMode::Existing,
        _ => anyhow::bail!("explicit NOVOVM_NATIVE_RESIDENT_START=create|existing required"),
    };
    let run_for = std::env::var("NOVOVM_NATIVE_RESIDENT_RUN_MS")
        .ok()
        .map(|text| text.parse::<u64>().map(Duration::from_millis))
        .transpose()?;
    std::thread::Builder::new()
        .name("native-resident-node".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || run(&path, mode, run_for))?
        .join()
        .map_err(|_| anyhow::anyhow!("resident node thread panicked"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_fixture(
        count: usize,
        raw_bytes: usize,
    ) -> (VecDeque<Hash>, BTreeMap<Hash, Pending>) {
        let mut order = VecDeque::new();
        let mut pending = BTreeMap::new();
        for nonce in 0..count {
            let mut hash = [0x55; 32];
            hash[..8].copy_from_slice(&(nonce as u64).to_le_bytes());
            let mut raw = vec![0; raw_bytes];
            raw[..8].copy_from_slice(&(nonce as u64).to_le_bytes());
            order.push_back(hash);
            pending.insert(
                hash,
                Pending {
                    raw,
                    signer: [1; 32],
                    nonce: nonce as u64,
                },
            );
        }
        (order, pending)
    }

    #[test]
    fn batch_selection_obeys_network_byte_limit_without_dropping_pending_or_nonce_credit() {
        let (order, pending) = pending_fixture(1024, 1024);
        let selected =
            select_pending_batch(&order, &pending, &BTreeMap::new(), BTreeMap::new(), 1024);
        assert_eq!(selected.len(), 384);
        assert_eq!(
            selected.iter().map(Vec::len).sum::<usize>(),
            super::super::body_byte_limit(1024)
        );
        assert_eq!(order.len(), 1024);
        assert_eq!(
            pending.len(),
            1024,
            "proposing must not discard unfinalized inputs"
        );
        let advanced = BTreeMap::from([([1; 32], 384)]);
        let next = select_pending_batch(&order, &pending, &advanced, BTreeMap::new(), 1024);
        assert_eq!(next.len(), 384);
        assert_eq!(u64::from_le_bytes(next[0][..8].try_into().unwrap()), 384);
        assert_eq!(
            u64::from_le_bytes(next.last().unwrap()[..8].try_into().unwrap()),
            767
        );
    }

    #[test]
    fn batch_selection_preserves_nonce_gaps_successor_prefix_and_count_cap() {
        let (mut order, pending) = pending_fixture(5, 8);
        let first = order.pop_front().unwrap();
        order.insert(2, first); // Future 1/2 appear before the required 0.
        let selected = select_pending_batch(&order, &pending, &BTreeMap::new(), BTreeMap::new(), 5);
        assert_eq!(selected, vec![0_u64.to_le_bytes().to_vec()]);
        let (order, pending) = pending_fixture(5, 8);
        let successor = BTreeMap::from([([1; 32], 2)]);
        let selected = select_pending_batch(&order, &pending, &BTreeMap::new(), successor, 2);
        assert_eq!(
            selected,
            vec![2_u64.to_le_bytes().to_vec(), 3_u64.to_le_bytes().to_vec()]
        );
        assert_eq!(pending.len(), 5);
    }

    fn query(index: u8, complete: bool) -> BalanceQuery {
        BalanceQuery {
            account: Account::try_from(vec![index; 20]).unwrap(),
            root: [index; 32],
            height: 1,
            ticket: None,
            result: complete.then(|| json!({"balance":"7","query_complete":true})),
        }
    }

    #[test]
    fn abandoned_completed_balance_results_do_not_exhaust_query_slots() {
        let mut balances: VecDeque<_> = (0..8).map(|i| query(i, true)).collect();
        for next in 8..24 {
            reserve_balance_slot(&mut balances).unwrap();
            assert_eq!(balances.len(), 7);
            balances.push_back(query(next, true));
            assert_eq!(balances.len(), 8);
        }
        assert_eq!(balances.front().unwrap().root, [16; 32]);
    }

    #[test]
    fn balance_backpressure_preserves_all_unfinished_slots_and_only_evicts_completed() {
        let mut balances: VecDeque<_> = (0..8).map(|i| query(i, false)).collect();
        assert!(reserve_balance_slot(&mut balances).is_err());
        assert_eq!(balances.len(), 8);
        assert_eq!(
            balances.iter().map(|q| q.root[0]).collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>()
        );
        balances[4].result = Some(json!({"query_complete":true}));
        reserve_balance_slot(&mut balances).unwrap();
        assert_eq!(
            balances.iter().map(|q| q.root[0]).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 5, 6, 7]
        );
        assert!(balances.iter().all(|q| q.result.is_none()));
    }

    #[test]
    fn full_signed_transfer_hex_reaches_the_real_authenticator() {
        let raw = super::super::tests::signed_raw(19, 0);
        assert!(raw.len() > 32);
        let decoded = one_hex_param(&json!({"params":[hex(&raw)]}), 1024).unwrap();
        assert_eq!(decoded, raw);
        authenticate_transfer_v3(&decoded, 71, 1024).unwrap();
        assert!(
            super::super::config::decode_hex(&hex(&raw)).is_err(),
            "account decoder stays bounded"
        );
        assert!(one_hex_param(&json!({"params":[hex(&raw)]}), raw.len() - 1).is_err());
        assert!(authenticate_transfer_v3(&decoded, 72, 1024).is_err());
        let mut tampered = decoded;
        *tampered.last_mut().unwrap() ^= 1;
        assert!(authenticate_transfer_v3(&tampered, 71, 1024).is_err());
    }

    #[test]
    fn raw_and_hash_requests_are_bounded_and_canonical() {
        assert!(one_hex_param(&json!({"params":["0x00"]}), 1).is_ok());
        assert_eq!(
            one_hex_param(&json!({"params":["ab".repeat(300)]}), 1024)
                .unwrap()
                .len(),
            300
        );
        for params in [
            json!([]),
            json!(["0x"]),
            json!(["0x0000"]),
            json!(["z0"]),
            json!(["00", "00"]),
        ] {
            assert!(one_hex_param(&json!({"params":params}), 1).is_err());
        }
    }
    #[test]
    fn business_failure_is_not_counted_as_success_or_zk() {
        let view = ReceiptView {
            tx_hash: [1; 32],
            signer_identity: [2; 32],
            nonce_after: 1,
            success: false,
            charged_fee: "3".into(),
        };
        let value = receipt_json(view, 1, [4; 32]);
        assert_eq!(value["success"], false);
        assert_eq!(value["finalized"], true);
        assert_eq!(value["proof_verified"], false);
        assert_eq!(value["state"], "finalized_business_failure");
    }
}
