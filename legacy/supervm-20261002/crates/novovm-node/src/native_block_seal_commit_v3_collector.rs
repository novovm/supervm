//! Bounded, single-local-decision collection. No signer, DB write or activation.
use super::*;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_block_seal::round_wire::{
    decode_nov_native_seal_round_wire_v1 as decode, encode_nov_native_seal_round_wire_v1 as encode,
};
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use std::collections::BTreeMap;

pub struct NovNativeSealDecisionCollectorV3 {
    authority: NovNativeSealEpochAuthorityV1,
    height: u64,
    target: [u8; 32],
    votes: BTreeMap<[u8; 32], NovNativeSealDecisionVoteV3>,
    witness: Option<Message>,
    complete: Option<Message>,
    weight: u64,
}

impl NovNativeSealDecisionCollectorV3 {
    /// Pin a locally executed decision, not the first remote packet's target.
    /// This is a read-only match, not new-view admission or signing permission.
    pub fn new(
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        authority: NovNativeSealEpochAuthorityV1,
        prepare: &NovNativeSealQuorumCertificateV1,
    ) -> Result<Self> {
        authority.validate()?;
        let set = &authority.validator_set;
        let target = decision_target_v3(prepare, set)?;
        let subject = &prepare.subject;
        authority.validate_subject_domain_v1(subject)?;
        if subject.genesis_block_hash != authority.genesis_block_hash
            || subject.protocol_config_commitment != authority.protocol_config_commitment
        {
            bail!("decision collector authority does not match local domain");
        }
        let local = store.prepare_local_subject(
            ledger,
            subject.chain_id,
            subject.block_hash,
            set,
            subject.round,
            (subject.justify_qc_hash != [0; 32]).then_some(subject.justify_qc_hash),
        )?;
        if local != *subject {
            bail!("decision collector requires matching local execution");
        }
        Ok(Self {
            authority,
            height: subject.height,
            target,
            votes: BTreeMap::new(),
            witness: None,
            complete: None,
            weight: 0,
        })
    }

    pub fn signed_weight(&self) -> u64 {
        self.weight
    }

    /// In-memory verified quorum only, NOT durable confirmation or finality.
    pub fn certificate_message(&self) -> Option<&Message> {
        self.complete.as_ref()
    }

    /// Source must be obtained from the authenticated transport, not payload.
    /// Every packet is decoded/verified even after quorum; rejects foreign goals.
    /// Errors leave all collector state unchanged.
    pub fn ingest_wire(&mut self, source_peer_id: &str, wire: &[u8]) -> Result<bool> {
        let message = decode(wire, &self.authority, self.height, source_peer_id)?;
        let prepare = match &message {
            Message::DecisionVoteV3 { qc, .. } => qc.as_ref(),
            Message::DecisionCertificateV3 { decision, .. } => &decision.prepare,
            _ => bail!("decision collector accepts V3 messages only"),
        };
        let set = &self.authority.validator_set;
        if decision_target_v3(prepare, set)? != self.target {
            bail!("decision collector rejects another execution target");
        }
        if self.complete.is_some() {
            return Ok(false);
        }
        if let Message::DecisionCertificateV3 { decision, .. } = &message {
            self.weight = decision.signed_weight;
            self.complete = Some(message);
            self.votes.clear();
            self.witness = None;
            return Ok(true);
        }
        let Message::DecisionVoteV3 { vote, .. } = &message else {
            unreachable!()
        };
        if let Some(previous) = self.votes.get(&vote.validator_id) {
            if previous != vote.as_ref() {
                bail!("decision collector signer changed its vote");
            }
            return Ok(false);
        }
        // Authentication restricts ids to the pinned set; one vote per id.
        let weight = self
            .weight
            .checked_add(
                set.validator(vote.validator_id)
                    .context("unknown decision signer")?
                    .weight,
            )
            .context("decision weight overflow")?;
        let witness = self.witness.as_ref().unwrap_or(&message);
        let completed = if weight >= set.quorum_weight {
            let Message::DecisionVoteV3 {
                proposal,
                qc,
                certificate,
                ..
            } = witness
            else {
                unreachable!()
            };
            let mut votes = self.votes.values().cloned().collect::<Vec<_>>();
            votes.push(vote.as_ref().clone());
            let decision =
                NovNativeSealDecisionCertificateV3::from_votes(qc.as_ref().clone(), set, votes)?;
            let envelope = Message::DecisionCertificateV3 {
                proposal: proposal.clone(),
                decision: Box::new(decision),
                certificate: certificate.clone(),
            };
            // Check the resulting aggregate's wire budget BEFORE changing state.
            encode(&envelope, &self.authority, self.height, source_peer_id)?;
            Some(envelope)
        } else {
            None
        };
        self.weight = weight;
        if let Some(envelope) = completed {
            self.complete = Some(envelope);
            self.votes.clear();
            self.witness = None;
        } else {
            self.votes.insert(vote.validator_id, vote.as_ref().clone());
            if self.witness.is_none() {
                self.witness = Some(message);
            }
        }
        Ok(true)
    }
}
