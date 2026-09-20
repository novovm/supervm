//! Explicit opt-in bridge from a single-height driver to authenticated Product
//! Overlay events. Does not drain unrelated events, own keys, activate a service,
//! acknowledge volatile ingress as durable, or promote a candidate to finality.

use super::round_driver::{
    NovNativeSealRoundDriverPhaseV1, NovNativeSealRoundDriverStatusV1, NovNativeSealRoundDriverV1,
};
use super::round_wire::{
    decode_nov_native_seal_round_wire_v1, encode_nov_native_seal_round_wire_v1,
    round_wire_object_hash_v1,
};
use super::{NovNativeBlockSealStoreV1, NovNativeSealQuorumCertificateV1};
use crate::native_block_ledger::NovNativeBlockLedgerV1;
use crate::product_delivery_journal::product_delivery_id_v1;
use crate::product_mainline_overlay::{
    ProductMainlineOverlayInboundV1, ProductMainlineOverlayPayloadClassV1,
    ProductMainlineOverlayRoleV1, ProductMainlineOverlayRuntimeV1,
    PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
};
use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::NovoRudpTransportFrameKindV0;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

/// Re-emission is intentional: driver ingress is volatile and an interrupted
/// recipient must recollect evidence. This is NOT a durable-delivery ACK.
pub const NOV_NATIVE_SEAL_ROUND_RETRY_INTERVAL_V1: Duration = Duration::from_millis(250);
const MAX_OUTPUT_MESSAGES: usize = 8;

pub struct NovNativeSealRoundOverlayV1 {
    driver: NovNativeSealRoundDriverV1,
    local_peer_id: String,
    peers: BTreeSet<String>,
    // At most MAX_OUTPUT_MESSAGES * (64 - 1) entries. No payloads or historical
    // rounds accumulate here; each poll retains only the current output set.
    attempted: BTreeMap<([u8; 32], String), Instant>,
    next_send: usize,
    received_round: u64,
    received: BTreeMap<String, VecDeque<[u8; 32]>>,
    halted: bool,
}

impl NovNativeSealRoundOverlayV1 {
    pub fn attach(
        driver: NovNativeSealRoundDriverV1,
        runtime: &ProductMainlineOverlayRuntimeV1,
    ) -> Result<Self> {
        let local_peer_id = driver
            .authority()
            .transport_peer_id(driver.local_validator_id())?
            .to_owned();
        let peers = driver
            .authority()
            .transport_bindings
            .iter()
            .filter(|binding| binding.validator_id != driver.local_validator_id())
            .map(|binding| binding.transport_peer_id.clone())
            .collect();
        let received_round = driver.status().round;
        let bridge = Self {
            driver,
            local_peer_id,
            peers,
            attempted: BTreeMap::new(),
            next_send: 0,
            received_round,
            received: BTreeMap::new(),
            halted: false,
        };
        bridge.check_runtime(runtime)?;
        Ok(bridge)
    }

    fn check_runtime(&self, runtime: &ProductMainlineOverlayRuntimeV1) -> Result<()> {
        if self.halted {
            bail!("native seal round overlay halted; inspect and reopen");
        }
        if runtime.role() != ProductMainlineOverlayRoleV1::Duplex
            || runtime.chain_id() != self.driver.authority().chain_id
            || runtime.startup().local_peer_id != self.local_peer_id
            || runtime
                .remote_peer_ids()
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                != self.peers
            || runtime.remote_peer_ids().len() != self.peers.len()
            || self.peers.is_empty()
        {
            bail!("native seal round overlay requires the pinned chain, signer transport identity and duplex validator mesh");
        }
        Ok(())
    }

