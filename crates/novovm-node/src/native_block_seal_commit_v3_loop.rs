//! Explicit lifecycle for an already durable local vote. Never owns a signer.
use super::collector::NovNativeSealDecisionCollectorV3 as Collector;
use super::sender::NovNativeSealDecisionSenderV3 as Sender;
use super::*;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_block_seal::round_wire::{
    encode_nov_native_seal_round_wire_v1 as encode, round_wire_object_hash_v1,
    NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1,
};
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use crate::product_delivery_journal::product_delivery_id_v1;
use crate::product_mainline_overlay::{
    ProductMainlineOverlayInboundV1 as Inbound, ProductMainlineOverlayPayloadClassV1 as Class,
    ProductMainlineOverlayRoleV1 as Role, ProductMainlineOverlayRuntimeV1 as Runtime,
    PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
};
use novovm_network::NovoRudpTransportFrameKindV0;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

struct Inbox {
    pending: VecDeque<Inbound>,
    recent: VecDeque<Instant>,
}

pub struct NovNativeSealDecisionLoopV3 {
    authority: NovNativeSealEpochAuthorityV1,
    id: [u8; 32],
    local_peer: String,
    local_vote: Message,
    collector: Collector,
    sender: Sender,
    inbox: BTreeMap<String, Inbox>,
    cursor: usize,
    last_poll: Instant,
    durable_hash: Option<[u8; 32]>,
    halted: bool,
}

impl NovNativeSealDecisionLoopV3 {
    #[allow(clippy::too_many_arguments)]
    pub fn attach(
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        authority: NovNativeSealEpochAuthorityV1,
        id: [u8; 32],
        local_vote: Message,
        runtime: &Runtime,
        now: Instant,
    ) -> Result<Self> {
        let Message::DecisionVoteV3 { qc, .. } = &local_vote else {
            bail!("V3 lifecycle requires a local vote envelope");
        };
        let local_peer = authority.transport_peer_id(id)?.to_owned();
        encode(&local_vote, &authority, qc.subject.height, &local_peer)?;
        let collector = Collector::new(ledger, store, authority.clone(), qc)?;
        let stored = store.load_decision_certificate_by_height_v3(
            authority.chain_id,
            authority.epoch,
            qc.subject.height,
        )?;
        let (outbound, durable_hash) = if let Some(cert) = stored {
            if decision_target_v3(&cert.prepare, &authority.validator_set)?
                != decision_target_v3(qc, &authority.validator_set)?
            {
                bail!("V3 lifecycle recovered another decision");
            }
            let hash = cert.certificate_hash;
            (certificate_envelope(store, cert)?, Some(hash))
        } else {
            (local_vote.clone(), None)
        };
        let sender = Sender::new(ledger, store, authority.clone(), id, outbound)?;
        let inbox = authority
            .transport_bindings
            .iter()
            .filter(|b| b.validator_id != id)
            .map(|b| {
                (
                    b.transport_peer_id.clone(),
                    Inbox {
                        pending: VecDeque::new(),
                        recent: VecDeque::new(),
                    },
                )
            })
            .collect();
        let mut result = Self {
            authority,
            id,
            local_peer,
            local_vote,
            collector,
            sender,
            inbox,
            cursor: 0,
            last_poll: now,
            durable_hash,
            halted: false,
        };
        result.check_runtime(runtime)?;
        if result.durable_hash.is_none() {
            let height = qc_height(&result.local_vote)?;
            let wire = encode(
                &result.local_vote,
                &result.authority,
                height,
                &result.local_peer,
            )?;
            result.collector.ingest_wire(&result.local_peer, &wire)?;
        }
        Ok(result)
    }

    fn check_runtime(&self, runtime: &Runtime) -> Result<()> {
        if self.halted
            || runtime.chain_id() != self.authority.chain_id
            || runtime.role() != Role::Duplex
            || runtime.startup().local_peer_id != self.local_peer
            || runtime.remote_peer_ids().len() != self.inbox.len()
            || runtime
                .remote_peer_ids()
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                != self.inbox.keys().cloned().collect()
        {
            bail!("V3 lifecycle halted or runtime identity/mesh mismatch");
        }
        Ok(())
    }

    /// Only pass events from the authenticated Overlay; constructing this public
    /// struct from RPC input does not authenticate it. Bounded staging, no DB IO.
    pub fn enqueue(&mut self, inbound: Inbound) -> bool {
        if self.halted
            || inbound.payload_class != Class::NativeSeal
            || inbound.frame.payload.len() > NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1
        {
            return false;
        }
        let Some(inbox) = self.inbox.get_mut(&inbound.source_peer_id) else {
            return false;
        };
        if inbox.pending.len() >= 4 {
            return false;
        }
        inbox.pending.push_back(inbound);
        true
    }

    pub fn confirmed(&self) -> bool {
        !self.halted && self.durable_hash.is_some()
    }
    pub fn halted(&self) -> bool {
        self.halted
    }

