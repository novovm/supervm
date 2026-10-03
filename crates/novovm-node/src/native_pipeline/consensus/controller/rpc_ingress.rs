//! Best-effort bounded input propagation through the existing channel owner.
//! No input, queue admission, or transport ACK is execution/signing authority.
//! Receiver pool validation remains mandatory; no received input is re-gossiped.
use super::super::channel::QueueBudget;
use super::super::transport::TransactionsScope;
use super::*;
use serde::Serialize;

const REPLAY_HINTS_PER_PEER: usize = 64;

#[derive(Clone, Debug, Default, Serialize)]
pub struct TransactionsStats {
    /// Local channel queue admission only, not remote delivery/pool/durability.
    pub outbound_batches_accepted: u64,
    pub outbound_peer_enqueued: u64,
    /// Remote bounded consumer took the complete bytes; no validation implied.
    pub peer_credits_returned: u64,
    pub rejected_credits: u64,
    pub taken_credits_queued: u64,
    pub taken_credits_enqueued: u64,
    /// Authenticated network source/domain only, NOT transaction signatures.
    pub inbound_batches: u64,
    pub inbound_dropped: u64,
    pub duplicate_batches: u64,
    pub conflicting_sequences: u64,
    pub expired_batches: u64,
    pub preparation_failures: u64,
    pub channel_expired_sends: u64,
    pub channel_dropped_received: u64,
    pub outbound_pending: usize,
    pub inbound_pending: usize,
    pub credit_pending: usize,
}

pub struct ReceivedTransactions {
    pub peer: String,
    /// Only Transactions is returned. Caller retains and validates each raw.
    pub message: Arc<Message>,
}

struct PendingSend {
    token: u64,
    prepared: Option<PreparedMessage>,
    created: Instant,
    next_peer: usize,
    pending: Vec<bool>,
    queued: Vec<bool>,
}

struct PendingCredit {
    peer: String,
    input: Option<Arc<Message>>,
    token: Option<u64>,
    prepared: Option<PreparedMessage>,
    created: Instant,
}

struct PendingReceive {
    ready: Ready,
    created: Instant,
}

struct ReplayHint {
    session: Hash,
    sequence: u64,
    message: Hash,
    created: Instant,
}

pub(super) struct Transactions {
    session: Hash,
    sequence: u64,
    peers: Vec<String>,
    outbox: VecDeque<PendingSend>,
    credits: VecDeque<PendingCredit>,
    inbox: BTreeMap<String, VecDeque<PendingReceive>>,
    replay: BTreeMap<String, VecDeque<ReplayHint>>,
    receive_turn: usize,
    budget: QueueBudget,
    charge: usize,
    ttl: Duration,
    stats: TransactionsStats,
}

impl Transactions {
    pub(super) fn new(channel: &HostChannel, peers: Vec<String>) -> Self {
        let (budget, ttl) = channel.transactions_budget();
        Self {
            session: rand::random(),
            sequence: 0,
            peers,
            outbox: VecDeque::new(),
            credits: VecDeque::new(),
            inbox: BTreeMap::new(),
            replay: BTreeMap::new(),
            receive_turn: 0,
            budget,
            charge: channel.preparation_charge(),
            ttl,
            stats: TransactionsStats::default(),
        }
    }

    fn capacity(&self) -> usize {
        self.budget.messages.min(self.budget.bytes / self.charge)
    }
    fn peer_capacity(&self) -> usize {
        self.capacity() / self.peers.len().max(1)
    }
    pub(super) fn has_peer_capacity(&self) -> bool {
        self.peer_capacity() > 0
    }
    fn sync_usage(&mut self) {
        self.stats.outbound_pending = self.outbox.len();
        self.stats.inbound_pending = self.inbox.values().map(VecDeque::len).sum();
        self.stats.credit_pending = self.credits.len();
    }
    fn check_replay(
        &mut self,
        peer: &str,
        scope: &TransactionsScope,
        message: Hash,
        now: Instant,
    ) -> Result<bool> {
        ensure!(
            self.peers.iter().any(|configured| configured == peer),
            "input gossip requires configured remote source"
        );
        let hints = self.replay.entry(peer.to_owned()).or_default();
        while hints
            .front()
            .is_some_and(|hint| now.duration_since(hint.created) >= self.ttl)
        {
            hints.pop_front();
        }
        if let Some(hint) = hints
            .iter()
            .find(|hint| hint.session == scope.session && hint.sequence == scope.sequence)
        {
            if hint.message == message {
                self.stats.duplicate_batches += 1;
            } else {
                self.stats.conflicting_sequences += 1;
            }
            return Ok(false);
        }
        Ok(true)
    }
    fn remember(&mut self, peer: String, scope: &TransactionsScope, message: Hash, now: Instant) {
        let hints = self.replay.entry(peer).or_default();
        hints.push_back(ReplayHint {
            session: scope.session,
            sequence: scope.sequence,
            message,
            created: now,
        });
        while hints.len() > REPLAY_HINTS_PER_PEER {
            hints.pop_front();
        }
    }
    fn return_credit(
        &mut self,
        peer: &str,
        scope: &TransactionsScope,
        fragment_id: Hash,
        now: Instant,
    ) -> Result<bool> {
        let index = self
            .peers
            .iter()
            .position(|expected| expected == peer)
            .context("input credit from unconfigured peer")?;
        if scope.session != self.session {
            return Ok(false);
        }
        let Some(pending) = self.outbox.iter_mut().find(|pending| pending.prepared.as_ref().is_some_and(|prepared| {
            prepared.fragment_id() == fragment_id && matches!(prepared.message().as_ref(), Message::Transactions { scope: original, .. } if original == scope)
        })) else { return Ok(false); };
        if now.duration_since(pending.created) >= self.ttl
            || !pending.queued[index]
            || !pending.pending[index]
        {
            return Ok(false);
        }
        pending.pending[index] = false;
        Ok(true)
    }
    fn earlier_pending(&self, token: u64, peer: usize) -> bool {
        self.outbox
            .iter()
            .any(|other| other.token < token && other.pending[peer])
    }
}

