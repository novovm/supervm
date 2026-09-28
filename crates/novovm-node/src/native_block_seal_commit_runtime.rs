//! Single prepared-round commit scheduler. No unlock, fork choice or promotion.
use super::*;
use crate::native_block_seal::commit_v2::{commit_target_v2, NovNativeSealCommitCertificateV2};

impl NovNativeSealRoundDriverV1 {
    fn commit_pin_key(&self, suffix: &str) -> String {
        format!("{}/commit-v2/{suffix}", Self::binding_key(&self.binding))
    }

    fn pin_commit_hash(
        &self,
        store: &NovNativeBlockSealStoreV1,
        suffix: &str,
        hash: [u8; 32],
    ) -> Result<()> {
        let key = self.commit_pin_key(suffix);
        let _guard = store.lock_writes_v1()?;
        match read_json_v1::<[u8; 32]>(&store.db, key.as_bytes(), "commit runtime pin")? {
            Some(old) if old != hash => bail!("commit runtime evidence pin changed"),
            Some(_) => return Ok(()),
            None => (),
        }
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, key.as_bytes(), &hash, "commit runtime pin")?;
        write_sync_v1(&store.db, batch)?;
        if read_json_v1::<[u8; 32]>(&store.db, key.as_bytes(), "commit pin readback")? != Some(hash)
        {
            bail!("commit runtime pin readback mismatch");
        }
        Ok(())
    }

    pub(super) fn recover_commit(&mut self, store: &NovNativeBlockSealStoreV1) -> Result<()> {
        if !self.binding.commit_v2 {
            return Ok(());
        }
        let stored = store.load_commit_certificate_by_height_v2(
            self.set().chain_id,
            self.set().epoch,
            self.binding.height,
        )?;
        let pinned = read_json_v1::<[u8; 32]>(
            &store.db,
            self.commit_pin_key("certificate").as_bytes(),
            "commit certificate pin",
        )?;
        if pinned.is_some_and(|hash| {
            stored
                .as_ref()
                .is_none_or(|cert| cert.certificate_hash != hash)
        }) || self
            .committed
            .as_ref()
            .is_some_and(|old| stored.as_ref() != Some(old))
        {
            bail!("commit runtime durable certificate disappeared or changed");
        }
        if let Some(cert) = &stored {
            let prepared = self
                .prepared
                .as_ref()
                .context("commit certificate without prepared QC")?;
            if commit_target_v2(&cert.prepare, self.set())?
                != commit_target_v2(prepared, self.set())?
            {
                bail!("commit runtime certificate conflicts with prepared target");
            }
            self.pin_commit_hash(store, "certificate", cert.certificate_hash)?;
        }
        self.committed = stored;
        Ok(())
    }

    pub(super) fn ingest_commit(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        message: &Message,
    ) -> Result<bool> {
        if self.binding.commit_v2
            && matches!(message, Message::CommitCertificateV2 { .. })
            && (message.round() != self.state.current.round || self.observed_commit.is_some())
        {
            return self.ingest_commit_catchup(ledger, store, message);
        }
        if !self.binding.commit_v2 || message.round() != self.state.current.round {
            return Ok(false);
        }
        let Some(prepared) = self.prepared.as_ref() else {
            return Ok(false);
        };
        let proposal = message
            .proposal()
            .context("commit message missing proposal")?;
        self.match_subject(ledger, store, &proposal.subject)?;
        let target = commit_target_v2(prepared, self.set())?;
        match message {
            Message::CommitVoteV2 { qc, vote, .. } => {
                if commit_target_v2(qc, self.set())? != target {
                    bail!("commit vote target differs from prepared target");
                }
                insert_unique(
                    &mut self.commit_votes,
                    vote.validator_id,
                    vote.as_ref().clone(),
                )
            }
            Message::CommitCertificateV2 { commit, .. } => {
                if commit_target_v2(&commit.prepare, self.set())? != target {
                    bail!("commit certificate target differs from prepared target");
                }
                if self.committed.is_some() || self.pending_commit.is_some() {
                    return Ok(false);
                }
                self.pending_commit = Some(commit.as_ref().clone());
                Ok(true)
            }
            _ => bail!("not a commit message"),
        }
    }

    pub(super) fn poll_commit(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        key: &SigningKey,
    ) -> Result<Vec<Message>> {
        let mut output = self.completed_output()?;
        if !self.binding.commit_v2 {
            return Ok(output);
        }
        self.recover_commit(store)?;
        let qc = self
            .prepared
            .clone()
            .context("commit scheduler requires prepared QC")?;
        let proposal = self
            .proposal
            .clone()
            .context("commit scheduler requires proposal")?;
        let local_slot = commit::lock_key(&qc.subject, self.binding.local_validator_id);
        let local_exists = store.db.get(local_slot.as_bytes())?.is_some();
        let vote_pin = read_json_v1::<[u8; 32]>(
            &store.db,
            self.commit_pin_key("vote").as_bytes(),
            "commit local vote pin",
        )?;
        if vote_pin.is_some() && !local_exists {
            bail!("commit runtime local vote disappeared");
        }
        if self.committed.is_none() {
            if let Some(cert) = self.pending_commit.take() {
                store.persist_local_verified_qc(ledger, &cert.prepare, self.set())?;
                store.persist_local_verified_commit_certificate_v2(ledger, &cert, self.set())?;
                self.recover_commit(store)?;
            }
        }
        if self.committed.is_none() {
            let timed_out = store
                .load_local_timeout(
                    ledger,
                    self.set(),
                    self.binding.height,
                    self.state.current.round,
                    self.binding.local_validator_id,
                )?
                .is_some();
            // A valid late QC is evidence, not permission to sign after timeout.
            // Existing signatures may be replayed; otherwise remain an observer.
            if local_exists || !timed_out {
                let vote = store.sign_local_commit_vote_v2(ledger, &qc, self.set(), key)?;
                if vote_pin.is_some_and(|hash| hash != vote.vote_hash) {
                    bail!("commit runtime local vote changed");
                }
                self.pin_commit_hash(store, "vote", vote.vote_hash)?;
                insert_unique(&mut self.commit_votes, vote.validator_id, vote.clone())?;
                output.push(Message::CommitVoteV2 {
                    proposal: Box::new(proposal.clone()),
                    qc: Box::new(qc.clone()),
                    vote: Box::new(vote),
                    certificate: self.certificate.clone().map(Box::new),
                });
            }
            if self.has_quorum(self.commit_votes.keys())? {
                let cert = NovNativeSealCommitCertificateV2::from_votes(
                    qc,
                    self.set(),
                    self.commit_votes.values().cloned().collect(),
                )?;
                store.persist_local_verified_commit_certificate_v2(ledger, &cert, self.set())?;
                self.recover_commit(store)?;
            }
        }
        if let Some(cert) = &self.committed {
            output.push(Message::CommitCertificateV2 {
                proposal: Box::new(proposal),
                commit: Box::new(cert.clone()),
                certificate: self.certificate.clone().map(Box::new),
            });
        }
        self.validate_output(output)
    }
}