    pub fn poll(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        runtime: &Runtime,
        now: Instant,
    ) -> Result<usize> {
        let result = self.poll_inner(ledger, store, runtime, now);
        if result.is_err() {
            self.halted = true;
        }
        result
    }

    fn poll_inner(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        runtime: &Runtime,
        now: Instant,
    ) -> Result<usize> {
        self.check_runtime(runtime)?;
        if now < self.last_poll {
            bail!("V3 lifecycle clock moved backwards");
        }
        self.last_poll = now;
        // Do not let a newly received quorum hide loss of our previously pinned
        // vote/archive evidence by switching senders before it is checked.
        self.sender.verify_durable(ledger, store)?;
        let peers = self.inbox.keys().cloned().collect::<Vec<_>>();
        let mut budget = 8;
        let start = self.cursor;
        for offset in 0..peers.len() * 4 {
            if budget == 0 {
                break;
            }
            let index = (start + offset) % peers.len();
            let inbox = self
                .inbox
                .get_mut(&peers[index])
                .context("V3 peer missing")?;
            while inbox
                .recent
                .front()
                .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1))
            {
                inbox.recent.pop_front();
            }
            if inbox.recent.len() >= 16 {
                continue;
            }
            let Some(inbound) = inbox.pending.pop_front() else {
                continue;
            };
            inbox.recent.push_back(now);
            budget -= 1;
            self.cursor = (index + 1) % peers.len();
            // Bad remote traffic consumes its own budget, never halts the owner.
            if self.validate_inbound(&inbound).is_ok() && self.durable_hash.is_none() {
                let _ = self
                    .collector
                    .ingest_wire(&inbound.source_peer_id, &inbound.frame.payload);
            }
        }
        if self.durable_hash.is_none() {
            if let Some(Message::DecisionCertificateV3 { decision, .. }) =
                self.collector.certificate_message()
            {
                let Message::DecisionVoteV3 { qc, .. } = &self.local_vote else {
                    unreachable!()
                };
                // V3 decisions allow another equivalent witness. Archive using
                // our already durable/admitted witness, not remote signing state.
                let certificate = NovNativeSealDecisionCertificateV3::from_votes(
                    qc.as_ref().clone(),
                    &self.authority.validator_set,
                    decision.votes.clone(),
                )?;
                store.persist_local_verified_decision_certificate_v3(
                    ledger,
                    &certificate,
                    &self.authority.validator_set,
                )?;
                let stored = store
                    .load_decision_certificate_by_height_v3(
                        self.authority.chain_id,
                        self.authority.epoch,
                        qc.subject.height,
                    )?
                    .context("V3 archive readback missing")?;
                let hash = stored.certificate_hash;
                self.sender = Sender::new(
                    ledger,
                    store,
                    self.authority.clone(),
                    self.id,
                    certificate_envelope(store, stored)?,
                )?;
                self.durable_hash = Some(hash);
            }
        }
        self.sender.poll(ledger, store, runtime, now)
    }

    fn validate_inbound(&self, inbound: &Inbound) -> Result<()> {
        let frame = &inbound.frame;
        let digest: [u8; 32] = Sha256::digest(&frame.payload).into();
        if frame.stream_id != self.authority.chain_id
            || frame.kind != NovoRudpTransportFrameKindV0::Data
            || frame.session_id != PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1
            || frame.sequence != inbound.original_frame_sequence
            || frame.ack_epoch != 0
            || frame.object_id != u64::from_le_bytes(inbound.object_hash[..8].try_into()?)
            || inbound.payload_sha256 != digest
            || inbound.object_hash != round_wire_object_hash_v1(&frame.payload)
            || inbound.delivery_id
                != product_delivery_id_v1(
                    self.authority.chain_id,
                    "native_seal",
                    inbound.object_hash,
                    digest,
                    &inbound.source_peer_id,
                    &self.local_peer,
                )
        {
            bail!("V3 ingress transport binding mismatch");
        }
        Ok(())
    }
}

fn qc_height(message: &Message) -> Result<u64> {
    Ok(message
        .proposal()
        .context("V3 proposal missing")?
        .subject
        .height)
}

fn certificate_envelope(
    store: &NovNativeBlockSealStoreV1,
    decision: NovNativeSealDecisionCertificateV3,
) -> Result<Message> {
    let subject = &decision.prepare.subject;
    let proposal = store
        .load_proposal(decision.prepare.proposal_hash)?
        .context("V3 archived proposal missing")?;
    let certificate = store
        .load_local_new_view_admission(
            subject.chain_id,
            subject.epoch,
            subject.height,
            subject.round,
        )?
        .map(|a| Box::new(a.certificate));
    Ok(Message::DecisionCertificateV3 {
        proposal: Box::new(proposal),
        decision: Box::new(decision),
        certificate,
    })
}
