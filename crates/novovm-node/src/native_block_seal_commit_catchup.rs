//! Observe a complete cross-round quorum without adopting its signing round.
//! The self-contained envelope is separate from active proposal/QC/admission
//! storage. Import cannot authorize a signature, unlock, or state promotion.
use super::*;
use crate::native_block_seal::commit_v2::commit_target_v2;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    schema: String,
    authority_commitment: [u8; 32],
    message: Message,
}

impl NovNativeSealRoundDriverV1 {
    fn observation_key(&self) -> String {
        format!("{}/commit-v2/observed", Self::binding_key(&self.binding))
    }

    fn validate_observation(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        message: &Message,
    ) -> Result<[u8; 32]> {
        let Message::CommitCertificateV2 {
            proposal, commit, ..
        } = message
        else {
            bail!("commit catchup requires a complete certificate, not individual votes");
        };
        message.validate_authenticated(
            &self.binding.authority,
            self.binding.height,
            self.binding
                .authority
                .transport_peer_id(self.binding.local_validator_id)?,
        )?;
        self.match_subject(ledger, store, &proposal.subject)?;
        let target = commit_target_v2(&commit.prepare, self.set())?;
        if let Some(local) = store.load_commit_certificate_by_height_v2(
            self.set().chain_id,
            self.set().epoch,
            self.binding.height,
        )? {
            if target != commit_target_v2(&local.prepare, self.set())? {
                bail!("cross-round certificate conflicts with existing local commit certificate");
            }
        }
        Ok(target)
    }

    pub(super) fn ingest_commit_catchup(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        message: &Message,
    ) -> Result<bool> {
        let target = self.validate_observation(ledger, store, message)?;
        if let Some(previous) = self
            .observed_commit
            .as_ref()
            .or(self.pending_observed_commit.as_ref())
        {
            if self.validate_observation(ledger, store, previous)? != target {
                bail!("cross-round commit observations have conflicting targets");
            }
            return Ok(false);
        }
        self.pending_observed_commit = Some(message.clone());
        Ok(true)
    }

    pub(super) fn recover_observed_commit(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
    ) -> Result<()> {
        if !self.binding.commit_v2 {
            return Ok(());
        }
        let key = self.observation_key();
        let record = read_json_v1::<Observation>(&store.db, key.as_bytes(), "commit observation")?;
        let pin = read_json_v1::<[u8; 32]>(
            &store.db,
            format!("{key}/hash").as_bytes(),
            "commit observation pin",
        )?;
        match &record {
            None if pin.is_some() || self.observed_commit.is_some() => {
                bail!("durable commit observation disappeared")
            }
            None => (),
            Some(record) => {
                if record.schema != "novovm-native-seal-commit-observation/v2"
                    || record.authority_commitment != self.binding.authority.authority_commitment
                    || self
                        .observed_commit
                        .as_ref()
                        .is_some_and(|old| old != &record.message)
                {
                    bail!("commit observation binding changed");
                }
                self.validate_observation(ledger, store, &record.message)?;
                if pin != Some(observation_hash(record)?) {
                    bail!("commit observation pin is missing or mismatched");
                }
            }
        }
        self.observed_commit = record.map(|record| record.message);
        Ok(())
    }

    pub(super) fn poll_commit_catchup(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
    ) -> Result<Option<Vec<Message>>> {
        if !self.binding.commit_v2 {
            return Ok(None);
        }
        self.recover_observed_commit(ledger, store)?;
        if let (Some(stored), Some(pending)) =
            (&self.observed_commit, &self.pending_observed_commit)
        {
            if self.validate_observation(ledger, store, stored)?
                != self.validate_observation(ledger, store, pending)?
            {
                bail!("pending commit observation conflicts with durable evidence");
            }
        }
        if self.observed_commit.is_none() {
            if let Some(message) = self.pending_observed_commit.clone() {
                self.validate_observation(ledger, store, &message)?;
                let record = Observation {
                    schema: "novovm-native-seal-commit-observation/v2".into(),
                    authority_commitment: self.binding.authority.authority_commitment,
                    message,
                };
                let key = self.observation_key();
                {
                    let _guard = store.lock_writes_v1()?;
                    if store.db.get(key.as_bytes())?.is_some()
                        || store.db.get(format!("{key}/hash"))?.is_some()
                    {
                        bail!("commit observation changed outside its owner");
                    }
                    let mut batch = RocksDbWriteBatch::default();
                    put_json_v1(&mut batch, key.as_bytes(), &record, "commit observation")?;
                    put_json_v1(
                        &mut batch,
                        format!("{key}/hash").as_bytes(),
                        &observation_hash(&record)?,
                        "commit observation pin",
                    )?;
                    write_sync_v1(&store.db, batch)?;
                }
                self.recover_observed_commit(ledger, store)?;
                if self.observed_commit.as_ref() != Some(&record.message) {
                    bail!("commit observation readback mismatch");
                }
            }
        }
        self.pending_observed_commit = None;
        Ok(self.observed_commit.clone().map(|message| vec![message]))
    }
}

fn observation_hash(record: &Observation) -> Result<[u8; 32]> {
    let mut bounded =
        vec![0; super::super::round_wire::NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1 + 256];
    let bytes =
        postcard::to_slice(record, &mut bounded).context("commit observation exceeds its bound")?;
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-seal-commit-observation-v2\0");
    hash.update(bytes);
    Ok(hash.finalize().into())
}
