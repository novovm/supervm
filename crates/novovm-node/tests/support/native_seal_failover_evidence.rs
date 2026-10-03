use anyhow::{ensure, Context, Result};
use novovm_node::{
    native_block_seal::{
        commit_v3::NovNativeSealDecisionCertificateV3,
        newview::{NovNativeSealNewViewAdmissionV1, NovNativeSealNewViewQcV1},
        timeout::NovNativeSealTimeoutContextV1,
        NovNativeBlockSealStoreV1,
    },
    native_block_seal_overlay::NovNativeSealEpochAuthorityV1,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct FailoverEvidence {
    pub(super) decision: NovNativeSealDecisionCertificateV3,
    pub(super) proposal: NovNativeSealNewViewQcV1,
    pub(super) admission: NovNativeSealNewViewAdmissionV1,
}

impl FailoverEvidence {
    pub(super) fn read(
        store: &NovNativeBlockSealStoreV1,
        authority: &NovNativeSealEpochAuthorityV1,
        height: u64,
        block_hash: [u8; 32],
        offline: [u8; 32],
    ) -> Result<Self> {
        let decision = store
            .load_decision_certificate_by_height_v3(authority.chain_id, authority.epoch, height)?
            .context("failover decision missing")?;
        let proposal = NovNativeSealNewViewQcV1 {
            proposal: store
                .load_proposal(decision.prepare.proposal_hash)?
                .context("failover proposal missing")?,
            qc: decision.prepare.clone(),
        };
        let admission = store
            .load_local_new_view_admission(
                authority.chain_id,
                authority.epoch,
                height,
                decision.prepare.subject.round,
            )?
            .context("failover new-view admission missing")?;
        let evidence = Self {
            decision,
            proposal,
            admission,
        };
        evidence.validate(authority, height, block_hash, offline)?;
        Ok(evidence)
    }

    pub(super) fn validate(
        &self,
        authority: &NovNativeSealEpochAuthorityV1,
        height: u64,
        block_hash: [u8; 32],
        offline: [u8; 32],
    ) -> Result<()> {
        authority.validate()?;
        ensure!(authority.expected_leader(height, 0)? == offline);
        self.decision.verify(&authority.validator_set)?;
        let subject = &self.decision.prepare.subject;
        ensure!(
            subject.round > 0,
            "offline round-zero leader cannot finalize"
        );
        ensure!(subject.height == height && subject.block_hash == block_hash);
        ensure!(self.proposal.qc == self.decision.prepare);
        ensure!(self.admission.authority == *authority && self.admission.subject == *subject);
        let context = NovNativeSealTimeoutContextV1 {
            chain_id: authority.chain_id,
            genesis_block_hash: authority.genesis_block_hash,
            protocol_config_commitment: authority.protocol_config_commitment,
            epoch: authority.epoch,
            validator_set_hash: authority.validator_set.validator_set_hash,
            height,
            round: subject.round,
        };
        self.admission.certificate.verify(&context, authority)?;
        let mut next = context;
        next.round = next.round.checked_add(1).context("round overflow")?;
        self.proposal.verify(&next, authority)?;
        ensure!(self.proposal.proposal.proposer_id != offline);
        ensure!(self.decision.votes.len() == 3 && self.proposal.qc.votes.len() == 3);
        ensure!(self
            .decision
            .votes
            .iter()
            .all(|vote| vote.validator_id != offline));
        ensure!(self
            .proposal
            .qc
            .votes
            .iter()
            .all(|vote| vote.validator_id != offline));
        ensure!(self
            .admission
            .certificate
            .previous_timeout
            .votes
            .iter()
            .all(|vote| vote.validator_id != offline));
        ensure!(self
            .admission
            .certificate
            .observations
            .iter()
            .all(|vote| vote.validator_id != offline));
        Ok(())
    }

    pub(super) fn check_negative_cases(
        &self,
        authority: &NovNativeSealEpochAuthorityV1,
        height: u64,
        block_hash: [u8; 32],
        offline: [u8; 32],
    ) {
        let mut bad = self.clone();
        bad.admission.certificate.previous_timeout.votes.truncate(2);
        assert!(bad
            .validate(authority, height, block_hash, offline)
            .is_err());
        let mut bad = self.clone();
        bad.admission.certificate.observations.truncate(2);
        assert!(bad
            .validate(authority, height, block_hash, offline)
            .is_err());
        let mut bad = self.clone();
        bad.decision.votes.truncate(2);
        assert!(bad
            .validate(authority, height, block_hash, offline)
            .is_err());
        let mut bad = self.clone();
        bad.admission.certificate.previous_timeout.votes[0].signature[0] ^= 1;
        assert!(bad
            .validate(authority, height, block_hash, offline)
            .is_err());
        let mut bad = self.clone();
        bad.proposal.proposal.signature[0] ^= 1;
        assert!(bad
            .validate(authority, height, block_hash, offline)
            .is_err());
        let mut bad = self.clone();
        bad.admission.subject.block_hash[0] ^= 1;
        assert!(bad
            .validate(authority, height, block_hash, offline)
            .is_err());
        assert!(self
            .validate(authority, height + 1, block_hash, offline)
            .is_err());
        let mut other_block = block_hash;
        other_block[0] ^= 1;
        assert!(self
            .validate(authority, height, other_block, offline)
            .is_err());
    }
}
