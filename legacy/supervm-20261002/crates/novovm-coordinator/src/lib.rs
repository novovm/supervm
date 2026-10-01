#![forbid(unsafe_code)]

use anyhow::{bail, Result};
use novovm_protocol::{ShardId, TwoPcMessage, TxEnvelope, TxId};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordinatorConfig {
    /// When true, all participants must vote yes to commit.
    pub require_unanimous: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            require_unanimous: true,
        }
    }
}

#[derive(Debug, Clone)]
struct PendingTx {
    tx: TxEnvelope,
    participants: HashSet<ShardId>,
    votes: HashMap<ShardId, bool>,
    decided: Option<bool>,
}

/// Minimal 2PC coordinator state machine used by migration phase F-06.
///
/// Scope:
/// - Tracks transaction participants and votes
/// - Emits protocol-level 2PC messages
/// - Produces deterministic decide(commit/abort)
///
/// Non-scope (for later phases):
/// - network transport/retry
/// - persistence/recovery
/// - timeout-driven reconfiguration
#[derive(Debug, Default)]
pub struct Coordinator {
    cfg: CoordinatorConfig,
    pending: HashMap<TxId, PendingTx>,
}

impl Coordinator {
    #[must_use]
    pub fn new(cfg: CoordinatorConfig) -> Self {
        Self {
            cfg,
            pending: HashMap::new(),
        }
    }

    pub fn begin_2pc(&mut self, tx: TxEnvelope) -> Result<TwoPcMessage> {
        if tx.target_shards.is_empty() {
            bail!("2pc tx must include at least one target shard");
        }
        if self.pending.contains_key(&tx.tx_id) {
            bail!("tx already exists in coordinator: {}", tx.tx_id.0);
        }
        let participants = tx.target_shards.iter().copied().collect::<HashSet<_>>();
        self.pending.insert(
            tx.tx_id,
            PendingTx {
                tx: tx.clone(),
                participants,
                votes: HashMap::new(),
                decided: None,
            },
        );
        Ok(TwoPcMessage::Propose { tx })
    }

    pub fn prepare(&self, tx_id: TxId) -> Result<TwoPcMessage> {
        if !self.pending.contains_key(&tx_id) {
            bail!("prepare for unknown tx: {}", tx_id.0);
        }
        Ok(TwoPcMessage::Prepare { tx_id })
    }

    pub fn record_vote(
        &mut self,
        tx_id: TxId,
        shard: ShardId,
        yes: bool,
    ) -> Result<Option<TwoPcMessage>> {
        let pending = self
            .pending
            .get_mut(&tx_id)
            .ok_or_else(|| anyhow::anyhow!("vote for unknown tx: {}", tx_id.0))?;
        if pending.decided.is_some() {
            bail!("tx already decided: {}", tx_id.0);
        }
        if !pending.participants.contains(&shard) {
            bail!(
                "shard {} is not a participant of tx {}",
                shard.0,
                pending.tx.tx_id.0
            );
        }

        pending.votes.insert(shard, yes);
        if pending.votes.len() < pending.participants.len() {
            return Ok(None);
        }

        let yes_count = pending.votes.values().filter(|v| **v).count();
        let commit = if self.cfg.require_unanimous {
            yes_count == pending.participants.len()
        } else {
            yes_count * 2 > pending.participants.len()
        };
        pending.decided = Some(commit);
        Ok(Some(TwoPcMessage::Decide { tx_id, commit }))
    }

    #[must_use]
    pub fn decision(&self, tx_id: TxId) -> Option<bool> {
        self.pending.get(&tx_id).and_then(|p| p.decided)
    }

    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_protocol::{NodeId, OperationClass};

    fn sample_tx(tx_id: u64, shards: &[u32]) -> TxEnvelope {
        TxEnvelope {
            tx_id: TxId(tx_id),
            from: NodeId(1),
            target_shards: shards.iter().map(|v| ShardId(*v)).collect(),
            op_class: OperationClass::TypeC,
            payload: vec![1, 2, 3],
        }
    }

    #[test]
    fn commit_when_all_votes_yes() {
        let mut c = Coordinator::new(CoordinatorConfig::default());
        let tx = sample_tx(42, &[1, 2, 3]);
        c.begin_2pc(tx).unwrap();
        c.prepare(TxId(42)).unwrap();

        assert!(c.record_vote(TxId(42), ShardId(1), true).unwrap().is_none());
        assert!(c.record_vote(TxId(42), ShardId(2), true).unwrap().is_none());
        let out = c
            .record_vote(TxId(42), ShardId(3), true)
            .unwrap()
            .expect("final vote should emit decide");

        match out {
            TwoPcMessage::Decide { tx_id, commit } => {
                assert_eq!(tx_id, TxId(42));
                assert!(commit);
            }
            _ => panic!("unexpected message"),
        }
        assert_eq!(c.decision(TxId(42)), Some(true));
    }

    #[test]
    fn abort_on_any_no_under_unanimous_mode() {
        let mut c = Coordinator::new(CoordinatorConfig::default());
        c.begin_2pc(sample_tx(7, &[1, 2])).unwrap();
        c.prepare(TxId(7)).unwrap();
        assert!(c.record_vote(TxId(7), ShardId(1), true).unwrap().is_none());
        let out = c
            .record_vote(TxId(7), ShardId(2), false)
            .unwrap()
            .expect("final vote should emit decide");
        match out {
            TwoPcMessage::Decide { tx_id, commit } => {
                assert_eq!(tx_id, TxId(7));
                assert!(!commit);
            }
            _ => panic!("unexpected message"),
        }
        assert_eq!(c.decision(TxId(7)), Some(false));
    }

    #[test]
    fn reject_non_participant_vote() {
        let mut c = Coordinator::new(CoordinatorConfig::default());
        c.begin_2pc(sample_tx(9, &[1, 2])).unwrap();
        let err = c
            .record_vote(TxId(9), ShardId(99), true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a participant"));
    }
}
