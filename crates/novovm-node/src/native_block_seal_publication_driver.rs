//! Completed fresh-chain publication remains a read-only certificate relay.
//! It holds no signing key and admits no new transactions or votes.
use super::*;
use crate::native_block_seal::commit_v3::lifecycle::certificate_envelope;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_block_seal::round_wire::{
    encode_nov_native_seal_round_wire_v1, round_wire_object_hash_v1,
};
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use crate::tx_ingress::candidate_workspace::{
    self as workspace, FreshSuccessorPublicationV1, GenesisPromotionPublicationV1,
};

#[derive(PartialEq, Eq, serde::Serialize)]
#[serde(untagged)]
enum PublicationReport {
    Genesis(GenesisPromotionPublicationV1),
    Successor(FreshSuccessorPublicationV1),
}
impl PublicationReport {
    fn finalized(&self) -> bool {
        match self {
            Self::Genesis(report) => report.finalized,
            Self::Successor(report) => report.finalized,
        }
    }
}

pub struct FreshGenesisPublicationDriverV1 {
    body_delivery: Option<body_delivery::BodyDeliveryV1>,
    chain: u64,
    height: u64,
    parent: Option<[u8; 32]>,
    id: [u8; 32],
    pin: [u8; 32],
    params: serde_json::Value,
    authority: NovNativeSealEpochAuthorityV1,
    local_peer: String,
    peers: BTreeSet<String>,
    frames: Vec<([u8; 32], Vec<u8>)>,
    certificate_hash: [u8; 32],
    report: PublicationReport,
    attempted: BTreeMap<([u8; 32], String), Instant>,
    next_send: usize,
    last_seen: Instant,
    sent: usize,
    halted: bool,
}

