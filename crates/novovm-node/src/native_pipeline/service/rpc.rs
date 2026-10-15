//! Product RPC adapter for the resident core. No RPC handler mutates balances,
//! publishes a head, constructs votes or substitutes a receipt for execution.
//! This experimental profile has a bounded, NON-durable pool and best-effort
//! signed-input fanout over the existing authenticated native channel. Receipt
//! queries, not transport admission, determine execution/finality. Clients must
//! retain signed input until confirmed and may resubmit after loss/restart.

use super::{ResidentConfig, ResidentNode, StartMode};
use crate::native_fresh_rpc::FreshRpcServer;
use crate::native_pipeline::business::nov_transfer_batch::balance_key;
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::consensus::statement::ParentPoint;
use crate::native_pipeline::consensus::transport::Message;
use crate::native_pipeline::consensus::wire::{Context as VoteContext, Hash};
use crate::native_pipeline::consensus::ArchiveRead;
use crate::native_pipeline::ingress::apfl::{ApflLimits, ApflTransferBatch};
#[cfg(test)]
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
#[cfg(test)]
use crate::native_pipeline::ingress::wire::{canonical_tx_hash, decode_transfer_v3};
use crate::native_pipeline::ingress::wire::{decode_transfer_view_v3, TransferView};
use crate::native_pipeline::persistence::io::IoTicket;
use crate::native_pipeline::persistence::packet::ReceiptView;
use crate::native_pipeline::pipeline::AdmissionInput;
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

/// Bounded cooperative handoff after real progress, then the original sleep.
/// Pending work alone cannot refresh the bound (offline peers/nonce gaps/slow
/// native owners must not cause an indefinite busy loop).
pub struct IdleBackoff {
    since: Instant,
    turns: u8,
}
impl IdleBackoff {
    pub fn new(now: Instant) -> Self {
        Self {
            since: now,
            turns: 0,
        }
    }
    pub fn should_sleep(&mut self, now: Instant, progress: bool) -> bool {
        if progress {
            self.since = now;
            self.turns = 0;
            return false;
        }
        self.turns = self.turns.saturating_add(1);
        self.turns >= 64 || now.saturating_duration_since(self.since) >= Duration::from_micros(250)
    }
}

mod admission;
mod gossip;
mod history;
mod native_query;
use novovm_protocol::native_query::{Balance, ReceiptState};

struct BalanceQuery {
    account: Account,
    root: Hash,
    height: u64,
    ticket: Option<IoTicket<Option<Vec<u8>>>>,
    result: Option<Balance>,
}

struct Pending {
    input: AdmissionInput,
    canonical_len: usize,
    signer: Hash,
    nonce: u64,
    /// Peers omitted from local fanout reservation, NOT delivery/finality bits.
    gossip_missing: u64,
}

impl Pending {
    fn view(&self) -> Result<TransferView<'_>> {
        match &self.input {
            AdmissionInput::Raw(raw) => decode_transfer_view_v3(raw, 1024),
            AdmissionInput::Apfl { batch, index } => batch.row(*index),
        }
    }

    fn matches(&self, view: TransferView<'_>) -> Result<bool> {
        match &self.input {
            AdmissionInput::Raw(raw) => view.matches_canonical_bytes(raw),
            AdmissionInput::Apfl { batch, index } => Ok(view == batch.row(*index)?),
        }
    }
}

/// Pool payload accounting. One retained row keeps its complete APFL owner
/// charged until the last pool reference is removed. Keys are allocation
/// identities, never dereferenced pointers or authentication identities.
#[derive(Default)]
struct PoolRetention {
    bytes: usize,
    owners: BTreeMap<usize, (usize, usize)>, // references, retained owner bytes
}

impl PoolRetention {
    fn reserve(&mut self, input: &AdmissionInput, limit: usize) -> Result<()> {
        let (owner, additional) = match input {
            AdmissionInput::Raw(raw) => (None, raw.capacity()),
            AdmissionInput::Apfl { batch, .. } => {
                let key = Arc::as_ptr(batch) as usize;
                let bytes = if self.owners.contains_key(&key) {
                    0
                } else {
                    batch.retained_bytes()?
                };
                (Some(key), bytes)
            }
        };
        let next = self
            .bytes
            .checked_add(additional)
            .context("pool retained bytes overflow")?;
        ensure!(
            next <= limit,
            "RPC retained pool backpressure; transaction not accepted"
        );
        if let Some(key) = owner {
            ensure!(
                self.owners.contains_key(&key) || self.owners.len() < MAX_POOL,
                "RPC retained owner count limit"
            );
            let record = self.owners.entry(key).or_insert((0, additional));
            record.0 = record
                .0
                .checked_add(1)
                .context("pool owner reference overflow")?;
        }
        self.bytes = next;
        Ok(())
    }

