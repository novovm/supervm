//! Explicit cold startup, never called to advance an ordinary block. Verify
//! the pinned genesis-to-head prefix, then reconcile the ONE global signer log.
use super::*;
use crate::consensus::chain::{ChainRecovery, RecoveredChain};
use crate::persistence::metadata::MetadataSnapshot;

pub struct JournalOpening {
    identity: Option<Identity>,
    initial: Option<IoTicket<MetadataSnapshot>>,
    loaded: Option<Option<Vec<u8>>>,
    chain: Option<ChainRecovery>,
    recovered_chain: Option<RecoveredChain>,
    recovered: Option<(Snapshot, Vec<u8>)>,
    last: Option<IoTicket<MetadataSnapshot>>,
    failed: bool,
}

impl JournalOpening {
    pub(super) fn new(identity: Identity) -> Self {
        Self {
            identity: Some(identity),
            initial: None,
            loaded: None,
            chain: None,
            recovered_chain: None,
            recovered: None,
            last: None,
            failed: false,
        }
    }

    pub fn poll(&mut self, pipeline: &CandidatePipeline) -> Result<Option<ValidatorJournal>> {
        ensure!(
            !self.failed,
            "signing journal startup failed; explicit recovery required"
        );
        let result = self.poll_inner(pipeline);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn poll_inner(&mut self, pipeline: &CandidatePipeline) -> Result<Option<ValidatorJournal>> {
        let identity = self
            .identity
            .as_mut()
            .context("journal startup already consumed")?;
        identity.check_owner(pipeline)?;
        if self.loaded.is_none() {
            if self.initial.is_none() {
                self.initial = pipeline.try_read_consensus_metadata(vec![
                    MetaKey::ConsensusState(identity.validator),
                    MetaKey::ConsensusOutbox {
                        validator: identity.validator,
                        sequence: 1,
                    },
                    MetaKey::ChainHead,
                ])?;
            }
            let Some(mut reply) = take(&mut self.initial)? else {
                return Ok(None);
            };
            ensure!(
                reply.values.len() == 3,
                "journal startup reply count mismatch"
            );
            let head = reply.values.pop().unwrap();
            let first = reply.values.pop().unwrap();
            let saved = reply.values.pop().unwrap();
            ensure!(
                saved.is_some() == first.is_some(),
                "signing snapshot/initial outbox disappeared; refuse reset"
            );
            ensure!(
                head.is_none() || saved.is_some(),
                "chain exists without local signer snapshot"
            );
            self.chain = Some(ChainRecovery::new(
                identity.context,
                identity.parent,
                identity.set.clone(),
                head,
            )?);
            self.loaded = Some(saved);
            return Ok(None);
        }
        if self.recovered_chain.is_none() {
            let Some(chain) = self
                .chain
                .as_mut()
                .context("missing chain recovery")?
                .poll(pipeline)?
            else {
                return Ok(None);
            };
            self.recovered_chain = Some(chain);
            self.chain.take();
            return Ok(None);
        }
        let chain = self.recovered_chain.as_ref().unwrap();
        let saved = self.loaded.as_ref().unwrap();
        let Some(saved) = saved else {
            ensure!(
                chain.record.is_none(),
                "cannot create signer over existing decided chain"
            );
            let snapshot = Snapshot {
                state: State::new(identity.context)?,
                revision: 0,
                proposed: None,
                witness: None,
            };
            return Ok(Some(ValidatorJournal {
                identity: self.identity.take().unwrap(),
                snapshot,
                saved: None,
                last_message: None,
                pending: None,
                frozen: false,
                head_record: None,
                head_bytes: None,
            }));
        };
        if self.recovered.is_none() {
            // A valid log is either the decided head itself, or its next height.
            // No arbitrary-height decoding or silently resetting a stale log.
            let snapshot = if let Some(record) = &chain.record {
                identity.context = record.context();
                identity.parent = record.parent();
                if let Ok(snapshot) = codec::decode_snapshot(identity, saved) {
                    ensure!(
                        snapshot.state.decided() == Some(record.point().block_hash),
                        "head-height signer snapshot lacks its decision"
                    );
                    snapshot
                } else {
                    identity.parent = record.point();
                    identity.context = successor_context(record.context(), record.point())?;
                    let snapshot = codec::decode_snapshot(identity, saved)?;
                    ensure!(
                        snapshot.state.decided().is_none(),
                        "decided signer is ahead of chain head"
                    );
                    snapshot
                }
            } else {
                let snapshot = codec::decode_snapshot(identity, saved)?;
                ensure!(
                    snapshot.state.decided().is_none(),
                    "decided signing log has no atomic chain head; explicit recovery required"
                );
                snapshot
            };
            self.recovered = Some((snapshot, saved.clone()));
            return Ok(None);
        }
        let (snapshot, saved) = self.recovered.as_ref().unwrap();
        if self.last.is_none() {
            self.last = pipeline.try_read_consensus_metadata(vec![
                MetaKey::ConsensusOutbox {
                    validator: identity.validator,
                    sequence: snapshot.revision,
                },
                MetaKey::ConsensusState(identity.validator),
                MetaKey::ChainHead,
            ])?;
        }
        let Some(reply) = take(&mut self.last)? else {
            return Ok(None);
        };
        ensure!(
            reply.values.len() == 3
                && reply.values[1].as_ref() == Some(saved)
                && reply.values[2] == chain.head_bytes,
            "signing state/head changed during startup"
        );
        let message = codec::decode_outbox(
            identity,
            snapshot,
            saved,
            reply.values[0]
                .as_deref()
                .context("durable signing outbox missing")?,
        )?;
        let (snapshot, saved) = self.recovered.take().unwrap();
        let chain = self.recovered_chain.take().unwrap();
        Ok(Some(ValidatorJournal {
            identity: self.identity.take().unwrap(),
            snapshot,
            saved: Some(saved),
            last_message: message,
            pending: None,
            frozen: false,
            head_record: chain.record,
            head_bytes: chain.head_bytes,
        }))
    }
}

fn take<T>(ticket: &mut Option<IoTicket<T>>) -> Result<Option<T>> {
    ticket
        .as_mut()
        .map(IoTicket::try_take)
        .transpose()
        .map(Option::flatten)
}