impl FreshGenesisPublicationDriverV1 {
    /// No archive means no publication. A corrupt/mismatched archive is an error,
    /// never a reason to fall back to signing or to silently initialize storage.
    pub fn open(
        config: &NovNativeSealServiceConfigV1,
        ledger_path: &Path,
        params: &serde_json::Value,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<Option<Self>> {
        config.validate(runtime.chain_id())?;
        validate_service_paths_v1(config, ledger_path, &[], &[])?;
        check_runtime(config, runtime)?;
        let pin = config
            .fresh_genesis_config_commitment
            .context("publication requires fresh genesis")?;
        let id = config
            .isolated_workspace_id
            .context("publication workspace missing")?;
        let Some(store) =
            NovNativeBlockSealStoreV1::open_existing_read_only(&config.seal_store_path)?
        else {
            return Ok(None);
        };
        let Some(certificate) = store.load_decision_certificate_by_height_v3(
            config.chain_id,
            config.authority.epoch,
            config.height,
        )?
        else {
            return Ok(None);
        };
        certificate.verify(&config.authority.validator_set)?;
        let subject = &certificate.prepare.subject;
        if subject.block_hash != config.block_hash || subject.height != config.height {
            bail!("publication archive differs from configured candidate");
        }
        let certificate_hash = certificate.certificate_hash;
        let envelope = certificate_envelope(&store, certificate)?;
        let proof = crate::native_block_ledger::NovNativeFreshFinalityProofV1 {
            authority: config.authority.clone(),
            witness: envelope.clone(),
        };
        let Message::DecisionCertificateV3 {
            proposal,
            decision,
            certificate,
        } = &envelope
        else {
            bail!("publication requires a decision certificate envelope");
        };
        let prepare = Message::QuorumCertificate {
            proposal: proposal.clone(),
            qc: Box::new(decision.prepare.clone()),
            certificate: certificate.clone(),
        };
        let body_proposal = Message::Proposal {
            proposal: proposal.clone(),
            certificate: certificate.clone(),
        };
        let local_peer = runtime.startup().local_peer_id.clone();
        // Both messages are validated before any state publication. Repeating
        // prepare as well as decision allows a slow/restarted peer to catch up.
        let frames = [prepare, envelope]
            .iter()
            .map(|message| {
                let wire = encode_nov_native_seal_round_wire_v1(
                    message,
                    &config.authority,
                    config.height,
                    &local_peer,
                )?;
                Ok((round_wire_object_hash_v1(&wire), wire))
            })
            .collect::<Result<Vec<_>>>()?;
        let artifact = workspace::load_block_artifact_v1(config.chain_id, id, params)?
            .context("publication output missing")?;
        if artifact.block().header.block_hash != config.block_hash
            || artifact.fresh_genesis_identity().is_none_or(|identity| {
                identity.config_commitment() != pin
                    || identity.anchor() != config.authority.genesis_block_hash
            })
        {
            bail!("publication output differs from configured identity");
        }
        let body_delivery = body_delivery::BodyDeliveryV1::build(
            config,
            &body_proposal,
            &artifact.block().body.raw_txs,
        )?;
        let report = if let Some(parent) = config.finalized_parent_workspace_id {
            PublicationReport::Successor(workspace::resume_successor_promotion_v1(
                config.chain_id,
                parent,
                id,
                pin,
                &proof,
                ledger_path,
                params,
            )?)
        } else {
            workspace::resume_genesis_promotion_v1(
                config.chain_id,
                id,
                pin,
                &config.seal_store_path,
                ledger_path,
                params,
            )?;
            PublicationReport::Genesis(workspace::finalize_genesis_promotion_v1(
                config.chain_id,
                id,
                pin,
                &proof,
                params,
            )?)
        };
        Ok(Some(Self {
            body_delivery,
            chain: config.chain_id,
            height: config.height,
            parent: config.finalized_parent_workspace_id,
            id,
            pin,
            params: params.clone(),
            authority: config.authority.clone(),
            local_peer,
            peers: runtime.remote_peer_ids().iter().cloned().collect(),
            frames,
            certificate_hash,
            report,
            attempted: BTreeMap::new(),
            next_send: 0,
            last_seen: now,
            sent: 0,
            halted: false,
        }))
    }

    pub fn poll(&mut self, runtime: &ProductMainlineOverlayRuntimeV1, now: Instant) -> Result<()> {
        if self.halted {
            bail!("published relay halted; inspect and restart");
        }
        let result = self.poll_inner(runtime, now);
        if result.is_err() {
            self.halted = true;
        }
        result
    }

    fn poll_inner(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        if now < self.last_seen
            || runtime.chain_id() != self.chain
            || runtime.role() != ProductMainlineOverlayRoleV1::Duplex
            || runtime.startup().local_peer_id != self.local_peer
            || runtime
                .remote_peer_ids()
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                != self.peers
        {
            bail!("published relay runtime or monotonic clock changed");
        }
        self.authority.validate()?;
        let verified = match self.parent {
            Some(parent) => PublicationReport::Successor(workspace::verify_successor_authority_v1(
                self.chain,
                parent,
                self.id,
                self.pin,
                &self.params,
            )?),
            None => PublicationReport::Genesis(workspace::verify_genesis_promotion_v1(
                self.chain,
                self.id,
                self.pin,
                &self.params,
            )?),
        };
        if verified != self.report {
            bail!("published relay authority changed");
        }
        self.last_seen = now;
        let sent = crate::native_block_seal::round_overlay::submit_frames(
            &self.frames,
            &self.peers,
            &mut self.attempted,
            &mut self.next_send,
            now,
            |peer, hash, bytes| {
                runtime.try_submit_to_peer(
                    peer,
                    ProductMainlineOverlayPayloadClassV1::NativeSeal,
                    hash,
                    bytes,
                )
            },
        )?;
        self.sent = self.sent.saturating_add(sent);
        if let Some(body) = &mut self.body_delivery {
            body.poll(runtime, now)?;
        }
        Ok(())
    }

    pub fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled":true, "ok":!self.halted, "halted":self.halted, "prepared":!self.halted,
            "decision_v3_enabled":true, "decision_confirmed":!self.halted,
            "decision_certificate_hash":crate::native_block_seal::hex_v1(&self.certificate_hash),
            "phase":"PublishedCertificateRelay", "height":self.height,
            "publication":self.report, "queued_egress":self.sent,
            "signing_enabled":false, "finalized":!self.halted && self.report.finalized(),
            "safe":!self.halted && self.report.finalized(),
            "proof_sealed":!self.halted && self.report.finalized(),
            "chain_canonical":!self.halted && self.report.finalized(),
            "proof_kind":"bft_decision_v3_with_local_aoem_readback",
            "zero_knowledge_execution_proof":false,
        })
    }
}
