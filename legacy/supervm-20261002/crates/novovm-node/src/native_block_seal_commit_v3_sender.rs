//! Explicit single-decision outbound scheduling over existing Product Overlay.
//! No private key, background task, receiving loop or implicit service activation.
use super::*;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_block_seal::round_wire::{
    encode_nov_native_seal_round_wire_v1 as encode, round_wire_object_hash_v1,
};
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use crate::product_mainline_overlay::{
    ProductMainlineOverlayPayloadClassV1, ProductMainlineOverlayRoleV1,
    ProductMainlineOverlayRuntimeV1,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

pub struct NovNativeSealDecisionSenderV3 {
    authority: NovNativeSealEpochAuthorityV1,
    local_peer: String,
    peers: BTreeSet<String>,
    message: Message,
    wire: Vec<u8>,
    hash: [u8; 32],
    attempted: BTreeMap<([u8; 32], String), Instant>,
    next_send: usize,
    last_poll: Option<Instant>,
    halted: bool,
}

impl NovNativeSealDecisionSenderV3 {
    /// The frame is immutable. Construct a new sender for the completed certificate.
    pub fn new(
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        authority: NovNativeSealEpochAuthorityV1,
        local_validator_id: [u8; 32],
        message: Message,
    ) -> Result<Self> {
        if !message.is_decision_v3() {
            bail!("V3 sender requires a decision message");
        }
        authority.validate()?;
        let local_peer = authority.transport_peer_id(local_validator_id)?.to_owned();
        let height = message
            .proposal()
            .context("decision proposal missing")?
            .subject
            .height;
        let wire = encode(&message, &authority, height, &local_peer)?;
        let peers = authority
            .transport_bindings
            .iter()
            .filter(|b| b.validator_id != local_validator_id)
            .map(|b| b.transport_peer_id.clone())
            .collect();
        let sender = Self {
            authority,
            local_peer,
            peers,
            message,
            hash: round_wire_object_hash_v1(&wire),
            wire,
            attempted: BTreeMap::new(),
            next_send: 0,
            last_poll: None,
            halted: false,
        };
        sender.verify_durable(ledger, store)?;
        Ok(sender)
    }

    pub(super) fn verify_durable(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
    ) -> Result<()> {
        let subject = &self
            .message
            .proposal()
            .context("decision proposal missing")?
            .subject;
        let set = &self.authority.validator_set;
        let local = store.prepare_local_subject(
            ledger,
            subject.chain_id,
            subject.block_hash,
            set,
            subject.round,
            (subject.justify_qc_hash != [0; 32]).then_some(subject.justify_qc_hash),
        )?;
        if local != *subject {
            bail!("outbound decision no longer matches local execution");
        }
        match &self.message {
            Message::DecisionVoteV3 { qc, vote, .. } => {
                store.verify_decision_outbound_vote_v3(qc, vote, set)?
            }
            Message::DecisionCertificateV3 { decision, .. } => {
                if store
                    .load_decision_certificate_by_height_v3(
                        subject.chain_id,
                        subject.epoch,
                        subject.height,
                    )?
                    .as_ref()
                    != Some(decision.as_ref())
                {
                    bail!("outbound decision certificate is not the exact durable archive");
                }
            }
            _ => bail!("invalid decision sender message"),
        }
        Ok(())
    }

    pub fn halted(&self) -> bool {
        self.halted
    }

    /// Queue admissions, NOT delivery acknowledgments. No runtime events drained.
    pub fn poll(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<usize> {
        if runtime.role() != ProductMainlineOverlayRoleV1::Duplex
            || runtime.chain_id() != self.authority.chain_id
            || runtime.startup().local_peer_id != self.local_peer
            || runtime.remote_peer_ids().len() != self.peers.len()
            || runtime
                .remote_peer_ids()
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                != self.peers
        {
            self.halted = true;
            bail!("decision sender requires its pinned duplex validator mesh");
        }
        self.poll_with(ledger, store, now, |peer, hash, wire| {
            runtime.try_submit_to_peer(
                peer,
                ProductMainlineOverlayPayloadClassV1::NativeSeal,
                hash,
                wire,
            )
        })
    }

    pub(in crate::native_block_seal) fn poll_with(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        now: Instant,
        submit: impl FnMut(&str, [u8; 32], Vec<u8>) -> Result<bool>,
    ) -> Result<usize> {
        let result = (|| {
            if self.halted {
                bail!("decision sender halted; inspect and reconstruct");
            }
            if self.last_poll.is_some_and(|last| now < last) {
                bail!("decision sender clock moved backwards");
            }
            self.verify_durable(ledger, store)?;
            self.last_poll = Some(now);
            super::super::round_overlay::submit_frames(
                &[(self.hash, self.wire.clone())],
                &self.peers,
                &mut self.attempted,
                &mut self.next_send,
                now,
                submit,
            )
        })();
        if result.is_err() {
            self.halted = true;
        }
        result
    }
}
