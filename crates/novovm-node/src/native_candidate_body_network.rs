//! Bounded, single-height integration with authenticated Product Overlay events.
//! Transport queue acceptance is not application delivery or durable execution.
use super::*;
use crate::native_block_seal::round_wire::{
    is_nov_native_seal_round_wire_v1, round_wire_object_hash_v1,
};
use crate::product_mainline_overlay::{
    ProductMainlineOverlayInboundV1 as Inbound, ProductMainlineOverlayPayloadClassV1 as Class,
    ProductMainlineOverlayRuntimeV1 as Runtime,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const MAX_ACTIVE: usize = 4;
const LIFETIME: Duration = Duration::from_secs(30);
const PER_SECOND: usize = 64;
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
const SEND_INTERVAL: Duration = Duration::from_millis(20);

fn object_hash(bytes: &[u8]) -> [u8; 32] {
    if is_nov_native_seal_round_wire_v1(bytes) {
        return round_wire_object_hash_v1(bytes);
    }
    let mut hash = Sha256::new();
    hash.update(b"novovm-candidate-body-chunk-object-v1\0");
    hash.update(bytes);
    hash.finalize().into()
}

/// Repeated bounded transfer attempts while the caller awaits confirmation.
/// Keep polling until the lifecycle retires this sender; true only reports a
/// locally queued attempt, NOT recipient acceptance. No new signatures or IDs.
pub struct CandidateBodySenderV1 {
    chain: u64,
    source: String,
    target: String,
    frames: Vec<Vec<u8>>,
    next: usize,
    last_seen: Option<Instant>,
    next_send: Option<Instant>,
    retry_at: Option<Instant>,
}

impl CandidateBodySenderV1 {
    pub fn new(
        wire: Vec<u8>,
        raws: &[Vec<u8>],
        authority: &NovNativeSealEpochAuthorityV1,
        height: u64,
        source: &str,
        target: &str,
    ) -> Result<Self> {
        if source == target || authority.validator_for_transport_peer(target).is_none() {
            bail!("body transfer target is not a distinct pinned validator");
        }
        let body = CandidateBodyAssemblerV1::new(&wire, authority, height, source)?;
        let mut frames = vec![wire];
        frames.extend(body.encode_chunks(raws)?);
        Ok(Self {
            chain: authority.chain_id,
            source: source.into(),
            target: target.into(),
            frames,
            next: 0,
            last_seen: None,
            next_send: None,
            retry_at: None,
        })
    }

    /// At most one frame per call; backpressure leaves its position unchanged.
    pub fn poll(&mut self, runtime: &Runtime) -> Result<bool> {
        self.poll_at(runtime, Instant::now())
    }

    pub fn poll_at(&mut self, runtime: &Runtime, now: Instant) -> Result<bool> {
        if runtime.chain_id() != self.chain || runtime.startup().local_peer_id != self.source {
            bail!("body sender runtime identity changed");
        }
        if self.last_seen.is_some_and(|previous| now < previous) {
            bail!("body sender monotonic clock moved backwards");
        }
        self.last_seen = Some(now);
        if self.next == self.frames.len() {
            if self.retry_at.is_some_and(|retry| now < retry) {
                return Ok(true);
            }
            self.next = 0;
            self.retry_at = None;
        }
        if self.next_send.is_some_and(|next| now < next) {
            return Ok(false);
        }
        self.next_send = Some(now + SEND_INTERVAL);
        if let Some(frame) = self.frames.get(self.next) {
            if runtime.try_submit_to_peer(
                &self.target,
                Class::NativeSeal,
                object_hash(frame),
                frame.clone(),
            )? {
                self.next += 1;
                if self.next == self.frames.len() {
                    self.retry_at = Some(now + RETRY_INTERVAL);
                }
            }
        }
        Ok(self.next == self.frames.len())
    }
}

struct Entry {
    body: Option<CandidateBodyAssemblerV1>,
    expires: Instant,
    source: String,
}

pub struct CandidateBodyInboxV1 {
    authority: NovNativeSealEpochAuthorityV1,
    height: u64,
    entries: BTreeMap<[u8; 32], Entry>,
    budgets: BTreeMap<String, (Instant, usize)>,
    last_seen: Instant,
}

impl CandidateBodyInboxV1 {
    pub fn new(
        authority: NovNativeSealEpochAuthorityV1,
        height: u64,
        now: Instant,
    ) -> Result<Self> {
        authority.validate()?;
        if height < authority.activation_height {
            bail!("body inbox height precedes authority");
        }
        let budgets = authority
            .transport_bindings
            .iter()
            .map(|b| (b.transport_peer_id.clone(), (now, 0)))
            .collect();
        Ok(Self {
            authority,
            height,
            entries: BTreeMap::new(),
            budgets,
            last_seen: now,
        })
    }

    /// Fixed deadline is not extended by duplicate manifests or chunks.
    pub fn expire(&mut self, now: Instant) -> Result<usize> {
        if now < self.last_seen {
            bail!("body inbox monotonic clock moved backwards");
        }
        self.last_seen = now;
        let before = self.entries.len();
        self.entries.retain(|_, entry| now < entry.expires);
        Ok(before - self.entries.len())
    }

    /// Only call with the real runtime's authenticated Inbound event. No ACK is
    /// emitted: completion here is volatile, not durable application acceptance.
    pub fn accept(
        &mut self,
        inbound: &Inbound,
        now: Instant,
    ) -> Result<Option<VerifiedCandidateBodyV1>> {
        self.expire(now)?;
        if inbound.payload_class != Class::NativeSeal
            || inbound.frame.stream_id != self.authority.chain_id
        {
            bail!("body inbox transport domain mismatch");
        }
        let packet = &inbound.frame.payload;
        if packet.len() > crate::product_mainline_overlay::PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1 {
            bail!("body inbox frame exceeds transport limit");
        }
        let budget = self
            .budgets
            .get_mut(&inbound.source_peer_id)
            .context("body inbox source is unbound")?;
        if now.duration_since(budget.0) >= Duration::from_secs(1) {
            *budget = (now, 0);
        }
        if budget.1 >= PER_SECOND {
            bail!("body inbox source budget exceeded");
        }
        budget.1 += 1;
        if object_hash(packet) != inbound.object_hash {
            bail!("body inbox transport object mismatch");
        }
        if is_nov_native_seal_round_wire_v1(packet) {
            let body = CandidateBodyAssemblerV1::new(
                packet,
                &self.authority,
                self.height,
                &inbound.source_peer_id,
            )?;
            let hash = body
                .message
                .proposal()
                .context("body proposal missing")?
                .proposal_hash;
            if self.entries.contains_key(&hash) {
                return Ok(None);
            }
            if self.entries.len() >= MAX_ACTIVE {
                bail!("body inbox candidate budget exhausted");
            }
            self.entries.insert(
                hash,
                Entry {
                    body: Some(body),
                    expires: now + LIFETIME,
                    source: inbound.source_peer_id.clone(),
                },
            );
            return Ok(None);
        }
        if packet.len() <= HEADER || !packet.starts_with(MAGIC) {
            bail!("unknown body fragment");
        }
        let hash: [u8; 32] = packet[8..40].try_into()?;
        let entry = self
            .entries
            .get_mut(&hash)
            .context("body fragment has no active signed manifest")?;
        if entry.source != inbound.source_peer_id {
            bail!("body fragment source differs from manifest");
        }
        let Some(body) = entry.body.as_mut() else {
            return Ok(None);
        };
        let result = body.push(&inbound.source_peer_id, packet);
        if result.is_err() || matches!(result, Ok(Some(_))) {
            entry.body = None;
        }
        result
    }
}
