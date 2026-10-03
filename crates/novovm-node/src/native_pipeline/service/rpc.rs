//! Product RPC adapter for the resident core. No RPC handler mutates balances,
//! publishes a head, constructs votes or substitutes a receipt for execution.
//! This experimental profile has a bounded, NON-durable pool and best-effort
//! signed-input fanout over the existing authenticated duplex channel. Receipt
//! queries, not transport admission, determine execution/finality. Clients must
//! retain signed input until confirmed and may resubmit after loss/restart.

use super::{ResidentConfig, ResidentNode, StartMode};
use crate::native_fresh_rpc::FreshRpcServer;
use crate::native_pipeline::business::nov_transfer_batch::balance_key;
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::consensus::transport::{EarlyBodyScope, Message};
use crate::native_pipeline::consensus::wire::Hash;
use crate::native_pipeline::consensus::ArchiveRead;
use crate::native_pipeline::ingress::apfl::{ApflLimits, ApflTransferBatch};
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
use crate::native_pipeline::ingress::wire::{canonical_tx_hash, decode_transfer_v3};
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

struct EarlyOffer {
    scope: EarlyBodyScope,
    hint: Arc<Message>,
    message: Arc<Message>,
}

struct GossipOffer {
    hashes: Vec<Hash>,
    message: Arc<Message>,
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
    early_offer: Option<EarlyOffer>,
    gossip_order: VecDeque<Hash>,
    gossip_queued: BTreeSet<Hash>,
    gossip_offer: Option<GossipOffer>,
    incoming: Option<(Arc<Message>, usize)>,
    ingress_batch_boundary: bool,
    gossip_verified: u64,
    gossip_rejected: u64,
    authentication_cache_hits: u64,
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
            early_offer: None,
            gossip_order: VecDeque::new(),
            gossip_queued: BTreeSet::new(),
            gossip_offer: None,
            incoming: None,
            ingress_batch_boundary: false,
            gossip_verified: 0,
            gossip_rejected: 0,
            authentication_cache_hits: 0,
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
        self.poll_gossip()?;
        if self.projection_error.is_some() || self.node.controller.is_recovering() {
            return Ok(());
        }
        // Catch up query/nonces before proposing current-height user work. The
        // core still prepares a successor from its private durable parent while
        // that parent's consensus waits; no future signing permission is granted.
        if self.indexed_height != self.node.controller.parent().height {
            return Ok(());
        }
        // Give a finite incoming body time to finish its strict admission
        // rather than turning every 32-signature service quantum into a block.
        // Every completed body grants a proposal turn even if the next body
        // has begun, so a continuous inbound stream cannot starve selection.
        if self.incoming.is_some() && !self.ingress_batch_boundary {
            return Ok(());
        }
        // Announce h+1 input while h is still executing/awaiting consensus.
        // Its nonce prefix is a scheduling hint only; the existing core binds
        // the exact durable parent and rechecks the real state before voting.
        self.poll_early_offer()?;
        let successor = self.node.controller.successor_parent()?;
        let current_height = self.node.controller.context().height;
        let (parent, position) = if self.node.controller.is_local_leader()?
            && !self.node.controller.has_early_target(current_height)
        {
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
                let Some(prefix) = pending_body_prefix(&body, &self.pending, &self.nonces)? else {
                    return Ok(());
                };
                next = prefix;
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
            let batch = apfl_batch(&raw_transactions, self.batch_size)?;
            self.offer = Some((position, Arc::new(Message::ApflBody { context, batch })));
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

    fn poll_early_offer(&mut self) -> Result<()> {
        let Some(scope) = self.node.controller.early_body_scope()? else {
            self.early_offer = None;
            return Ok(());
        };
        let Some(body) = self.node.controller.current_body_hint()? else {
            self.early_offer = None;
            return Ok(());
        };
        if self
            .early_offer
            .as_ref()
            .is_none_or(|old| old.scope != scope || !Arc::ptr_eq(&old.hint, &body))
        {
            let Some(next) = pending_body_prefix(&body, &self.pending, &self.nonces)? else {
                return Ok(());
            };
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
            let batch = apfl_batch(&raw_transactions, self.batch_size)?;
            self.early_offer = Some(EarlyOffer {
                scope,
                hint: body,
                message: Arc::new(Message::ApflEarlyBody { scope, batch }),
            });
        }
        let timestamp = self
            .node
            .template
            .timestamp_unix_ms
            .checked_add(scope.target_height)
            .context("resident early timestamp overflow")?;
        if self.node.controller.try_submit_early_body(
            &self.early_offer.as_ref().expect("early RPC body").message,
            scope.target_height,
            timestamp,
        )? {
            self.early_offer = None;
        }
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
        self.gossip_order
            .retain(|hash| self.pending.contains_key(hash));
        self.gossip_queued
            .retain(|hash| self.pending.contains_key(hash));
        self.indexed_height = block.point().height;
        self.archive = None;
        Ok(())
    }

    fn submit(&mut self, raw: Vec<u8>) -> Result<Value> {
        self.admit(raw, true)
    }

    fn admit(&mut self, raw: Vec<u8>, broadcast: bool) -> Result<Value> {
        ensure!(
            self.projection_error.is_none(),
            "RPC projection unavailable"
        );
        // Signature-checked memory admission is not current-state admission.
        // A lagging query projection must not stall ingress. Selection still
        // waits for the exact published parent above; projection removes stale
        // nonces before any new current-height body can be submitted.
        // A peer fanout and a client retry may carry the very same input. Reuse
        // ONLY an exact byte match still held in the signature-checked pool;
        // the canonical ID alone omits the signature and is insufficient.
        let decoded = decode_transfer_v3(&raw, 1024)?;
        let hash = canonical_tx_hash(&decoded)?;
        if self
            .pending
            .get(&hash)
            .is_some_and(|entry| entry.raw == raw)
        {
            self.authentication_cache_hits = self.authentication_cache_hits.saturating_add(1);
            if broadcast {
                self.queue_gossip(hash);
            }
            return Ok(self.pending_status(hash));
        }
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
            if broadcast {
                self.queue_gossip(hash);
            }
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
        if broadcast {
            self.queue_gossip(hash);
        }
        Ok(self.pending_status(hash))
    }

    fn queue_gossip(&mut self, hash: Hash) {
        if self.gossip_queued.insert(hash) {
            self.gossip_order.push_back(hash);
        }
    }

    fn poll_gossip(&mut self) -> Result<()> {
        self.ingress_batch_boundary = false;
        if self.projection_error.is_some() {
            return Ok(());
        }
        // Network identity authenticates the source, not its transactions. Work
        // is bounded independently of control events and never re-broadcasts
        // received input. A partially consumed batch retains one bounded Arc.
        for _ in 0..32 {
            if self.incoming.is_none() {
                self.incoming = self
                    .node
                    .controller
                    .take_transactions()
                    .map(|received| (received.message, 0));
            }
            let Some((message, index)) = self.incoming.as_mut() else {
                break;
            };
            let (raw, count) = match message.as_ref() {
                Message::Transactions {
                    raw_transactions, ..
                } => (raw_transactions[*index].clone(), raw_transactions.len()),
                Message::ApflTransactions { batch, .. } => {
                    (batch.canonical_raw(*index)?, batch.len())
                }
                _ => anyhow::bail!("transaction channel returned a non-transaction message"),
            };
            *index += 1;
            if *index == count {
                self.incoming = None;
                self.ingress_batch_boundary = true;
            }
            match self.admit(raw, false) {
                Ok(_) => self.gossip_verified = self.gossip_verified.saturating_add(1),
                Err(_) => self.gossip_rejected = self.gossip_rejected.saturating_add(1),
            }
        }
        if self.gossip_offer.is_none() {
            let mut hashes = Vec::new();
            let mut raw_transactions = Vec::new();
            let mut bytes = 0;
            // All stale hashes are removed as the finalized projection drains;
            // only a finite body is copied on each scheduling turn.
            while let Some(hash) = self.gossip_order.front().copied() {
                if let Some(entry) = self.pending.get(&hash) {
                    if hashes.len() == self.batch_size
                        || bytes + entry.raw.len() > super::body_byte_limit(self.batch_size)
                    {
                        break;
                    }
                    bytes += entry.raw.len();
                    hashes.push(hash);
                    raw_transactions.push(entry.raw.clone());
                } else {
                    self.gossip_queued.remove(&hash);
                }
                self.gossip_order.pop_front();
            }
            if !raw_transactions.is_empty() {
                let batch = apfl_batch(&raw_transactions, self.batch_size)?;
                self.gossip_offer = Some(GossipOffer {
                    hashes,
                    message: self.node.controller.apfl_transactions_message(batch)?,
                });
            }
        }
        if let Some(offer) = &self.gossip_offer {
            if self
                .node
                .controller
                .try_submit_transactions(&offer.message)?
            {
                for hash in &offer.hashes {
                    self.gossip_queued.remove(hash);
                }
                self.gossip_offer = None;
            }
        }
        Ok(())
    }

    fn pending_status(&self, hash: Hash) -> Value {
        json!({"tx_hash":hex(&hash),"state":"received","signature_verified":true,
            "admission_durable":false,"nonce_checked_at_execution":true,
            "nonce_projection_height":self.indexed_height,
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
        let apfl_transport = match controller.channel_status() {
            Ok(channel) => json!({"prepared":channel.apfl_prepared,
                "received":channel.apfl_received,
                "scope":"application codec work, not unique transactions or TLS delivery"}),
            Err(error) => json!({"error":error.to_string()}),
        };
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
            "execution_components_total":stats.execution_components_total,
            "execution_credit_only_accounts_total":stats.execution_credit_only_accounts_total,
            "execution_peak_callbacks":stats.execution_peak_callbacks,
            "execution_recomputed_transactions_total":stats.execution_recomputed_transactions_total,
            "apfl_view_transactions_total":stats.apfl_view_transactions_total,
            "apfl_transport":apfl_transport,
            "successor_reused":stats.successor_reused,"capture_seed_nodes":stats.capture_seed_nodes,
            "early_authentication_started":stats.early_authentication_started,
            "early_authentication_completed":stats.early_authentication_completed,
            "early_authentication_completed_before_parent":stats.early_authentication_completed_before_parent,
            "early_bind_reused":stats.early_bind_reused,"early_discarded":stats.early_discarded,
            "execution_failures":stats.execution_failures,"last_error":stats.last_error,
            "execution_backend":"aoem_semantic_v2_cpu_callbacks",
            "business_gpu_active":false,"business_proof_required":false,"business_proof_verified":false,
            "mempool_gossip":true,"gossip_delivery":"best_effort_not_durable",
            "transaction_gossip":self.node.controller.transactions_stats(),
            "gossip_verified_inputs":self.gossip_verified,"gossip_rejected_inputs":self.gossip_rejected,
            "authentication_cache_hits":self.authentication_cache_hits,
            "pending_survives_restart":false,
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

/// Exclude a proposed prefix using only exact already-verified pool bytes.
/// The canonical transaction hash deliberately omits the signature; hash
/// equality alone is NOT authentication. Unknown/altered/gapped input makes
/// lookahead unavailable, never changes finalized nonce or removes pending.
fn pending_body_prefix(
    body: &Message,
    pending: &BTreeMap<Hash, Pending>,
    nonces: &BTreeMap<Hash, u64>,
) -> Result<Option<BTreeMap<Hash, u64>>> {
    match body {
        Message::Body {
            raw_transactions, ..
        } => pending_raw_prefix(raw_transactions, pending, nonces),
        Message::ApflBody { batch, .. } => pending_apfl_prefix(batch, pending, nonces),
        _ => Ok(None),
    }
}

fn pending_apfl_prefix(
    batch: &ApflTransferBatch,
    pending: &BTreeMap<Hash, Pending>,
    nonces: &BTreeMap<Hash, u64>,
) -> Result<Option<BTreeMap<Hash, u64>>> {
    let mut next = BTreeMap::new();
    for index in 0..batch.len() {
        let raw = batch.canonical_raw(index)?;
        if !advance_pending_raw(&raw, &mut next, pending, nonces)? {
            return Ok(None);
        }
    }
    Ok(Some(next))
}

fn pending_raw_prefix(
    raw_transactions: &[Vec<u8>],
    pending: &BTreeMap<Hash, Pending>,
    nonces: &BTreeMap<Hash, u64>,
) -> Result<Option<BTreeMap<Hash, u64>>> {
    let mut next = BTreeMap::new();
    for raw in raw_transactions {
        if !advance_pending_raw(raw, &mut next, pending, nonces)? {
            return Ok(None);
        }
    }
    Ok(Some(next))
}

fn advance_pending_raw(
    raw: &[u8],
    next: &mut BTreeMap<Hash, u64>,
    pending: &BTreeMap<Hash, Pending>,
    nonces: &BTreeMap<Hash, u64>,
) -> Result<bool> {
    let decoded = match decode_transfer_v3(raw, 1024) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let hash = canonical_tx_hash(&decoded)?;
    let Some(entry) = pending.get(&hash).filter(|entry| entry.raw == raw) else {
        return Ok(false);
    };
    let expected = next
        .entry(entry.signer)
        .or_insert_with(|| nonces.get(&entry.signer).copied().unwrap_or(0));
    if *expected != entry.nonce {
        return Ok(false);
    }
    *expected = expected.checked_add(1).context("hint nonce overflow")?;
    Ok(true)
}

fn apfl_batch(raw: &[Vec<u8>], batch_size: usize) -> Result<Arc<ApflTransferBatch>> {
    Ok(Arc::new(ApflTransferBatch::from_raw(
        raw,
        ApflLimits {
            transactions: batch_size,
            transaction_bytes: 1024,
            body_bytes: super::body_byte_limit(batch_size),
        },
    )?))
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
            // Do not park between chunks of already-received signature work.
            // Each chunk still returns here through controller and HTTP polling.
            // Mere outbound backpressure is NOT work and must not cause a spin.
            if lifecycle.projection_error.is_some()
                || (lifecycle.incoming.is_none()
                    && lifecycle
                        .node
                        .controller
                        .transactions_stats()
                        .inbound_pending
                        == 0)
            {
                std::thread::sleep(Duration::from_millis(1));
            }
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

    #[test]
    fn apfl_pending_prefix_uses_exact_original_signature_and_nonce_not_codec_trust() -> Result<()> {
        let raws: Vec<_> = (0..3)
            .map(|n| super::super::tests::signed_raw(19, n))
            .collect();
        let mut pending = BTreeMap::new();
        for raw in &raws {
            let checked = authenticate_transfer_v3(raw, 71, 1024)?;
            pending.insert(
                checked.tx_hash(),
                Pending {
                    raw: raw.clone(),
                    signer: checked.nonce_identity(),
                    nonce: checked.transfer().nonce,
                },
            );
        }
        let signer = authenticate_transfer_v3(&raws[0], 71, 1024)?.nonce_identity();
        let mut altered = raws[0].clone();
        *altered.last_mut().unwrap() ^= 1;
        let cases = [
            (raws[..2].to_vec(), 0, true),
            (raws[1..].to_vec(), 0, false),
            (raws[1..].to_vec(), 1, true),
            (vec![raws[0].clone(), raws[0].clone()], 0, false),
            (vec![altered], 0, false),
            (vec![super::super::tests::signed_raw(20, 0)], 0, false),
        ];
        for (raw, nonce, accepted) in cases {
            let nonces = BTreeMap::from([(signer, nonce)]);
            let batch = apfl_batch(&raw, 8)?;
            let structured = pending_apfl_prefix(&batch, &pending, &nonces)?;
            assert_eq!(structured, pending_raw_prefix(&raw, &pending, &nonces)?);
            assert_eq!(structured.is_some(), accepted);
            assert_eq!(nonces[&signer], nonce);
            assert_eq!(pending.len(), 3);
        }
        Ok(())
    }

    #[test]
    fn early_selection_uses_exact_verified_bytes_and_never_mutates_parent_nonces() {
        let raws: Vec<_> = (0..3)
            .map(|n| super::super::tests::signed_raw(19, n))
            .collect();
        let mut pending = BTreeMap::new();
        let mut order = VecDeque::new();
        for raw in &raws {
            let checked = authenticate_transfer_v3(raw, 71, 1024).unwrap();
            order.push_back(checked.tx_hash());
            pending.insert(
                checked.tx_hash(),
                Pending {
                    raw: raw.clone(),
                    signer: checked.nonce_identity(),
                    nonce: checked.transfer().nonce,
                },
            );
        }
        let signer = authenticate_transfer_v3(&raws[0], 71, 1024)
            .unwrap()
            .nonce_identity();
        let nonces = BTreeMap::from([(signer, 0)]);
        let prefix = pending_raw_prefix(&raws[..2], &pending, &nonces)
            .unwrap()
            .unwrap();
        assert_eq!(prefix[&signer], 2);
        assert_eq!(
            select_pending_batch(&order, &pending, &nonces, prefix, 1024),
            vec![raws[2].clone()]
        );
        assert_eq!(nonces[&signer], 0);
        assert_eq!(pending.len(), 3);
        assert!(pending_raw_prefix(&raws[1..], &pending, &nonces)
            .unwrap()
            .is_none());
        assert!(
            pending_raw_prefix(&[raws[0].clone(), raws[0].clone()], &pending, &nonces)
                .unwrap()
                .is_none()
        );
        let mut tampered = raws[0].clone();
        *tampered.last_mut().unwrap() ^= 1;
        let canonical =
            |raw: &[u8]| canonical_tx_hash(&decode_transfer_v3(raw, 1024).unwrap()).unwrap();
        assert_eq!(
            canonical(&tampered),
            canonical(&raws[0]),
            "canonical ID omits signature"
        );
        assert!(pending_raw_prefix(&[tampered], &pending, &nonces)
            .unwrap()
            .is_none());
        let unknown = super::super::tests::signed_raw(20, 0);
        assert!(pending_raw_prefix(&[unknown], &pending, &nonces)
            .unwrap()
            .is_none());
        let advanced = BTreeMap::from([(signer, 1)]);
        assert!(pending_raw_prefix(&raws[..1], &pending, &advanced)
            .unwrap()
            .is_none());
        assert_eq!(
            pending_raw_prefix(&raws[1..], &pending, &advanced)
                .unwrap()
                .unwrap()[&signer],
            3
        );
    }

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