    /// Call only with an Inbound event produced by the authenticated Overlay.
    /// The public event type is not an authentication token: constructing it from
    /// arbitrary RPC fields does NOT authenticate a sender. No signatures or DB
    /// writes are made here and no JournalPersisted ACK is emitted.
    pub fn ingest(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        runtime: &ProductMainlineOverlayRuntimeV1,
        inbound: &ProductMainlineOverlayInboundV1,
    ) -> Result<bool> {
        self.check_runtime(runtime)?;
        if inbound.payload_class != ProductMainlineOverlayPayloadClassV1::NativeSeal {
            return Ok(false);
        }
        if inbound.frame.stream_id != self.driver.authority().chain_id
            || inbound.frame.kind != NovoRudpTransportFrameKindV0::Data
            || inbound.frame.session_id != PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1
            || inbound.frame.sequence != inbound.original_frame_sequence
            || inbound.frame.ack_epoch != 0
            || inbound.frame.object_id != u64::from_le_bytes(inbound.object_hash[..8].try_into()?)
            || !self.peers.contains(&inbound.source_peer_id)
        {
            bail!("native seal round ingress is outside the pinned transport domain");
        }
        let wire = &inbound.frame.payload;
        if wire.len() > super::round_wire::NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1 {
            bail!("native seal round ingress exceeds its transport bound");
        }
        let payload_sha256: [u8; 32] = Sha256::digest(wire).into();
        if inbound.payload_sha256 != payload_sha256
            || inbound.object_hash != round_wire_object_hash_v1(wire)
            || inbound.delivery_id
                != product_delivery_id_v1(
                    runtime.chain_id(),
                    "native_seal",
                    inbound.object_hash,
                    payload_sha256,
                    &inbound.source_peer_id,
                    &self.local_peer_id,
                )
        {
            bail!("native seal round ingress payload/delivery binding mismatch");
        }
        let round = self.driver.status().round;
        if self.received_round != round {
            self.received.clear();
            self.received_round = round;
        }
        if self
            .received
            .get(&inbound.source_peer_id)
            .is_some_and(|hashes| hashes.contains(&inbound.object_hash))
        {
            return Ok(false);
        }
        let message = decode_nov_native_seal_round_wire_v1(
            wire,
            self.driver.authority(),
            self.driver.status().height,
            &inbound.source_peer_id,
        )?;
        let cacheable = message.round() <= round;
        let accepted =
            self.driver
                .ingest_authenticated(ledger, store, &inbound.source_peer_id, message)?;
        // At most eight exact previously verified frames per authenticated
        // peer; no global eviction by one faulty validator. Never cache errors
        // or future-round messages; clear on each local round transition.
        if cacheable {
            let hashes = self
                .received
                .entry(inbound.source_peer_id.clone())
                .or_default();
            if hashes.len() == MAX_OUTPUT_MESSAGES {
                hashes.pop_front();
            }
            hashes.push_back(inbound.object_hash);
        }
        Ok(accepted)
    }