fn check_scope(scope: &TransactionsScope, context: Context) -> Result<()> {
    scope.validate_shape()?;
    ensure!(
        scope.chain_id == context.chain_id
            && scope.genesis == context.genesis_config_commitment
            && scope.protocol == context.protocol_commitment
            && scope.epoch == context.epoch
            && scope.validator_set_hash == context.validator_set_hash,
        "input gossip domain mismatch"
    );
    Ok(())
}

impl Controller {
    /// Construct from already pool-admitted bytes; this function does NOT do
    /// authentication and therefore cannot be used as pool/signing admission.
    pub fn transactions_message(&mut self, raw_transactions: Vec<Vec<u8>>) -> Result<Arc<Message>> {
        self.transactions.sequence = self
            .transactions
            .sequence
            .checked_add(1)
            .context("input gossip sequence exhausted")?;
        let context = self.context();
        Ok(Arc::new(Message::Transactions {
            scope: TransactionsScope {
                chain_id: context.chain_id,
                genesis: context.genesis_config_commitment,
                protocol: context.protocol_commitment,
                epoch: context.epoch,
                validator_set_hash: context.validator_set_hash,
                session: self.transactions.session,
                sequence: self.transactions.sequence,
            },
            raw_transactions,
        }))
    }

    pub fn transactions_stats(&self) -> &TransactionsStats {
        &self.transactions.stats
    }

    /// Accepted means a bounded LOCAL preparation slot, never remote or durable
    /// receipt. Backpressure leaves the caller's original Arc untouched.
    pub fn try_submit_transactions(&mut self, message: &Arc<Message>) -> Result<bool> {
        let Message::Transactions { scope, .. } = message.as_ref() else {
            anyhow::bail!("expected raw input gossip");
        };
        self.check_transactions_scope(scope)?;
        ensure!(
            scope.session == self.transactions.session
                && scope.sequence <= self.transactions.sequence,
            "input gossip is not from this controller process"
        );
        if self.is_recovering() || self.transactions.outbox.len() >= self.transactions.capacity() {
            return Ok(false);
        }
        let token = self.allocate_token()?;
        match self.channel.try_prepare(PrepareRequest {
            token,
            input: PrepareInput::New(message.clone()),
        })? {
            PrepareAdmission::Accepted => {
                self.transactions.outbox.push_back(PendingSend {
                    token,
                    prepared: None,
                    created: Instant::now(),
                    next_peer: 0,
                    pending: vec![true; self.transactions.peers.len()],
                    queued: vec![false; self.transactions.peers.len()],
                });
                self.transactions.stats.outbound_batches_accepted += 1;
                self.transactions.sync_usage();
                Ok(true)
            }
            PrepareAdmission::Backpressure(_) => Ok(false),
            PrepareAdmission::Rejected { reason, .. } => anyhow::bail!(reason),
        }
    }

