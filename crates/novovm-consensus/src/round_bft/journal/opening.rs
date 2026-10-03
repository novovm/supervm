//! Explicit cold startup, never called to advance an ordinary block. Verify
//! the pinned genesis-to-head prefix, then reconcile the ONE global signer log.
use super::backend::RecoveredChain;
use super::*;

pub struct JournalOpening<B: JournalBackend> {
    identity: Option<Identity>,
    initial: Option<B::ReadTicket>,
    loaded: Option<Option<Vec<u8>>>,
    chain: Option<B::Recovery>,
    recovered_chain: Option<RecoveredChain<B::Record>>,
    recovered: Option<(Snapshot, Vec<u8>)>,
    last: Option<B::ReadTicket>,
    failed: bool,
}

impl<B: JournalBackend> JournalOpening<B> {
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

    pub fn poll(&mut self, pipeline: &B) -> Result<Option<ValidatorJournal<B>>> {
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

    fn poll_inner(&mut self, pipeline: &B) -> Result<Option<ValidatorJournal<B>>> {
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
            self.chain = Some(B::begin_recovery(
                identity.context,
                identity.parent,
                identity.set.clone(),
                head,
            )?);
            self.loaded = Some(saved);
            return Ok(None);
        }
        if self.recovered_chain.is_none() {
            let Some(chain) =
                pipeline.poll_recovery(self.chain.as_mut().context("missing chain recovery")?)?
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
                replay: ReplayIndex::default(),
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
                verified_witness: None,
                replay_records: Vec::new(),
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
        let references = snapshot.replay.references()?;
        let mut keys = vec![MetaKey::ConsensusOutbox {
            validator: identity.validator,
            sequence: snapshot.revision,
        }];
        for reference in &references {
            if reference.revision != snapshot.revision {
                keys.push(MetaKey::ConsensusOutbox {
                    validator: identity.validator,
                    sequence: reference.revision,
                });
            }
        }
        keys.push(MetaKey::ConsensusState(identity.validator));
        keys.push(MetaKey::ChainHead);
        ensure!(
            keys.len() <= MAX_REPLAY_RECORDS + 3,
            "opening replay read exceeds metadata key bound"
        );
        if self.last.is_none() {
            self.last = pipeline.try_read_consensus_metadata(keys.clone())?;
        }
        let Some(reply) = take(&mut self.last)? else {
            return Ok(None);
        };
        ensure!(
            reply.values.len() == keys.len()
                && reply.values[keys.len() - 2].as_ref() == Some(saved)
                && reply.values[keys.len() - 1] == chain.head_bytes,
            "signing state/head changed during startup"
        );
        let latest = codec::decode_outbox(
            identity,
            snapshot,
            saved,
            reply.values[0]
                .as_deref()
                .context("durable signing outbox missing")?,
        )?;
        let message = latest.message.clone();
        let mut latest = Some(latest);
        let mut replay_records = Vec::with_capacity(references.len());
        let mut offset = 1;
        for reference in &references {
            let record = if reference.revision == snapshot.revision {
                let record = latest.take().context("duplicate latest replay event")?;
                ensure!(
                    codec::replay_digest(&record)? == reference.digest,
                    "latest outbox replay payload differs from snapshot locator"
                );
                record
            } else {
                let bytes = reply.values[offset]
                    .as_deref()
                    .context("referenced immutable replay outbox missing")?;
                offset += 1;
                codec::decode_replay(identity, reference, bytes)?
            };
            replay_records.push(record);
        }
        codec::validate_replay_index(identity, snapshot, &replay_records)?;
        let verified_witness = match snapshot.replay.valid {
            Some(reference) => {
                let record = replay_records
                    .iter()
                    .find(|record| record.revision == reference.revision)
                    .context("valid replay witness missing after validation")?;
                let ReplayEvidence::Certified { certificate, .. } = &record.evidence else {
                    anyhow::bail!("valid replay role has no typed certificate")
                };
                Some(Arc::new(certificate.clone()))
            }
            None => snapshot
                .witness
                .as_ref()
                .map(|quorum| quorum.verify(&identity.set).map(Arc::new))
                .transpose()?,
        };
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
            verified_witness,
            replay_records,
        }))
    }
}

fn take<T>(ticket: &mut Option<impl JournalTicket<T>>) -> Result<Option<T>> {
    ticket
        .as_mut()
        .map(JournalTicket::try_take)
        .transpose()
        .map(Option::flatten)
}
