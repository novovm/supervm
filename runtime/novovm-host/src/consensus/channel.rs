//! Resident assembly/ingress owner above the authenticated network worker.
//! Full encoding, decoding, body hashing, certificate verification and raw-body
//! cloning happen here, never in `try_*`. Neither preparation nor transport
//! admission grants current-parent, execution, signing or finality authority.
//!
//! Budgets cover retained logical content, not allocator/TLS overhead or input
//! allocations already owned by a local producer. New input reserves the
//! configured worst-case content before owner-side validation. Local replies,
//! remote reception, and outgoing jobs have independent control/body lanes.
//! Callers must budget handles/replies they retain after taking ownership.

use super::transport::{self, DecodeLimits, EarlyBodyScope, Message};
use super::wire::{Hash, Phase, ValidatorSet, VerifiedProposal, VerifiedQuorum, VerifiedVote};
use crate::business::direct_nov_fee::DirectNovFeePolicy;
use crate::execution::plan::BatchContext;
use crate::persistence::StoredCandidate;
use crate::pipeline::{
    AuthenticatedBody, AuthenticatedRequest, AuthenticationRequest, AuthenticationTicket,
    BatchRequest, DurableBatch, DurableCandidate,
};
use anyhow::{ensure, Context, Result};
use novovm_network::fragments::{
    FragmentAdmission, OutgoingMessage, Reassembler, ReassemblyLimits,
};
use novovm_network::worker::{NetworkWorker, SendAdmission as NetworkAdmission};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct QueueBudget {
    pub messages: usize,
    pub bytes: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct LaneBudget {
    pub control: QueueBudget,
    pub body: QueueBudget,
}

#[derive(Clone)]
pub struct ChannelConfig {
    pub chain_id: u64,
    pub genesis: Hash,
    pub protocol: Hash,
    pub peers: Vec<String>,
    pub validators: Arc<ValidatorSet>,
    pub policy: DirectNovFeePolicy,
    pub codec: DecodeLimits,
    pub reassembly: ReassemblyLimits,
    pub prepare: LaneBudget,
    /// Global outgoing ceilings, divided equally among configured peers for
    /// each lane. Unused remainder is not lent to a blocked/offline peer.
    pub send: LaneBudget,
    pub receive: LaneBudget,
    pub ttl: Duration,
}

#[derive(Clone, Debug)]
pub enum VerifiedEvidence {
    None,
    Proposal {
        proposal: VerifiedProposal,
        valid_quorum: Option<VerifiedQuorum>,
    },
    Vote(VerifiedVote),
    Decision {
        proposal: VerifiedProposal,
        certificate: VerifiedQuorum,
    },
}

struct PreparedInner {
    scope: Arc<()>,
    message: Arc<Message>,
    encoded: OutgoingMessage,
    evidence: Arc<VerifiedEvidence>,
    body_id: Option<Hash>,
    early_id: Option<Hash>,
    charge: usize,
    lane: usize,
}

/// Immutable owner-prepared bytes. Clone/retransmit is O(1), and never repeats
/// full-body encoding/hashing. A handle belongs to its originating channel.
#[derive(Clone)]
pub struct PreparedMessage(Arc<PreparedInner>);

impl PreparedMessage {
    pub fn message(&self) -> &Arc<Message> {
        &self.0.message
    }
    pub fn body_id(&self) -> Option<Hash> {
        self.0.body_id
    }
    pub fn early_id(&self) -> Option<Hash> {
        self.0.early_id
    }
    pub fn fragment_id(&self) -> Hash {
        self.0.encoded.id()
    }
    /// Conservative logical-content charge for the controller's own cache.
    /// Taking a ready handle transfers retention responsibility to its caller.
    pub fn retained_bytes(&self) -> usize {
        self.0.charge
    }
}

/// Unverified business input, not a candidate or permission to execute/vote.
/// The controller must pin its parent/program/policy before taking the request.
/// A retry uses `PrepareInput::Cached`, so raw cloning remains on the owner.
pub struct PreparedBody {
    id: Hash,
    context: BatchContext,
    message: Arc<Message>,
    request: Option<BatchRequest>,
}

impl PreparedBody {
    pub fn id(&self) -> Hash {
        self.id
    }
    pub fn context(&self) -> &BatchContext {
        &self.context
    }
    pub fn message(&self) -> &Arc<Message> {
        &self.message
    }
    pub fn take_request(&mut self) -> Option<BatchRequest> {
        self.request.take()
    }
}

/// Parent-independent, unverified input. Neither this wrapper nor its identity
/// grants permission to execute against a parent, persist, or vote.
pub struct PreparedEarlyBody {
    id: Hash,
    scope: EarlyBodyScope,
    request: Option<AuthenticationRequest>,
}

impl PreparedEarlyBody {
    pub fn id(&self) -> Hash {
        self.id
    }
    pub fn scope(&self) -> &EarlyBodyScope {
        &self.scope
    }
    pub fn take_request(&mut self) -> Option<AuthenticationRequest> {
        self.request.take()
    }
}

pub struct Ready {
    pub message: Arc<Message>,
    pub prepared: PreparedMessage,
    pub evidence: Arc<VerifiedEvidence>,
    pub body: Option<PreparedBody>,
    pub early: Option<PreparedEarlyBody>,
    /// Correlation for this binding reply, not authorization of its parent.
    pub bound_early: Option<(EarlyBodyScope, Hash)>,
}

pub struct Received {
    pub peer: String,
    pub ready: Ready,
}

// Binding carries only a fixed-size context beside its O(1) handle. Do not
// require a new heap allocation on the admission/control caller.
#[allow(clippy::large_enum_variant)]
pub enum PrepareInput {
    New(Arc<Message>),
    Cached(PreparedMessage),
    /// A validated local archive's bytes, copied only on this owner. It does
    /// not grant permission to execute or sign a historical/current height.
    StoredBody(Arc<StoredCandidate>),
    /// Rebuild the canonical old Body on its originating owner. Both the old
    /// prepared allocation and the new worst-case reply are reserved together.
    BindEarly {
        early: PreparedMessage,
        context: BatchContext,
    },
}

pub struct PrepareRequest {
    pub token: u64,
    pub input: PrepareInput,
}

pub enum PrepareAdmission {
    Accepted,
    Backpressure(PrepareRequest),
    Rejected {
        request: PrepareRequest,
        reason: String,
    },
}

pub struct Outbound {
    pub peer: String,
    pub message: PreparedMessage,
}

pub enum SendAdmission {
    Accepted,
    Backpressure(Outbound),
    Rejected { outbound: Outbound, reason: String },
}

/// Bounded, explicitly typed data-plane reclamation. No signer, database,
/// executable callback or publication capability can enter this queue.
// Keep the fixed-size wrapper inline: retiring on a latency-sensitive caller
// must not require a new Box allocation. Entry counts are bounded, and the
// per-entry 4096-byte allowance exceeds this wrapper's retained content.
#[allow(clippy::large_enum_variant)]
pub enum Retirement {
    Ready(Ready),
    Prepared(PreparedMessage),
    Body {
        prepared: PreparedMessage,
        request: Option<BatchRequest>,
        candidate: Option<DurableCandidate>,
    },
    Candidate(DurableCandidate),
    Batch(DurableBatch),
    Input(PrepareInput),
    Request(BatchRequest),
    AuthenticationRequest(AuthenticationRequest),
    AuthenticatedBody(AuthenticatedBody),
    AuthenticatedRequest(AuthenticatedRequest),
    AuthenticationTicket(AuthenticationTicket),
    Message(Arc<Message>),
}

pub enum RetireAdmission {
    Accepted,
    Backpressure(Retirement),
    Rejected { value: Retirement, reason: String },
}

pub enum ChannelEvent {
    Prepared {
        token: u64,
        result: std::result::Result<Ready, String>,
    },
    Received(Received),
}

#[derive(Clone, Debug, Default)]
pub struct ChannelStatus {
    pub stopped: bool,
    pub encoded_messages: u64,
    pub prepared_bodies: u64,
    pub preparation_failures: u64,
    pub received_messages: u64,
    pub invalid_received: u64,
    pub dropped_received: u64,
    pub sent_messages: u64,
    pub sent_frames: u64,
    pub expired_sends: u64,
    pub rejected_sends: u64,
    pub retired_messages: u64,
    pub pending_prepare: usize,
    pub pending_send: usize,
    pub pending_receive: usize,
    pub pending_retirement: usize,
    pub last_error: Option<String>,
}

#[derive(Default)]
struct Usage {
    messages: usize,
    bytes: usize,
}

impl Usage {
    fn reserve(&mut self, budget: QueueBudget, charge: usize) -> bool {
        if self.messages >= budget.messages || charge > budget.bytes.saturating_sub(self.bytes) {
            return false;
        }
        self.messages += 1;
        self.bytes += charge;
        true
    }
    fn release(&mut self, charge: usize) {
        self.messages -= 1;
        self.bytes -= charge;
    }
}

struct Charged<T> {
    value: T,
    charge: usize,
    created: Instant,
}
struct SendJob {
    outbound: Outbound,
    next: usize,
    pending_frame: Option<Vec<u8>>,
}
struct LocalReply {
    token: u64,
    result: std::result::Result<Ready, String>,
}

#[derive(Default)]
struct Shared {
    prepare: [VecDeque<Charged<PrepareRequest>>; 2],
    local: [VecDeque<Charged<LocalReply>>; 2],
    send: [VecDeque<Charged<SendJob>>; 2],
    receive: [VecDeque<Charged<Received>>; 2],
    retire: [VecDeque<Charged<Retirement>>; 2],
    prepare_usage: [Usage; 2],
    send_usage: [Usage; 2],
    send_peer_usage: [BTreeMap<String, Usage>; 2],
    receive_usage: [Usage; 2],
    retire_usage: [Usage; 2],
    event_turn: usize,
    status: ChannelStatus,
}

/// `try_*` never waits on a socket, owner, mutex or whole-body operation.
/// Only explicit shutdown/Drop joins the owner (and its bounded network I/O).
pub struct HostChannel {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    scope: Arc<()>,
    peers: BTreeSet<String>,
    prepare_budget: LaneBudget,
    send_budget: LaneBudget,
    charge: usize,
    ttl: Duration,
    worker: Option<JoinHandle<()>>,
}

fn lane(message: &Message) -> usize {
    usize::from(matches!(
        message,
        Message::Body { .. } | Message::EarlyBody { .. }
    ))
}
fn budget(limits: LaneBudget, lane: usize) -> QueueBudget {
    if lane == 0 {
        limits.control
    } else {
        limits.body
    }
}
fn peer_budget(global: QueueBudget, peers: usize) -> QueueBudget {
    QueueBudget {
        messages: global.messages / peers.max(1),
        bytes: global.bytes / peers.max(1),
    }
}
fn short_error(error: impl std::fmt::Display) -> String {
    error.to_string().chars().take(512).collect()
}

impl HostChannel {
    /// O(1) logical-content reservation used by this channel for each message.
    /// Controllers can use the same bound for their own retained-body quotas.
    pub fn preparation_charge(&self) -> usize {
        self.charge
    }

    /// Optional early binding co-retains two distinct body allocations. Old
    /// one-body configurations remain valid, but must use ordinary Body input.
    pub(crate) fn supports_early_binding(&self) -> bool {
        self.charge
            .checked_mul(2)
            .is_some_and(|bytes| bytes <= self.prepare_budget.body.bytes)
    }

    pub fn start(network: NetworkWorker, config: ChannelConfig) -> Result<Self> {
        let (assembler, charge) = validate(&config)?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let scope = Arc::new(());
        let owner_shared = shared.clone();
        let owner_stop = stop.clone();
        let owner_scope = scope.clone();
        let peers = config.peers.iter().cloned().collect();
        let prepare_budget = config.prepare;
        let send_budget = config.send;
        let ttl = config.ttl;
        let worker = thread::Builder::new()
            .name("novovm-host-ingress".into())
            .spawn(move || {
                let result = run(
                    network,
                    &config,
                    assembler,
                    charge,
                    &owner_scope,
                    &owner_shared,
                    &owner_stop,
                );
                if let Err(error) = result {
                    if let Ok(mut shared) = owner_shared.lock() {
                        shared.status.last_error = Some(short_error(error));
                    }
                }
                owner_stop.store(true, Ordering::Release);
            })
            .context("spawn host assembly owner")?;
        Ok(Self {
            shared,
            stop,
            scope,
            peers,
            prepare_budget,
            send_budget,
            charge,
            ttl,
            worker: Some(worker),
        })
    }

    pub fn try_prepare(&self, request: PrepareRequest) -> Result<PrepareAdmission> {
        let selected = match &request.input {
            PrepareInput::New(message) => lane(message),
            PrepareInput::Cached(message) => {
                if !Arc::ptr_eq(&message.0.scope, &self.scope) {
                    return Ok(PrepareAdmission::Rejected {
                        request,
                        reason: "prepared handle belongs to another channel".into(),
                    });
                }
                message.0.lane
            }
            PrepareInput::StoredBody(_) => 1,
            PrepareInput::BindEarly { early, .. } => {
                if !Arc::ptr_eq(&early.0.scope, &self.scope) {
                    return Ok(PrepareAdmission::Rejected {
                        request,
                        reason: "prepared handle belongs to another channel".into(),
                    });
                }
                1
            }
        };
        // Archive input also retains decoded receipts/state nodes while queued;
        // its cached record size is available without walking the body here.
        let charge = match &request.input {
            PrepareInput::StoredBody(stored) => self.charge.checked_add(stored.record_bytes()),
            // Equal announcement identities do not make the retained early
            // allocation and the freshly rebuilt canonical body one allocation.
            PrepareInput::BindEarly { early, .. } => {
                self.charge.checked_add(early.retained_bytes())
            }
            _ => Some(self.charge),
        };
        let Some(charge) =
            charge.filter(|charge| *charge <= budget(self.prepare_budget, selected).bytes)
        else {
            return Ok(PrepareAdmission::Rejected {
                request,
                reason: "preparation exceeds lane byte ceiling".into(),
            });
        };
        if self.stop.load(Ordering::Acquire) {
            return Ok(PrepareAdmission::Rejected {
                request,
                reason: "channel stopped".into(),
            });
        }
        let mut shared = match self.shared.try_lock() {
            Ok(shared) => shared,
            Err(TryLockError::WouldBlock) => return Ok(PrepareAdmission::Backpressure(request)),
            Err(TryLockError::Poisoned(_)) => {
                return Ok(PrepareAdmission::Rejected {
                    request,
                    reason: "channel queue poisoned".into(),
                })
            }
        };
        if !shared.prepare_usage[selected].reserve(budget(self.prepare_budget, selected), charge) {
            return Ok(PrepareAdmission::Backpressure(request));
        }
        shared.prepare[selected].push_back(Charged {
            value: request,
            charge,
            created: Instant::now(),
        });
        drop(shared);
        self.wake();
        Ok(PrepareAdmission::Accepted)
    }

    pub fn try_send(&self, outbound: Outbound) -> Result<SendAdmission> {
        let reason = if self.stop.load(Ordering::Acquire) {
            Some("channel stopped")
        } else if !self.peers.contains(&outbound.peer) {
            Some("unconfigured peer")
        } else if !Arc::ptr_eq(&outbound.message.0.scope, &self.scope) {
            Some("prepared handle belongs to another channel")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Ok(SendAdmission::Rejected {
                outbound,
                reason: reason.into(),
            });
        }
        let selected = outbound.message.0.lane;
        let charge = outbound.message.0.charge;
        let mut shared = match self.shared.try_lock() {
            Ok(shared) => shared,
            Err(TryLockError::WouldBlock) => return Ok(SendAdmission::Backpressure(outbound)),
            Err(TryLockError::Poisoned(_)) => {
                return Ok(SendAdmission::Rejected {
                    outbound,
                    reason: "channel queue poisoned".into(),
                })
            }
        };
        let global = budget(self.send_budget, selected);
        if !shared.send_peer_usage[selected]
            .entry(outbound.peer.clone())
            .or_default()
            .reserve(peer_budget(global, self.peers.len()), charge)
        {
            return Ok(SendAdmission::Backpressure(outbound));
        }
        if !shared.send_usage[selected].reserve(global, charge) {
            shared.send_peer_usage[selected]
                .get_mut(&outbound.peer)
                .expect("reserved peer")
                .release(charge);
            return Ok(SendAdmission::Backpressure(outbound));
        }
        shared.send[selected].push_back(Charged {
            value: SendJob {
                outbound,
                next: 0,
                pending_frame: None,
            },
            charge,
            created: Instant::now(),
        });
        drop(shared);
        self.wake();
        Ok(SendAdmission::Accepted)
    }

    pub fn try_recv(&self) -> Result<Option<ChannelEvent>> {
        let mut shared = match self.shared.try_lock() {
            Ok(shared) => shared,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => anyhow::bail!("channel queue poisoned"),
        };
        // Four independent sources: local control/body and remote control/body.
        for offset in 0..4 {
            let selected = (shared.event_turn + offset) % 4;
            if selected < 2 {
                if let Some(reply) = shared.local[selected].pop_front() {
                    shared.prepare_usage[selected].release(reply.charge);
                    shared.event_turn = (selected + 1) % 4;
                    return Ok(Some(ChannelEvent::Prepared {
                        token: reply.value.token,
                        result: reply.value.result,
                    }));
                }
            } else {
                let lane = selected - 2;
                if shared.receive[lane]
                    .front()
                    .is_some_and(|received| received.created.elapsed() >= self.ttl)
                {
                    // Do not destroy a potentially large body on the control
                    // caller. The owner retires it and returns the quota.
                    shared.event_turn = (selected + 1) % 4;
                    self.wake();
                    return Ok(None);
                }
                if let Some(received) = shared.receive[lane].pop_front() {
                    shared.receive_usage[lane].release(received.charge);
                    shared.event_turn = (selected + 1) % 4;
                    return Ok(Some(ChannelEvent::Received(received.value)));
                }
            }
        }
        Ok(None)
    }

    /// Transfer final destruction to the owner without inspecting raw content.
    /// These lanes have independent usage but the same limits as preparation.
    /// On any non-acceptance the caller MUST retain the returned value and
    /// backpressure its producer; silently dropping it defeats this boundary.
    pub fn try_retire(&self, value: Retirement) -> Result<RetireAdmission> {
        let selected = match &value {
            Retirement::Ready(ready) => ready.prepared.0.lane,
            Retirement::Prepared(prepared) => prepared.0.lane,
            Retirement::Body { .. }
            | Retirement::Request(_)
            | Retirement::AuthenticationRequest(_)
            | Retirement::AuthenticatedBody(_)
            | Retirement::AuthenticatedRequest(_)
            | Retirement::AuthenticationTicket(_)
            | Retirement::Candidate(_)
            | Retirement::Batch(_) => 1,
            Retirement::Input(input) => match input {
                PrepareInput::New(message) => lane(message),
                PrepareInput::Cached(prepared) => prepared.0.lane,
                PrepareInput::StoredBody(_) => 1,
                PrepareInput::BindEarly { .. } => 1,
            },
            Retirement::Message(message) => lane(message),
        };
        let packet_bytes = match &value {
            Retirement::Body { candidate, .. } => Some(
                candidate
                    .as_ref()
                    .map_or(0, |candidate| candidate.packet().record_bytes()),
            ),
            Retirement::Candidate(candidate) => Some(candidate.packet().record_bytes()),
            // Public diagnostics can hold a different packet from the private
            // completion, so conservatively reserve both even if shared.
            Retirement::Batch(batch) => batch
                .packet
                .record_bytes()
                .checked_add(batch.candidate().packet().record_bytes()),
            Retirement::Input(PrepareInput::StoredBody(stored)) => Some(stored.record_bytes()),
            // Unaccepted BindEarly retains only its old prepared allocation;
            // retirement never builds a second body/reply. The base charge
            // therefore suffices even when a tight lane rejected preparation.
            Retirement::Input(PrepareInput::BindEarly { .. }) => Some(0),
            _ => Some(0),
        };
        let Some(charge) = packet_bytes.and_then(|bytes| self.charge.checked_add(bytes)) else {
            return Ok(RetireAdmission::Rejected {
                value,
                reason: "retirement reservation overflow".into(),
            });
        };
        if charge > budget(self.prepare_budget, selected).bytes {
            return Ok(RetireAdmission::Rejected {
                value,
                reason: "retirement exceeds lane byte ceiling".into(),
            });
        }
        if self.stop.load(Ordering::Acquire) {
            return Ok(RetireAdmission::Rejected {
                value,
                reason: "channel stopped".into(),
            });
        }
        let mut shared = match self.shared.try_lock() {
            Ok(shared) => shared,
            Err(TryLockError::WouldBlock) => return Ok(RetireAdmission::Backpressure(value)),
            Err(TryLockError::Poisoned(_)) => {
                return Ok(RetireAdmission::Rejected {
                    value,
                    reason: "channel queue poisoned".into(),
                })
            }
        };
        if !shared.retire_usage[selected].reserve(budget(self.prepare_budget, selected), charge) {
            return Ok(RetireAdmission::Backpressure(value));
        }
        shared.retire[selected].push_back(Charged {
            value,
            charge,
            created: Instant::now(),
        });
        drop(shared);
        self.wake();
        Ok(RetireAdmission::Accepted)
    }

    pub fn status(&self) -> Result<ChannelStatus> {
        let shared = self
            .shared
            .try_lock()
            .map_err(|_| anyhow::anyhow!("channel status busy or poisoned"))?;
        let mut status = shared.status.clone();
        status.stopped = self.stop.load(Ordering::Acquire)
            || self.worker.as_ref().is_some_and(JoinHandle::is_finished);
        status.pending_prepare = shared
            .prepare_usage
            .iter()
            .map(|usage| usage.messages)
            .sum();
        status.pending_send = shared.send_usage.iter().map(|usage| usage.messages).sum();
        status.pending_receive = shared
            .receive_usage
            .iter()
            .map(|usage| usage.messages)
            .sum();
        status.pending_retirement = shared.retire_usage.iter().map(|usage| usage.messages).sum();
        Ok(status)
    }

    fn wake(&self) {
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("host ingress owner panicked"))?;
        }
        Ok(())
    }
}