    fn release(&mut self, input: &AdmissionInput) {
        match input {
            AdmissionInput::Raw(raw) => self.bytes -= raw.capacity(),
            AdmissionInput::Apfl { batch, .. } => {
                let key = Arc::as_ptr(batch) as usize;
                let record = self.owners.get_mut(&key).expect("retained pool owner");
                record.0 -= 1;
                if record.0 == 0 {
                    self.bytes -= self.owners.remove(&key).unwrap().1;
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Position {
    height: u64,
    round: u64,
    parent: ParentPoint,
    // A private lookahead is tied to the source round, not just a parent hash.
    // This token never authorizes a proposal, vote, or published nonce update.
    source: Option<(VoteContext, u64)>,
}

struct GossipOffer {
    hashes: Vec<Hash>,
    message: Arc<Message>,
    repair_peer: Option<usize>,
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
    pending_retention: PoolRetention,
    nonces: BTreeMap<Hash, u64>,
    receipts: BTreeMap<Hash, ReceiptState>,
    recent: VecDeque<Hash>,
    indexed_height: u64,
    archive: Option<ArchiveRead>,
    offer: Option<(Position, Arc<Message>)>,
    offered: Option<Position>,
    gossip_order: VecDeque<Hash>,
    gossip_queued: BTreeSet<Hash>,
    gossip_offer: Option<GossipOffer>,
    gossip_repair: gossip::RepairHints,
    incoming: Option<(Arc<Message>, usize)>,
    incoming_from_client: bool,
    ingress_batch_boundary: bool,
    gossip_verified: u64,
    gossip_rejected: u64,
    authentication_cache_hits: u64,
    projection_error: Option<String>,
    balances: VecDeque<BalanceQuery>,
    admission: admission::AdmissionState,
    history: history::ReceiptHistory,
}

impl RpcLifecycle {
    /// Scheduling evidence only; pending work does not grant a busy-loop lease.
    pub fn admission_progress(&self) -> bool {
        self.admission.made_progress
    }
    pub fn new(node: ResidentNode, batch_size: usize) -> Result<Self> {
        ensure!((1..=1024).contains(&batch_size), "invalid RPC batch limit");
        Ok(Self {
            node,
            batch_size,
            pending: BTreeMap::new(),
            order: VecDeque::new(),
            reservations: BTreeSet::new(),
            pending_bytes: 0,
            pending_retention: PoolRetention::default(),
            nonces: BTreeMap::new(),
            receipts: BTreeMap::new(),
            recent: VecDeque::new(),
            indexed_height: 0,
            archive: None,
            offer: None,
            offered: None,
            gossip_order: VecDeque::new(),
            gossip_queued: BTreeSet::new(),
            gossip_offer: None,
            gossip_repair: gossip::RepairHints::default(),
            incoming: None,
            incoming_from_client: false,
            ingress_batch_boundary: false,
            gossip_verified: 0,
            gossip_rejected: 0,
            authentication_cache_hits: 0,
            projection_error: None,
            balances: VecDeque::new(),
            admission: admission::AdmissionState::default(),
            history: history::ReceiptHistory::default(),
        })
    }

    pub fn poll(&mut self, now: Instant) -> Result<()> {
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::Lifecycle,
        );
        self.node.poll(now)?;
        self.poll_balances()?;
        if self.projection_error.is_none() {
            let _span = crate::native_pipeline::profile::span(
                crate::native_pipeline::profile::Phase::Projection,
            );
            if let Err(error) = self.poll_projection() {
                self.projection_error = Some(format!("{error:#}"));
            }
        }
        // Projection/consensus retain first use of the bounded I/O owner.
        // Optional historical queries must not stall current-height selection.
        self.history.poll(&self.node.pipeline);
        self.poll_admission()?;
        self.poll_native_query(now)?;
        {
            let _span = crate::native_pipeline::profile::span(
                crate::native_pipeline::profile::Phase::Gossip,
            );
            self.poll_gossip()?;
        }
        if self.projection_error.is_some() || self.node.controller.is_recovering() {
            return Ok(());
        }
        // The durable parent receipt overlay is meaningful only on top of this
        // exact published projection. Neither it nor selection mutates nonces.
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
        let current_height = self.node.controller.context().height;
        let successor = if self.node.controller.is_local_leader()?
            && !self.node.controller.has_successor_target(current_height)
        {
            None
        } else {
            // Reuse one bounded lookahead within the existing body owner.
            // No duplicate body or repeated nonce projection while it runs.
            let Some(next_height) = current_height.checked_add(1) else {
                return Ok(());
            };
            if self.node.controller.has_successor_target(next_height) {
                return Ok(());
            }
            let Some(selection) = self.node.controller.successor_selection()? else {
                return Ok(());
            };
            Some(selection)
        };
        let parent = successor
            .as_ref()
            .map_or_else(|| self.node.controller.parent(), |s| s.parent);
        let position = Position {
            height: parent
                .height
                .checked_add(1)
                .context("candidate height exhausted")?,
            round: successor
                .as_ref()
                .map_or_else(|| self.node.controller.round(), |_| 0),
            parent,
            source: successor.as_ref().map(|s| (s.source, s.source_round)),
        };
        if self.offered == Some(position) {
            return Ok(());
        }
        if self.offer.as_ref().is_none_or(|(old, _)| *old != position) {
            let selected = select_pending_batch(
                &self.order,
                &self.pending,
                &self.nonces,
                successor
                    .as_ref()
                    .map_or_else(BTreeMap::new, |s| s.nonce_after.clone()),
                self.batch_size,
            );
            if selected.is_empty() {
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
            let batch = selected_apfl_batch(&selected, &self.pending, self.batch_size)?;
            self.offer = Some((position, Arc::new(Message::ApflBody { context, batch })));
        }
        let message = &self.offer.as_ref().expect("prepared native body").1;
        let accepted = match &successor {
            Some(selection) => {
                self.node
                    .controller
                    .offer_successor(message, selection, &self.node.pipeline)?
            }
            None => self.node.controller.offer(message)?,
        };
        if accepted {
            self.offered = Some(position);
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
                self.gossip_repair.remove(entry.gossip_missing);
                self.pending_bytes -= entry.canonical_len;
                self.pending_retention.release(&entry.input);
                self.reservations.remove(&(entry.signer, entry.nonce));
            }
            let hash = receipt.tx_hash;
            self.receipts.insert(
                hash,
                ReceiptState::Finalized {
                    success: receipt.success,
                    nonce_after: receipt.nonce_after,
                    charged_fee: receipt
                        .charged_fee
                        .parse()
                        .context("invalid archived fee width")?,
                    height: block.point().height,
                    block_hash: block.point().block_hash,
                },
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
            self.gossip_repair.remove(entry.gossip_missing);
            self.pending_bytes -= entry.canonical_len;
            self.pending_retention.release(&entry.input);
            self.reservations.remove(&(entry.signer, entry.nonce));
            self.receipts.insert(hash, ReceiptState::RejectedNonce);
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

    fn admit_checked(
        &mut self,
        input: crate::native_pipeline::pipeline::AdmissionInput,
        checked: crate::native_pipeline::ingress::authentication::SignatureCheckedTransfer,
        broadcast: bool,
    ) -> Result<Hash> {
        ensure!(
            self.projection_error.is_none(),
            "RPC projection unavailable"
        );
        let matches = match &input {
            crate::native_pipeline::pipeline::AdmissionInput::Raw(raw) => {
                checked.transfer().matches_canonical_bytes(raw)?
            }
            crate::native_pipeline::pipeline::AdmissionInput::Apfl { batch, index } => {
                checked.transfer() == batch.row(*index)?
            }
        };
        ensure!(matches, "admission signature/input mismatch");
        // Signature-checked memory admission is not current-state admission.
        // A lagging query projection must not stall ingress. Selection still
        // requires the exact published projection above; optional lookahead
        // overlays only locally executed durable parent receipts. Publication
        // alone removes stale nonces from the authoritative query projection.
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
        if self.receipts.contains_key(&hash) {
            return Ok(hash);
        }
        if self.pending.contains_key(&hash) {
            if broadcast {
                self.queue_gossip(hash);
            }
            return Ok(hash);
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
        let canonical_len = input.encoded_len()?;
        ensure!(
            self.pending.len() < MAX_POOL
                && canonical_len <= MAX_POOL_BYTES.saturating_sub(self.pending_bytes),
            "RPC pool backpressure; transaction not accepted"
        );
        self.pending_retention.reserve(&input, MAX_POOL_BYTES)?;
        self.pending_bytes += canonical_len;
        self.pending.insert(
            hash,
            Pending {
                input,
                canonical_len,
                signer,
                nonce,
                gossip_missing: 0,
            },
        );
        self.reservations.insert((signer, nonce));
        self.order.push_back(hash);
        if broadcast {
            self.queue_gossip(hash);
        }
        Ok(hash)
    }

    fn queue_gossip(&mut self, hash: Hash) {
        if self.gossip_queued.insert(hash) {
            self.gossip_order.push_back(hash);
        }
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
            query.result = Some(Balance {
                amount,
                state_root: query.root,
                height: query.height,
            });
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
        let result = self.balance_read(account.clone())?;
        Ok(match result {
            Some(value) => json!({"account":hex(account.as_bytes()),"asset":"NOV",
                "balance":value.amount.to_string(),"state_root":hex(&value.state_root),
                "block_height":value.height,"finalized":value.height>0,"query_complete":true}),
            None => json!({"query_complete":false,"state":"query_pending"}),
        })
    }

    fn balance_read(&mut self, account: Account) -> Result<Option<Balance>> {
        if let Some(position) = self.balances.iter().position(|q| q.account == account) {
            if self.balances[position].result.is_some() {
                return Ok(self.balances.remove(position).unwrap().result);
            }
            return Ok(None);
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
        Ok(None)
    }

    pub fn status(&self) -> Value {
        let core = &self.node.controller;
        let backend = match self.node.pipeline.execution_backend() {
            crate::native_pipeline::pipeline::ExecutionBackend::CpuCallbacks => {
                "aoem_semantic_v2_cpu_callbacks"
            }
            crate::native_pipeline::pipeline::ExecutionBackend::AoemIntegerV7 => {
                "aoem_integer_complete_effect_composition"
            }
        };
        let mut status = json!({"profile":self.node.profile,"experimental":true,
            "core":"novovm-node/native_pipeline -> novovm-exec/resident -> AOEM",
            "transport":"native_novorudp_apfl","consensus":"novovm-consensus/round_bft/journal",
            "chain_id":self.node.template.chain_id,"head":core.head(),
            "current_height":core.context().height,"round":core.round(),
            "scheduled_proposer":self.node.validators.leader(core.context().height,core.round()).ok().map(|h|hex(&h)),
            "local_proposer":core.is_local_leader().ok(),"recovery_in_progress":core.is_recovering(),
            "pending":self.pending.len(),"pending_bytes":self.pending_bytes,
            "rpc_indexed_height":self.indexed_height,"projection_error":self.projection_error,
            "executed_batches":core.executed(),"durable_decisions":core.head().map_or(0,|h|h.height),
            "execution_failures":core.rejected_execution(),"execution_backend":backend,
            "execution_integer_rows_total":core.integer_execution().0,
            "execution_integer_plans_total":core.integer_execution().1,
            "execution_apfl_view_transactions_total":core.apfl_execution_rows(),
            "requested_bodies":core.requested_bodies,"served_bodies":core.served_bodies,
            "native_pending_deliveries":core.pending_deliveries(),"native_rejected_messages":core.rejected_messages,
            "native_expired_sends":core.expired_sends,
            "successor_overlap_active":true,"business_proof_required":false,"business_proof_verified":false,
            "mempool_gossip":true,"gossip_delivery":"bounded_native_custody_not_durable_admission",
            "gossip_repair":self.gossip_repair.status(),"gossip_verified_inputs":self.gossip_verified,
            "gossip_rejected_inputs":self.gossip_rejected,"authentication_cache_hits":self.authentication_cache_hits,
            "rpc_authentication":self.admission.status(),"pending_survives_restart":false,
            "receipt_query_scope":"recent 65536 entries or explicit verified archive height",
            "historical_receipt_queries":self.history.len()});
        status["pending_retained_payload_bytes"] = json!(self.pending_retention.bytes);
        status["pending_retained_apfl_owners"] = json!(self.pending_retention.owners.len());
        status["stage_profile"] = crate::native_pipeline::profile::snapshot();
        let [auth, started, before_parent, promoted, discarded] = core.successor_metrics();
        status["successor"] = json!({"authentication_started":auth,"execution_started":started,
            "completed_before_parent":before_parent,"promoted":promoted,"discarded":discarded,
            "scope":"local_next_proposer_after_durable_parent_candidate"});
        status
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
                Some("nov_sendRawTransaction") => {
                    anyhow::bail!("signed admission requires deferred resident RPC")
                }
                Some("nov_getAssetBalance") => self.balance(&request["params"]),
                Some("nov_getTransactionStatus") => {
                    let (hash, height) = history::parse_lookup(&request)?;
                    if let Some(height) = height {
                        ensure!(
                            !self.node.controller.is_recovering(),
                            "historical receipt query unavailable during chain recovery"
                        );
                        let head = self.node.controller.head().context(
                            "historical receipt query requires a locally validated decided head",
                        )?;
                        return self.history.lookup(
                            hash,
                            height,
                            head,
                            self.node.controller.context(),
                            self.node.validators.clone(),
                        );
                    }
                    Ok(self.receipt_status_json(hash))
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
#[cfg(test)]
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

#[cfg(test)]
fn pending_apfl_prefix(
    batch: &ApflTransferBatch,
    pending: &BTreeMap<Hash, Pending>,
    nonces: &BTreeMap<Hash, u64>,
) -> Result<Option<BTreeMap<Hash, u64>>> {
    let mut next = BTreeMap::new();
    for index in 0..batch.len() {
        let row = batch.row(index)?;
        let hash = row.canonical_tx_hash()?;
        let Some(entry) = pending.get(&hash) else {
            return Ok(None);
        };
        if !entry.matches(row)? {
            return Ok(None);
        }
        let expected = next
            .entry(entry.signer)
            .or_insert_with(|| nonces.get(&entry.signer).copied().unwrap_or(0));
        if *expected != entry.nonce {
            return Ok(None);
        }
        *expected = expected.checked_add(1).context("hint nonce overflow")?;
    }
    Ok(Some(next))
}

#[cfg(test)]
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

#[cfg(test)]
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
    let Some(entry) = pending.get(&hash) else {
        return Ok(false);
    };
    if !entry.matches(decoded.as_view())? {
        return Ok(false);
    }
    let expected = next
        .entry(entry.signer)
        .or_insert_with(|| nonces.get(&entry.signer).copied().unwrap_or(0));
    if *expected != entry.nonce {
        return Ok(false);
    }
    *expected = expected.checked_add(1).context("hint nonce overflow")?;
    Ok(true)
}

#[cfg(test)]
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

fn selected_apfl_batch(
    selected: &[Hash],
    pending: &BTreeMap<Hash, Pending>,
    batch_size: usize,
) -> Result<Arc<ApflTransferBatch>> {
    let views = selected
        .iter()
        .map(|hash| {
            pending
                .get(hash)
                .context("selected pending row missing")?
                .view()
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Arc::new(ApflTransferBatch::from_views(
        &views,
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
) -> Vec<Hash> {
    let mut selected = Vec::new();
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
            if raw_bytes + entry.canonical_len > body_limit {
                break;
            }
            raw_bytes += entry.canonical_len;
            selected.push(*hash);
            *expected += 1;
            if selected.len() == batch_size {
                break;
            }
        }
    }
    selected
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
    hex_param(&params[0], max)
}

fn hex_param(value: &Value, max: usize) -> Result<Vec<u8>> {
    let text = value.as_str().context("hex string required")?;
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
    // Never open an HTTP endpoint in the native release. Explicit historical
    // builds alone may observe the old RPC contract for same-core comparison.
    let mut rpc = config
        .historical_http_addr()?
        .map(|address| FreshRpcServer::bind_with_max_request(address, 4 * 1024 * 1024))
        .transpose()?;
    let batch_size = config.batch_size;
    let mut lifecycle = RpcLifecycle::new(ResidentNode::start(config, mode)?, batch_size)?;
    let started = Instant::now();
    let mut idle = IdleBackoff::new(started);
    if let Some(rpc) = &rpc {
        println!("historical_rpc_listening={}", rpc.local_addr()?);
    } else {
        println!("native_application_runtime=true http_json_rpc=false");
    }
    let result = (|| -> Result<()> {
        loop {
            let now = Instant::now();
            let before = control_progress(&lifecycle);
            lifecycle.poll(now)?;
            let rpc_progress = {
                let _span = crate::native_pipeline::profile::span(
                    crate::native_pipeline::profile::Phase::Http,
                );
                match &mut rpc {
                    Some(rpc) => rpc.poll_deferred(&mut lifecycle)?,
                    None => false,
                }
            };
            if run_for.is_some_and(|limit| started.elapsed() >= limit) {
                break;
            }
            // HTTP silence is not controller idleness. Allow bounded handoff
            // turns after actual progress, without changing protocol timers,
            // queue bounds, durability or the ordinary 1ms idle sleep.
            let progress = rpc_progress
                || lifecycle.admission.made_progress
                || before != control_progress(&lifecycle);
            if idle.should_sleep(Instant::now(), progress) {
                let _span = crate::native_pipeline::profile::span(
                    crate::native_pipeline::profile::Phase::Idle,
                );
                std::thread::sleep(Duration::from_millis(1));
            } else if !progress {
                let _span = crate::native_pipeline::profile::span(
                    crate::native_pipeline::profile::Phase::Yield,
                );
                std::thread::yield_now();
            }
        }
        Ok(())
    })();
    let drain = lifecycle.node.shutdown();
    result.and(drain)
}

/// O(1) scheduling hint, never an execution, publication or signing permit.
pub fn control_progress(lifecycle: &RpcLifecycle) -> [u64; 10] {
    let core = &lifecycle.node.controller;
    [
        core.executed(),
        core.context().height,
        core.round(),
        core.pending_deliveries() as u64,
        core.requested_bodies,
        core.served_bodies,
        lifecycle.indexed_height,
        lifecycle.gossip_verified,
        core.io_progress_generation(),
        core.successor_metrics()
            .into_iter()
            .fold(0_u64, u64::wrapping_add),
    ]
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
    #[test]
    fn idle_handoff_is_bounded_by_time_and_turns_and_only_progress_resets_it() {
        let now = std::time::Instant::now();
        let mut idle = super::IdleBackoff::new(now);
        for _ in 0..63 {
            assert!(!idle.should_sleep(now, false));
        }
        assert!(idle.should_sleep(now, false));
        for _ in 0..300 {
            assert!(idle.should_sleep(now, false));
        }
        assert!(!idle.should_sleep(now, true));
        assert!(!idle.should_sleep(now + std::time::Duration::from_micros(249), false));
        assert!(idle.should_sleep(now + std::time::Duration::from_micros(250), false));
        assert!(!idle.should_sleep(now + std::time::Duration::from_millis(3), true));
    }

    use super::*;

    #[test]
    fn prepared_selection_identity_includes_full_parent_and_source_round() {
        let parent = ParentPoint {
            height: 3,
            block_hash: [1; 32],
            state_root: [2; 32],
            receipt_batch_commitment: [3; 32],
            state_version: 12,
            decision_hash: [4; 32],
        };
        let source = VoteContext {
            chain_id: 71,
            genesis_config_commitment: [5; 32],
            protocol_commitment: [6; 32],
            epoch: 1,
            validator_set_hash: [7; 32],
            height: 3,
            parent_block_hash: [8; 32],
            parent_decision_hash: [9; 32],
        };
        let position = Position {
            height: 4,
            round: 0,
            parent,
            source: Some((source, 2)),
        };
        assert!(
            position
                != Position {
                    source: Some((source, 3)),
                    ..position
                }
        );
        assert!(
            position
                != Position {
                    source: None,
                    ..position
                }
        );
        assert!(
            position
                != Position {
                    source: Some((
                        VoteContext {
                            parent_decision_hash: [10; 32],
                            ..source
                        },
                        2
                    )),
                    ..position
                }
        );
        for changed in [
            ParentPoint {
                height: 4,
                ..parent
            },
            ParentPoint {
                block_hash: [11; 32],
                ..parent
            },
            ParentPoint {
                state_root: [12; 32],
                ..parent
            },
            ParentPoint {
                receipt_batch_commitment: [13; 32],
                ..parent
            },
            ParentPoint {
                state_version: 13,
                ..parent
            },
            ParentPoint {
                decision_hash: [14; 32],
                ..parent
            },
        ] {
            assert!(
                position
                    != Position {
                        parent: changed,
                        ..position
                    }
            );
        }
    }

    #[test]
    fn durable_successor_nonce_overlay_keeps_published_projection_and_pending_unchanged() {
        let (order, mut pending) = pending_fixture(4, 8);
        // The last row belongs to an independent sender and must use the
        // published prefix when the durable parent did not touch that sender.
        let independent = *order.back().unwrap();
        pending.get_mut(&independent).unwrap().signer = [2; 32];
        let published = BTreeMap::from([([1; 32], 0), ([2; 32], 3)]);
        // This is an already-executed parent result (success/failure nonce
        // semantics are covered by the real-pipeline tests), not guessed credit.
        let next = BTreeMap::from([([1; 32], 2)]);
        let selected = select_pending_batch(&order, &pending, &published, next.clone(), 4);
        assert_eq!(selected, vec![order[2], independent]);
        assert_eq!(published[&[1; 32]], 0);
        assert_eq!(next[&[1; 32]], 2);
        assert_eq!(pending.len(), 4);
        assert_eq!(order.len(), 4);
    }

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
                    input: AdmissionInput::Raw(raw.clone()),
                    canonical_len: raw.len(),
                    signer: checked.nonce_identity(),
                    nonce: checked.transfer().nonce,
                    gossip_missing: 0,
                },
            );
        }
        let signer = authenticate_transfer_v3(&raws[0], 71, 1024)?.nonce_identity();
        // A protocol hint is never a verified candidate-body prefix.
        assert!(pending_body_prefix(
            &Message::RequestBody { body_id: [0; 32] },
            &pending,
            &BTreeMap::new(),
        )?
        .is_none());
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
                    input: AdmissionInput::Raw(raw.clone()),
                    canonical_len: raw.len(),
                    signer: checked.nonce_identity(),
                    nonce: checked.transfer().nonce,
                    gossip_missing: 0,
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
            selected_raw_for_test(
                &select_pending_batch(&order, &pending, &nonces, prefix, 1024),
                &pending
            ),
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
                    canonical_len: raw.len(),
                    input: AdmissionInput::Raw(raw),
                    signer: [1; 32],
                    nonce: nonce as u64,
                    gossip_missing: 0,
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
            selected
                .iter()
                .map(|hash| pending[hash].canonical_len)
                .sum::<usize>(),
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
        let next = selected_raw_for_test(&next, &pending);
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
        assert_eq!(
            selected_raw_for_test(&selected, &pending),
            vec![0_u64.to_le_bytes().to_vec()]
        );
        let (order, pending) = pending_fixture(5, 8);
        let successor = BTreeMap::from([([1; 32], 2)]);
        let selected = select_pending_batch(&order, &pending, &BTreeMap::new(), successor, 2);
        assert_eq!(
            selected_raw_for_test(&selected, &pending),
            vec![2_u64.to_le_bytes().to_vec(), 3_u64.to_le_bytes().to_vec()]
        );
        assert_eq!(pending.len(), 5);
    }

    fn selected_raw_for_test(selected: &[Hash], pending: &BTreeMap<Hash, Pending>) -> Vec<Vec<u8>> {
        selected
            .iter()
            .map(|hash| match &pending[hash].input {
                AdmissionInput::Raw(raw) => raw.clone(),
                AdmissionInput::Apfl { batch, index } => batch.canonical_raw(*index).unwrap(),
            })
            .collect()
    }

    #[test]
    fn pending_apfl_selection_preserves_shared_rows_signature_and_nonce_order() -> Result<()> {
        let raws: Vec<_> = (0..4)
            .map(|n| super::super::tests::signed_raw(19, n))
            .collect();
        let source = apfl_batch(&raws, 8)?;
        let mut pending = BTreeMap::new();
        let mut order = VecDeque::new();
        for (index, raw) in raws.iter().enumerate() {
            let checked = authenticate_transfer_v3(raw, 71, 1024)?;
            order.push_back(checked.tx_hash());
            pending.insert(
                checked.tx_hash(),
                Pending {
                    input: AdmissionInput::Apfl {
                        batch: source.clone(),
                        index,
                    },
                    canonical_len: raw.len(),
                    signer: checked.nonce_identity(),
                    nonce: checked.transfer().nonce,
                    gossip_missing: 0,
                },
            );
        }
        let selected = select_pending_batch(&order, &pending, &BTreeMap::new(), BTreeMap::new(), 2);
        assert_eq!(selected, order.iter().take(2).copied().collect::<Vec<_>>());
        let batch = selected_apfl_batch(&selected, &pending, 2)?;
        assert_eq!(batch.len(), 2);
        for (index, raw) in raws.iter().take(2).enumerate() {
            assert!(batch.row(index)?.matches_canonical_bytes(raw)?);
        }
        for entry in pending.values() {
            let AdmissionInput::Apfl { batch, .. } = &entry.input else {
                panic!("APFL pending row expanded");
            };
            assert!(Arc::ptr_eq(batch, &source));
        }
        let entry = &pending[&order[0]];
        assert!(entry.matches(source.row(0)?)?);
        let mut bad = raws[0].clone();
        *bad.last_mut().unwrap() ^= 1;
        let bad_view = decode_transfer_view_v3(&bad, 1024)?;
        assert_eq!(
            bad_view.canonical_tx_hash()?,
            source.row(0)?.canonical_tx_hash()?
        );
        assert!(!entry.matches(bad_view)?, "hash-only signature cache reuse");
        // Raw compatibility and APFL rows can share one ordered selected body.
        let last = order[1];
        pending.get_mut(&last).unwrap().input = AdmissionInput::Raw(raws[1].clone());
        let mixed = selected_apfl_batch(&selected, &pending, 2)?;
        assert_eq!(mixed.encode()?, batch.encode()?);
        assert_eq!(pending.len(), 4);
        Ok(())
    }

    #[test]
    fn pool_retention_keeps_full_owner_charged_until_last_row_and_rejects_without_mutation(
    ) -> Result<()> {
        let raws: Vec<_> = (0..3)
            .map(|n| super::super::tests::signed_raw(19, n))
            .collect();
        let source = apfl_batch(&raws, 8)?;
        let owner_bytes = source.retained_bytes()?;
        let rows: Vec<_> = (0..3)
            .map(|index| AdmissionInput::Apfl {
                batch: source.clone(),
                index,
            })
            .collect();
        let mut retained = PoolRetention::default();
        assert!(retained.reserve(&rows[0], owner_bytes - 1).is_err());
        assert_eq!(retained.bytes, 0);
        assert!(retained.owners.is_empty());
        for row in &rows {
            retained.reserve(row, owner_bytes)?;
        }
        assert_eq!(retained.bytes, owner_bytes);
        assert_eq!(retained.owners.len(), 1);
        retained.release(&rows[0]);
        retained.release(&rows[1]);
        assert_eq!(
            retained.bytes, owner_bytes,
            "one remaining row pins the full owner"
        );
        let other = apfl_batch(&raws, 8)?;
        let other_row = AdmissionInput::Apfl {
            batch: other,
            index: 0,
        };
        assert!(retained.reserve(&other_row, owner_bytes).is_err());
        assert_eq!(retained.bytes, owner_bytes);
        assert_eq!(retained.owners.len(), 1);
        retained.release(&rows[2]);
        assert_eq!(retained.bytes, 0);
        assert!(retained.owners.is_empty());
        retained.reserve(&other_row, owner_bytes)?;
        retained.release(&other_row);
        assert_eq!(retained.bytes, 0);
        Ok(())
    }

    #[test]
    fn raw_pool_retention_charges_capacity_not_only_visible_length() -> Result<()> {
        let mut raw = Vec::with_capacity(4096);
        raw.extend_from_slice(&[1; 8]);
        let capacity = raw.capacity();
        let input = AdmissionInput::Raw(raw);
        let mut retained = PoolRetention::default();
        assert!(retained.reserve(&input, 8).is_err());
        assert_eq!(retained.bytes, 0);
        retained.reserve(&input, capacity)?;
        assert_eq!(retained.bytes, capacity);
        retained.release(&input);
        assert_eq!(retained.bytes, 0);
        Ok(())
    }

    fn query(index: u8, complete: bool) -> BalanceQuery {
        BalanceQuery {
            account: Account::try_from(vec![index; 20]).unwrap(),
            root: [index; 32],
            height: 1,
            ticket: None,
            result: complete.then_some(Balance {
                amount: 7,
                state_root: [index; 32],
                height: 1,
            }),
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
        balances[4].result = Some(Balance {
            amount: 7,
            state_root: [4; 32],
            height: 1,
        });
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