    /// One bounded peer turn. The application must keep the Arc while draining
    /// a limited number of strict authentications per poll. This returns only
    /// flow credit, not authenticated/pool/persistent receipt of the inputs.
    pub fn take_transactions(&mut self) -> Option<ReceivedTransactions> {
        let peers = self.transactions.peers.len();
        for offset in 0..peers {
            let index = (self.transactions.receive_turn + offset) % peers;
            let peer = self.transactions.peers[index].clone();
            // At most one unsent credit per peer, independent of fixed BFT
            // retransmissions. Do not lose credit by consuming another batch.
            if self
                .transactions
                .credits
                .iter()
                .any(|credit| credit.peer == peer)
            {
                continue;
            }
            if let Some(input) = self
                .transactions
                .inbox
                .get_mut(&peer)
                .and_then(VecDeque::pop_front)
            {
                self.transactions.receive_turn = (index + 1) % peers;
                self.transactions.sync_usage();
                if input.created.elapsed() >= self.transactions.ttl {
                    self.transactions.stats.expired_batches += 1;
                    self.retire(Retirement::Ready(input.ready));
                    return None;
                }
                let Message::Transactions { scope, .. } = input.ready.message.as_ref() else {
                    unreachable!()
                };
                self.transactions.credits.push_back(PendingCredit {
                    peer: peer.clone(),
                    input: Some(Arc::new(Message::TransactionsTaken {
                        scope: *scope,
                        fragment_id: input.ready.prepared.fragment_id(),
                    })),
                    token: None,
                    prepared: None,
                    created: Instant::now(),
                });
                self.transactions.stats.taken_credits_queued += 1;
                self.transactions.sync_usage();
                let message = input.ready.message.clone();
                self.retire(Retirement::Ready(input.ready));
                return Some(ReceivedTransactions { peer, message });
            }
        }
        None
    }

    fn check_transactions_scope(&self, scope: &TransactionsScope) -> Result<()> {
        check_scope(scope, self.context())
    }

    pub(super) fn transactions_prepared(&self, token: u64) -> bool {
        self.transactions
            .outbox
            .iter()
            .any(|pending| pending.token == token)
            || self
                .transactions
                .credits
                .iter()
                .any(|credit| credit.token == Some(token))
    }

    pub(super) fn finish_transactions_prepared(
        &mut self,
        token: u64,
        result: std::result::Result<Ready, String>,
    ) -> Result<()> {
        if let Some(index) = self
            .transactions
            .credits
            .iter()
            .position(|credit| credit.token == Some(token))
        {
            match result {
                Ok(ready) => {
                    self.transactions.credits[index].prepared = Some(ready.prepared.clone());
                    self.retire(Retirement::Ready(ready));
                }
                Err(error) => {
                    self.transactions.credits.remove(index);
                    self.transactions.stats.preparation_failures += 1;
                    self.reject(error);
                }
            }
            self.transactions.sync_usage();
            return Ok(());
        }
        let index = self
            .transactions
            .outbox
            .iter()
            .position(|pending| pending.token == token)
            .expect("known input preparation");
        match result {
            Ok(ready) => {
                self.transactions.outbox[index].prepared = Some(ready.prepared.clone());
                self.retire(Retirement::Ready(ready));
            }
            Err(error) => {
                self.transactions.outbox.remove(index);
                self.transactions.stats.preparation_failures += 1;
                self.reject(error);
            }
        }
        self.transactions.sync_usage();
        Ok(())
    }

    pub(super) fn receive_transactions_taken(&mut self, peer: &str, ready: &Ready) -> Result<()> {
        let Message::TransactionsTaken { scope, fragment_id } = ready.message.as_ref() else {
            unreachable!()
        };
        let accepted = (|| -> Result<bool> {
            self.check_transactions_scope(scope)?;
            self.transactions
                .return_credit(peer, scope, *fragment_id, Instant::now())
        })();
        match accepted {
            Ok(true) => self.transactions.stats.peer_credits_returned += 1,
            other => {
                self.transactions.stats.rejected_credits += 1;
                if let Err(error) = other {
                    self.reject(error);
                }
            }
        }
        Ok(())
    }

    fn poll_transactions_credits(&mut self) -> Result<()> {
        for _ in 0..self
            .config
            .limits
            .sends_per_poll
            .min(self.transactions.credits.len())
        {
            let mut credit = self
                .transactions
                .credits
                .pop_front()
                .expect("bounded credit");
            // Keep owner-accepted preparations until their bounded reply is
            // received so an expired request cannot become an unknown token.
            if credit.created.elapsed() >= self.transactions.ttl
                && (credit.token.is_none() || credit.prepared.is_some())
            {
                self.transactions.stats.expired_batches += 1;
                if let Some(input) = credit.input {
                    self.retire(Retirement::Message(input));
                }
                if let Some(prepared) = credit.prepared {
                    self.retire(Retirement::Prepared(prepared));
                }
                continue;
            }
            if let Some(prepared) = &credit.prepared {
                match self.channel.try_send(Outbound {
                    peer: credit.peer.clone(),
                    message: prepared.clone(),
                })? {
                    SendAdmission::Accepted => {
                        self.transactions.stats.taken_credits_enqueued += 1;
                        self.retire(Retirement::Prepared(credit.prepared.take().unwrap()));
                        continue;
                    }
                    SendAdmission::Backpressure(_) => {}
                    SendAdmission::Rejected { reason, .. } => {
                        self.reject(reason);
                        self.retire(Retirement::Prepared(credit.prepared.take().unwrap()));
                        continue;
                    }
                }
            } else if let Some(input) = credit.input.take() {
                let token = self.allocate_token()?;
                match self.channel.try_prepare(PrepareRequest {
                    token,
                    input: PrepareInput::New(input),
                })? {
                    PrepareAdmission::Accepted => credit.token = Some(token),
                    PrepareAdmission::Backpressure(request) => {
                        let PrepareInput::New(input) = request.input else {
                            unreachable!()
                        };
                        credit.input = Some(input);
                    }
                    PrepareAdmission::Rejected { request, reason } => {
                        self.reject(reason);
                        self.retire(Retirement::Input(request.input));
                        continue;
                    }
                }
            }
            self.transactions.credits.push_back(credit);
        }
        Ok(())
    }