impl Drop for HostChannel {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn validate(config: &ChannelConfig) -> Result<(Reassembler, usize)> {
    ensure!(
        config.chain_id != 0
            && config.genesis != [0; 32]
            && config.protocol != [0; 32]
            && config.validators.chain_id() == config.chain_id
            && !config.ttl.is_zero(),
        "invalid channel fixed domain/TTL"
    );
    config.policy.validate()?;
    // Validate the codec without traversing any body.
    transport::encode(&Message::RequestBody { body_id: [1; 32] }, config.codec)?;
    ensure!(
        config.reassembly.max_message_bytes == config.codec.message_bytes,
        "codec/reassembly ceilings differ"
    );
    let charge = config
        .codec
        .message_bytes
        .checked_mul(4)
        .and_then(|bytes| bytes.checked_add(4096))
        .context("channel logical-content reservation overflow")?;
    for limits in [config.prepare, config.send, config.receive] {
        for budget in [limits.control, limits.body] {
            ensure!(
                (1..=4096).contains(&budget.messages) && budget.bytes >= charge,
                "channel lane cannot retain one maximum-size message"
            );
        }
    }
    for global in [config.send.control, config.send.body] {
        let peer = peer_budget(global, config.peers.len());
        ensure!(
            peer.messages > 0 && peer.bytes >= charge,
            "channel send lane cannot reserve one maximum-size message per peer"
        );
    }
    let assembler = Reassembler::new(
        transport::fragment_domain(config.chain_id, config.genesis, config.protocol),
        config.peers.clone(),
        config.reassembly.clone(),
    )?;
    Ok((assembler, charge))
}

fn fixed_context(context: &super::wire::Context, config: &ChannelConfig) -> Result<()> {
    context.validate(&config.validators)?;
    ensure!(
        context.genesis_config_commitment == config.genesis
            && context.protocol_commitment == config.protocol,
        "message differs from local genesis/protocol"
    );
    Ok(())
}

fn fixed_early(scope: &EarlyBodyScope, config: &ChannelConfig) -> Result<()> {
    scope.validate_shape()?;
    fixed_context(&scope.source, config)
}

/// Shape only: all nonzero parent roots, program pins and time fields remain
/// untrusted claims. The controller/compiler must independently authorize them.
fn fixed_binding(
    scope: &EarlyBodyScope,
    id: &Hash,
    context: &BatchContext,
    config: &ChannelConfig,
) -> Result<()> {
    fixed_early(scope, config)?;
    scope.validate_binding(id, context)?;
    ensure!(
        context.semantic_version != 0,
        "empty bound semantic version"
    );
    ensure!(
        (context.parent_height == 0) == (context.parent_block_hash == [0; 32]),
        "bound first-block parent hash convention mismatch"
    );
    for hash in [
        context.business_program,
        context.effect_contract,
        context.parent_state_root,
        context.parent_receipt_root,
        context.receipt_codec,
    ] {
        ensure!(hash != [0; 32], "empty bound domain/root commitment");
    }
    Ok(())
}

fn evidence(message: &Message, config: &ChannelConfig) -> Result<VerifiedEvidence> {
    match message {
        Message::Body { context, .. } => {
            ensure!(
                context.chain_id == config.chain_id
                    && context.genesis_config_commitment == config.genesis
                    && context.protocol_commitment == config.protocol,
                "body differs from local chain domain"
            );
            Ok(VerifiedEvidence::None)
        }
        Message::EarlyBody { scope, .. } => {
            fixed_early(scope, config)?;
            Ok(VerifiedEvidence::None)
        }
        Message::BindBody {
            scope,
            announcement_id,
            context,
        } => {
            fixed_binding(scope, announcement_id, context, config)?;
            Ok(VerifiedEvidence::None)
        }
        Message::Proposal {
            proposal,
            valid_quorum,
            ..
        } => {
            fixed_context(&proposal.context, config)?;
            let verified = proposal.verify(&config.validators)?;
            let valid = valid_quorum
                .as_ref()
                .map(|qc| qc.verify(&config.validators))
                .transpose()?;
            ensure!(
                match (&valid, proposal.valid_round) {
                    (None, None) => true,
                    (Some(qc), Some(round)) =>
                        qc.context() == &proposal.context
                            && qc.phase() == Phase::Prevote
                            && qc.round() == round
                            && qc.value() == Some(proposal.value),
                    _ => false,
                },
                "proposal valid-round certificate mismatch"
            );
            Ok(VerifiedEvidence::Proposal {
                proposal: verified,
                valid_quorum: valid,
            })
        }
        Message::Vote(vote) => {
            fixed_context(&vote.context, config)?;
            Ok(VerifiedEvidence::Vote(vote.verify(&config.validators)?))
        }
        Message::Decision {
            proposal,
            certificate,
            ..
        } => {
            fixed_context(&proposal.context, config)?;
            let proposal = proposal.verify(&config.validators)?;
            let qc = certificate.verify(&config.validators)?;
            ensure!(
                qc.context() == &proposal.proposal().context
                    && qc.round() == proposal.proposal().round
                    && qc.phase() == Phase::Precommit
                    && qc.value() == Some(proposal.proposal().value),
                "decision certificate mismatch"
            );
            Ok(VerifiedEvidence::Decision {
                proposal,
                certificate: qc,
            })
        }
        Message::RequestBody { .. } => Ok(VerifiedEvidence::None),
        Message::RequestDecision { context } => {
            fixed_context(context, config)?;
            Ok(VerifiedEvidence::None)
        }
    }
}

fn ready(prepared: PreparedMessage, config: &ChannelConfig) -> Result<Ready> {
    let message = prepared.0.message.clone();
    let body = match message.as_ref() {
        Message::Body {
            context,
            raw_transactions,
        } => Some(PreparedBody {
            id: prepared.0.body_id.context("prepared body lacks identity")?,
            context: *context,
            message: message.clone(),
            request: Some(BatchRequest::new(
                raw_transactions.clone(),
                *context,
                config.policy.clone(),
            )?),
        }),
        _ => None,
    };
    let early = match message.as_ref() {
        Message::EarlyBody {
            scope,
            raw_transactions,
        } => Some(PreparedEarlyBody {
            id: prepared
                .0
                .early_id
                .context("prepared early body lacks identity")?,
            scope: *scope,
            request: Some(AuthenticationRequest::new(
                raw_transactions.clone(),
                config.policy.clone(),
            )?),
        }),
        _ => None,
    };
    Ok(Ready {
        message,
        evidence: prepared.0.evidence.clone(),
        prepared,
        body,
        early,
        bound_early: None,
    })
}

fn prepare(
    message: Arc<Message>,
    encoded: Vec<u8>,
    config: &ChannelConfig,
    scope: &Arc<()>,
    charge: usize,
) -> Result<Ready> {
    let evidence = Arc::new(evidence(&message, config)?);
    let body_id = match message.as_ref() {
        Message::Body {
            context,
            raw_transactions,
        } => Some(transport::body_id(context, raw_transactions, config.codec)?),
        _ => None,
    };
    let early_id = match message.as_ref() {
        Message::EarlyBody {
            scope,
            raw_transactions,
        } => Some(transport::early_body_id(
            scope,
            raw_transactions,
            config.codec,
        )?),
        _ => None,
    };
    let selected = lane(&message);
    let encoded = OutgoingMessage::new(
        transport::fragment_domain(config.chain_id, config.genesis, config.protocol),
        encoded,
        config.codec.message_bytes,
    )?;
    ready(
        PreparedMessage(Arc::new(PreparedInner {
            scope: scope.clone(),
            message,
            encoded,
            evidence,
            body_id,
            early_id,
            charge,
            lane: selected,
        })),
        config,
    )
}

fn bind_early(
    early: PreparedMessage,
    context: BatchContext,
    config: &ChannelConfig,
    owner: &Arc<()>,
    charge: usize,
) -> Result<Ready> {
    ensure!(
        Arc::ptr_eq(&early.0.scope, owner),
        "early handle belongs to another channel"
    );
    let Message::EarlyBody {
        scope,
        raw_transactions,
    } = early.message().as_ref()
    else {
        anyhow::bail!("binding requires an early-body handle");
    };
    let id = early.early_id().context("early handle lacks identity")?;
    fixed_binding(scope, &id, &context, config)?;
    let message = Arc::new(Message::Body {
        context,
        raw_transactions: raw_transactions.clone(),
    });
    let encoded = transport::encode(&message, config.codec)?;
    let mut result = prepare(message, encoded, config, owner, charge)?;
    result.bound_early = Some((*scope, id));
    // Keep the original allocation live through the entire overlap above. Its
    // independent admission charge is held until the local reply is consumed.
    drop(early);
    Ok(result)
}

fn run(
    mut network: NetworkWorker,
    config: &ChannelConfig,
    mut assembler: Reassembler,
    charge: usize,
    scope: &Arc<()>,
    shared: &Mutex<Shared>,
    stop: &AtomicBool,
) -> Result<()> {
    let mut turn = 0usize;
    let result = (|| -> Result<()> {
        while !stop.load(Ordering::Acquire) {
            let mut progressed = false;
            // Control/body take alternating first turns; each job sends only
            // one carrier chunk, even if the network queue remains writable.
            for offset in 0..2 {
                let selected = (turn + offset) % 2;
                progressed |= retire_one(shared, selected)?;
                progressed |= expire_receive_one(shared, selected, config.ttl)?;
                progressed |= prepare_one(config, scope, charge, shared, selected)?;
                progressed |= send_one(&network, config, shared, selected)?;
            }
            turn = (turn + 1) % 2;
            for _ in 0..8 {
                let Some(inbound) = network.try_recv()? else {
                    break;
                };
                progressed = true;
                match assembler.push(&inbound.peer_id, &inbound.bytes, Instant::now()) {
                    Ok(FragmentAdmission::Accepted | FragmentAdmission::Duplicate) => {}
                    Ok(FragmentAdmission::Backpressure) => {
                        shared
                            .lock()
                            .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?
                            .status
                            .dropped_received += 1
                    }
                    Err(_) => {
                        shared
                            .lock()
                            .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?
                            .status
                            .invalid_received += 1
                    }
                }
            }
            match assembler.poll_complete(Instant::now(), 1) {
                Ok(Some(completed)) => {
                    progressed = true;
                    receive_one(completed, config, scope, charge, shared)?;
                }
                Ok(None) => {}
                Err(_) => {
                    shared
                        .lock()
                        .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?
                        .status
                        .invalid_received += 1
                }
            }
            if !progressed {
                thread::park_timeout(Duration::from_millis(1));
            }
        }
        Ok(())
    })();
    let shutdown = network.shutdown();
    result.and(shutdown)
}

fn prepare_one(
    config: &ChannelConfig,
    scope: &Arc<()>,
    charge: usize,
    shared: &Mutex<Shared>,
    selected: usize,
) -> Result<bool> {
    let job = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?
        .prepare[selected]
        .pop_front();
    let Some(job) = job else { return Ok(false) };
    let token = job.value.token;
    let mut encoded_new = false;
    let result = (|| -> Result<Ready> {
        ensure!(
            job.created.elapsed() < config.ttl,
            "local preparation expired"
        );
        let message = match job.value.input {
            PrepareInput::Cached(message) => return ready(message, config),
            PrepareInput::BindEarly { early, context } => {
                let ready = bind_early(early, context, config, scope, charge)?;
                encoded_new = true;
                return Ok(ready);
            }
            PrepareInput::New(message) => message,
            PrepareInput::StoredBody(stored) => {
                let transactions = stored.raw_transactions();
                ensure!(
                    transactions.len() <= config.codec.transactions,
                    "stored body transaction count exceeds channel budget"
                );
                let mut bytes = 0usize;
                for raw in transactions {
                    ensure!(
                        !raw.is_empty() && raw.len() <= config.codec.transaction_bytes,
                        "stored transaction exceeds channel budget"
                    );
                    bytes = bytes
                        .checked_add(raw.len())
                        .context("stored body size overflow")?;
                    ensure!(
                        bytes <= config.codec.body_bytes,
                        "stored body exceeds channel budget"
                    );
                }
                Arc::new(Message::Body {
                    context: *stored.context(),
                    raw_transactions: transactions.to_vec(),
                })
            }
        };
        let encoded = transport::encode(&message, config.codec)?;
        let ready = prepare(message, encoded, config, scope, charge)?;
        encoded_new = true;
        Ok(ready)
    })()
    .map_err(short_error);
    let mut shared = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?;
    if encoded_new {
        shared.status.encoded_messages += 1;
    }
    if result.as_ref().is_ok_and(|ready| ready.body.is_some()) {
        shared.status.prepared_bodies += 1;
    }
    if result.is_err() {
        shared.status.preparation_failures += 1;
    }
    shared.local[selected].push_back(Charged {
        value: LocalReply { token, result },
        charge: job.charge,
        created: job.created,
    });
    Ok(true)
}

fn send_one(
    network: &NetworkWorker,
    config: &ChannelConfig,
    shared: &Mutex<Shared>,
    selected: usize,
) -> Result<bool> {
    let job = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?
        .send[selected]
        .pop_front();
    let Some(mut job) = job else { return Ok(false) };
    if job.created.elapsed() >= config.ttl {
        let mut shared = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?;
        release_send(&mut shared, selected, &job.value.outbound.peer, job.charge);
        shared.status.expired_sends += 1;
        return Ok(true);
    }
    let frame = match job.value.pending_frame.take() {
        Some(frame) => frame,
        None => job.value.outbound.message.0.encoded.frame(job.value.next)?,
    };
    let result = network.try_send(job.value.outbound.peer.clone(), frame)?;
    let mut shared = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?;
    match result {
        NetworkAdmission::Accepted => {
            job.value.next += 1;
            shared.status.sent_frames += 1;
            if job.value.next == job.value.outbound.message.0.encoded.frame_count() {
                release_send(&mut shared, selected, &job.value.outbound.peer, job.charge);
                shared.status.sent_messages += 1;
            } else {
                shared.send[selected].push_back(job);
            }
            Ok(true)
        }
        NetworkAdmission::Backpressure(returned) => {
            job.value.pending_frame = Some(returned.bytes);
            shared.send[selected].push_back(job);
            Ok(false)
        }
        NetworkAdmission::Rejected { .. } => {
            release_send(&mut shared, selected, &job.value.outbound.peer, job.charge);
            shared.status.rejected_sends += 1;
            Ok(true)
        }
    }
}

fn release_send(shared: &mut Shared, selected: usize, peer: &str, charge: usize) {
    shared.send_usage[selected].release(charge);
    shared.send_peer_usage[selected]
        .get_mut(peer)
        .expect("queued send peer reserved")
        .release(charge);
}

fn expire_receive_one(shared: &Mutex<Shared>, selected: usize, ttl: Duration) -> Result<bool> {
    let expired = {
        let mut shared = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?;
        if shared.receive[selected]
            .front()
            .is_some_and(|item| item.created.elapsed() >= ttl)
        {
            let item = shared.receive[selected]
                .pop_front()
                .expect("checked expired front");
            shared.receive_usage[selected].release(item.charge);
            shared.status.dropped_received += 1;
            Some(item)
        } else {
            None
        }
    };
    // Potentially large request/message deallocation belongs to this owner,
    // outside the admission lock, not to the coordinator's try_recv call.
    Ok(expired.is_some())
}

fn retire_one(shared: &Mutex<Shared>, selected: usize) -> Result<bool> {
    let job = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?
        .retire[selected]
        .pop_front();
    let Some(job) = job else { return Ok(false) };
    let charge = job.charge;
    // Last-Arc and raw transaction destruction can be proportional to a body.
    // Retain its reservation until destruction ends, and never hold the lock.
    drop(job);
    let mut shared = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?;
    shared.retire_usage[selected].release(charge);
    shared.status.retired_messages += 1;
    Ok(true)
}

fn receive_one(
    completed: novovm_network::fragments::CompletedMessage,
    config: &ChannelConfig,
    scope: &Arc<()>,
    charge: usize,
    shared: &Mutex<Shared>,
) -> Result<()> {
    let selected = match transport::body_prefix(completed.chunks.first().map_or(&[], Vec::as_slice))
    {
        Ok(is_body) => usize::from(is_body),
        Err(_) => {
            shared
                .lock()
                .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?
                .status
                .invalid_received += 1;
            return Ok(());
        }
    };
    {
        let mut shared = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?;
        if !shared.receive_usage[selected].reserve(budget(config.receive, selected), charge) {
            shared.status.dropped_received += 1;
            return Ok(());
        }
    }
    // Fragment storage is already bounded by the assembler. Reserve the
    // receive lane BEFORE flattening, decoding, hashing or cloning raw input.
    let result = (|| -> Result<Ready> {
        let bytes = completed.chunks.concat();
        drop(completed.chunks);
        let decoded = transport::decode(&bytes, config.codec)?;
        ensure!(lane(&decoded) == selected, "host message lane mismatch");
        prepare(Arc::new(decoded), bytes, config, scope, charge)
    })();
    let mut shared = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("channel queue poisoned"))?;
    match result {
        Ok(ready) => {
            shared.status.received_messages += 1;
            shared.status.encoded_messages += 1;
            if ready.body.is_some() {
                shared.status.prepared_bodies += 1;
            }
            shared.receive[selected].push_back(Charged {
                value: Received {
                    peer: completed.peer,
                    ready,
                },
                charge,
                created: Instant::now(),
            });
        }
        Err(_) => {
            shared.receive_usage[selected].release(charge);
            shared.status.invalid_received += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod early_tests;