    /// Run from the local lifecycle loop, after separately dispatching events.
    /// Returns the number of recipient queue admissions, NOT deliveries or votes.
    /// A full peer queue is retried independently, without blocking other peers.
    pub fn poll(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        key: &SigningKey,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<usize> {
        let result = self.poll_inner(ledger, store, key, runtime, now);
        if result.is_err() {
            self.halted = true;
        }
        result
    }

    fn poll_inner(
        &mut self,
        ledger: &NovNativeBlockLedgerV1,
        store: &NovNativeBlockSealStoreV1,
        key: &SigningKey,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<usize> {
        self.check_runtime(runtime)?;
        let messages = self.driver.poll(ledger, store, key, now)?;
        if messages.len() > MAX_OUTPUT_MESSAGES {
            bail!("native seal round output exceeds its per-poll message budget");
        }
        // Encode and validate the entire batch before enqueueing any of it.
        let mut frames = Vec::with_capacity(messages.len());
        let mut current = BTreeSet::new();
        for message in messages {
            let wire = encode_nov_native_seal_round_wire_v1(
                &message,
                self.driver.authority(),
                self.driver.status().height,
                &self.local_peer_id,
            )?;
            let hash = round_wire_object_hash_v1(&wire);
            if current.insert(hash) {
                frames.push((hash, wire));
            }
        }
        self.attempted.retain(|(hash, _), _| current.contains(hash));
        submit_frames(
            &frames,
            &self.peers,
            &mut self.attempted,
            &mut self.next_send,
            now,
            |peer, hash, wire| {
                runtime.try_submit_to_peer(
                    peer,
                    ProductMainlineOverlayPayloadClassV1::NativeSeal,
                    hash,
                    wire,
                )
            },
        )
    }

    pub fn status(&self) -> NovNativeSealRoundDriverStatusV1 {
        let mut status = self.driver.status();
        if self.halted {
            status.phase = NovNativeSealRoundDriverPhaseV1::Halted;
            status.prepared = false;
            status.qc_hash = None;
        }
        status
    }

    pub fn prepared_qc(&self) -> Option<&NovNativeSealQuorumCertificateV1> {
        if self.halted {
            None
        } else {
            self.driver.prepared_qc()
        }
    }
}

fn submit_frames(
    frames: &[([u8; 32], Vec<u8>)],
    peers: &BTreeSet<String>,
    attempted: &mut BTreeMap<([u8; 32], String), Instant>,
    next_send: &mut usize,
    now: Instant,
    mut submit: impl FnMut(&str, [u8; 32], Vec<u8>) -> Result<bool>,
) -> Result<usize> {
    let mut queued = 0;
    let peers = peers.iter().collect::<Vec<_>>();
    let work_count = frames.len() * peers.len();
    let start = *next_send;
    for offset in 0..work_count {
        let index = (start + offset) % work_count;
        let (hash, wire) = &frames[index / peers.len()];
        let peer = peers[index % peers.len()];
        let binding = (*hash, peer.clone());
        if let Some(previous) = attempted.get(&binding) {
            if now
                .checked_duration_since(*previous)
                .context("native seal round retry clock moved backwards")?
                < NOV_NATIVE_SEAL_ROUND_RETRY_INTERVAL_V1
            {
                continue;
            }
        }
        let accepted = submit(peer, *hash, wire.clone())?;
        if accepted {
            attempted.insert(binding, now);
            *next_send = (index + 1) % work_count;
            queued += 1;
        }
    }
    Ok(queued)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_seal_round_overlay_tiny_queue_is_fair_across_messages_and_peers() {
        let frames = vec![([1; 32], vec![1]), ([2; 32], vec![2])];
        let peers = ["a", "b", "c"].map(str::to_owned).into_iter().collect();
        for step in [Duration::from_millis(10), Duration::from_millis(300)] {
            let mut attempted = BTreeMap::new();
            let mut next = 0;
            let mut accepted = BTreeSet::new();
            let start = Instant::now();
            for tick in 0..6 {
                let mut capacity = 1;
                let count = submit_frames(
                    &frames,
                    &peers,
                    &mut attempted,
                    &mut next,
                    start + step * tick,
                    |peer, hash, _| {
                        if capacity == 0 {
                            return Ok(false);
                        }
                        capacity -= 1;
                        assert!(
                            accepted.insert((hash, peer.to_owned())),
                            "repeated one target before serving other pending targets"
                        );
                        Ok(true)
                    },
                )
                .unwrap();
                assert_eq!(count, 1);
            }
            assert_eq!(accepted.len(), 6);
            assert_eq!(attempted.len(), 6);
        }
    }

    #[test]
    fn native_seal_round_overlay_full_peer_does_not_block_others_or_claim_delivery() {
        let frames = vec![([1; 32], vec![1])];
        let peers = ["offline", "online"]
            .map(str::to_owned)
            .into_iter()
            .collect();
        let mut attempted = BTreeMap::new();
        let mut next = 0;
        let now = Instant::now();
        assert_eq!(
            submit_frames(
                &frames,
                &peers,
                &mut attempted,
                &mut next,
                now,
                |peer, _, _| Ok(peer == "online")
            )
            .unwrap(),
            1
        );
        assert!(!attempted.contains_key(&([1; 32], "offline".to_owned())));
        assert_eq!(
            submit_frames(
                &frames,
                &peers,
                &mut attempted,
                &mut next,
                now,
                |peer, _, _| {
                    assert_eq!(peer, "offline");
                    Ok(false)
                }
            )
            .unwrap(),
            0
        );
        // A queue error is not converted to success or a recipient ACK.
        assert!(submit_frames(
            &frames,
            &peers,
            &mut attempted,
            &mut next,
            now,
            |_, _, _| bail!("worker gone")
        )
        .is_err());
    }
}