    pub(super) fn receive_transactions(&mut self, peer: String, ready: Ready) -> Result<()> {
        let Message::Transactions { scope, .. } = ready.message.as_ref() else {
            unreachable!()
        };
        let now = Instant::now();
        let accepted = (|| -> Result<bool> {
            self.check_transactions_scope(scope)?;
            if !self
                .transactions
                .check_replay(&peer, scope, ready.prepared.fragment_id(), now)?
            {
                return Ok(false);
            }
            ensure!(
                self.transactions.inbox.get(&peer).map_or(0, VecDeque::len)
                    < self.transactions.peer_capacity(),
                "input gossip peer queue full"
            );
            self.transactions
                .remember(peer.clone(), scope, ready.prepared.fragment_id(), now);
            Ok(true)
        })();
        match accepted {
            Ok(true) => {
                self.transactions
                    .inbox
                    .entry(peer)
                    .or_default()
                    .push_back(PendingReceive {
                        ready,
                        created: now,
                    });
                self.transactions.stats.inbound_batches += 1;
            }
            other => {
                self.transactions.stats.inbound_dropped += 1;
                if let Err(error) = other {
                    self.reject(error);
                }
                self.retire(Retirement::Ready(ready));
            }
        }
        self.transactions.sync_usage();
        if let Ok(status) = self.channel.status() {
            self.transactions.stats.channel_expired_sends = status.ingress_expired_sends;
            self.transactions.stats.channel_dropped_received = status.ingress_dropped_received;
        }
        Ok(())
    }

    pub(super) fn poll_transactions(&mut self, _now: Instant) -> Result<()> {
        self.poll_transactions_credits()?;
        // A separate finite queue, never the consensus retransmission cache.
        // Each poll visits at most sends_per_poll entries and only one peer per
        // entry. A slow/offline peer cannot pin the cursor on healthy peers.
        for _ in 0..self
            .config
            .limits
            .sends_per_poll
            .min(self.transactions.outbox.len())
        {
            let mut pending = self
                .transactions
                .outbox
                .pop_front()
                .expect("bounded input send");
            let Some(prepared) = pending.prepared.as_ref() else {
                self.transactions.outbox.push_back(pending);
                continue;
            };
            if pending.created.elapsed() >= self.transactions.ttl {
                self.transactions.stats.expired_batches += 1;
                self.retire(Retirement::Prepared(pending.prepared.take().unwrap()));
                continue;
            }
            let peers = self.transactions.peers.len();
            if peers != 0 {
                let index = pending.next_peer;
                let earlier = self.transactions.earlier_pending(pending.token, index);
                // One unconsumed message per peer. Slow input validation on
                // one node never consumes another peer's window or consensus.
                if pending.pending[index] && !pending.queued[index] && !earlier {
                    match self.channel.try_send(Outbound {
                        peer: self.transactions.peers[index].clone(),
                        message: prepared.clone(),
                    })? {
                        SendAdmission::Accepted => {
                            pending.queued[index] = true;
                            self.transactions.stats.outbound_peer_enqueued += 1;
                        }
                        SendAdmission::Backpressure(_) => {}
                        SendAdmission::Rejected { reason, .. } => {
                            pending.pending[index] = false;
                            self.reject(reason);
                        }
                    }
                }
                pending.next_peer = (index + 1) % peers;
            }
            if pending.pending.iter().any(|value| *value) {
                self.transactions.outbox.push_back(pending);
            } else {
                self.retire(Retirement::Prepared(pending.prepared.take().unwrap()));
            }
        }
        self.transactions.sync_usage();
        if let Ok(status) = self.channel.status() {
            self.transactions.stats.channel_expired_sends = status.ingress_expired_sends;
            self.transactions.stats.channel_dropped_received = status.ingress_dropped_received;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
