//! Opt-in main-node lifecycle for one explicitly pinned, already executed candidate.
//! Staging is bounded and does not sign. Local poll is the sole signing scheduler.
use super::round_driver::NovNativeSealRoundDriverV1;
use super::round_overlay::NovNativeSealRoundOverlayV1;
use super::round_wire::{
    is_nov_native_seal_round_wire_v1, NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1,
};
use super::service_config::NovNativeSealServiceConfigV1;
use super::service_paths::validate_service_paths_v1;
use super::NovNativeBlockSealStoreV1;
use crate::native_block_ledger::NovNativeBlockLedgerV1;
use crate::product_mainline_overlay::{
    ProductMainlineOverlayInboundV1, ProductMainlineOverlayPayloadClassV1,
    ProductMainlineOverlayRoleV1, ProductMainlineOverlayRuntimeV1,
};
use anyhow::{bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const PER_PEER_QUEUE: usize = 4;

/// Pure selection: no key or database access when disabled. A stray config is
/// an error, not an implicit permission to sign. No RPC can enable this service.
pub fn native_seal_config_selection_v1(
    enabled: Option<&str>,
    config: Option<&str>,
    native_tick_mode: bool,
    overlay_enabled: bool,
) -> Result<Option<PathBuf>> {
    let enabled = match enabled.map(str::trim) {
        None | Some("0" | "false") => false,
        Some("1" | "true") => true,
        Some(_) => bail!("NOVOVM_NATIVE_SEAL_ENABLED must be explicitly 0/false or 1/true"),
    };
    if !enabled {
        if config.is_some() {
            bail!("native seal config was provided without explicit enablement");
        }
        return Ok(None);
    }
    if !native_tick_mode || !overlay_enabled {
        bail!("native seal requires native execution tick mode and Product Overlay");
    }
    let path = config
        .filter(|s| !s.trim().is_empty())
        .context("NOVOVM_NATIVE_SEAL_CONFIG is required")?;
    Ok(Some(PathBuf::from(path)))
}

struct PeerInbox {
    pending: VecDeque<ProductMainlineOverlayInboundV1>,
    admitted_at: VecDeque<Instant>,
}

pub struct NovNativeSealServiceV1 {
    config: NovNativeSealServiceConfigV1,
    ledger: NovNativeBlockLedgerV1,
    store: NovNativeBlockSealStoreV1,
    bridge: NovNativeSealRoundOverlayV1,
    inbox: BTreeMap<String, PeerInbox>,
    next_peer: usize,
    last_seen: Instant,
    last_poll: Option<Instant>,
    halted: bool,
    last_error: Option<&'static str>,
    accepted: u64,
    rejected: u64,
    dropped: u64,
    processed: u64,
    sent: u64,
}

impl NovNativeSealServiceV1 {
    pub fn open(
        config: NovNativeSealServiceConfigV1,
        ledger_path: &Path,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<Self> {
        config.validate(runtime.chain_id())?;
        validate_service_paths_v1(&config, ledger_path, &[], &[])?;
        check_runtime(&config, runtime)?;
        // Read-only existence/domain/candidate checks before creating the seal DB.
        let probe = NovNativeBlockLedgerV1::open_existing_read_only(ledger_path)?
            .context("native seal requires an existing AOEM-owned candidate ledger")?;
        config.authority.validate_against_ledger(&probe)?;
        let (_, block) = probe
            .load_seal_eligible_local_candidate_v1(config.chain_id, config.block_hash)
            .context("native seal pinned candidate is not locally eligible; it is not fetched or fabricated")?;
        if block.header.height != config.height {
            bail!("native seal candidate height mismatch");
        }
        drop(probe);
        // Join the process-shared live ledger handle, never retain a detached
        // read-only snapshot while the main execution owner updates its ledger.
        // The service invokes no ledger mutation methods.
        let ledger = NovNativeBlockLedgerV1::open(ledger_path)?;
        let store = NovNativeBlockSealStoreV1::open(&config.seal_store_path)?;
        let driver = NovNativeSealRoundDriverV1::open(
            &ledger,
            &store,
            config.authority.clone(),
            config.block_hash,
            config.justify_qc_hash,
            config.local_validator_id,
            now,
            config.round_timeout,
        )?;
        let bridge = NovNativeSealRoundOverlayV1::attach(driver, runtime)?;
        let inbox = runtime
            .remote_peer_ids()
            .iter()
            .map(|peer| {
                (
                    peer.clone(),
                    PeerInbox {
                        pending: VecDeque::new(),
                        admitted_at: VecDeque::new(),
                    },
                )
            })
            .collect();
        Ok(Self {
            config,
            ledger,
            store,
            bridge,
            inbox,
            next_peer: 0,
            last_seen: now,
            last_poll: None,
            halted: false,
            last_error: None,
            accepted: 0,
            rejected: 0,
            dropped: 0,
            processed: 0,
            sent: 0,
        })
    }

    /// Accept only the authenticated runtime's Inbound event. Cheap staging only;
    /// no message can make this call create a local signature or persistent ACK.
    pub fn enqueue(&mut self, inbound: ProductMainlineOverlayInboundV1) -> bool {
        let valid = !self.halted
            && inbound.payload_class == ProductMainlineOverlayPayloadClassV1::NativeSeal
            && inbound.frame.stream_id == self.config.chain_id
            && inbound.frame.payload.len() <= NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1
            && is_nov_native_seal_round_wire_v1(&inbound.frame.payload);
        if valid {
            if let Some(peer) = self.inbox.get_mut(&inbound.source_peer_id) {
                if peer.pending.len() < PER_PEER_QUEUE {
                    peer.pending.push_back(inbound);
                    return true;
                }
            }
        }
        self.dropped = self.dropped.saturating_add(1);
        false
    }

    pub fn poll(&mut self, runtime: &ProductMainlineOverlayRuntimeV1, now: Instant) -> Result<()> {
        if self.halted {
            bail!("native seal service is halted; inspect and restart explicitly");
        }
        let result = self.poll_inner(runtime, now);
        if result.is_err() {
            self.halt("local_poll_fault");
        }
        result
    }

    fn poll_inner(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        check_runtime(&self.config, runtime)?;
        if now < self.last_seen {
            bail!("native seal service monotonic clock moved backwards");
        }
        self.last_seen = now;
        if self
            .last_poll
            .is_some_and(|last| now.duration_since(last) < self.config.poll_interval)
        {
            return Ok(());
        }
        self.last_poll = Some(now);
        let peers = self.inbox.keys().cloned().collect::<Vec<_>>();
        let mut budget = self.config.ingress_per_poll;
        // Round robin across pinned sources; one faulty source cannot occupy the
        // entire verifier budget. A sliding one-second window counts rejects too.
        for _ in 0..PER_PEER_QUEUE {
            for offset in 0..peers.len() {
                if budget == 0 {
                    break;
                }
                let index = (self.next_peer + offset) % peers.len();
                let peer = self.inbox.get_mut(&peers[index]).expect("pinned peer");
                while peer
                    .admitted_at
                    .front()
                    .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1))
                {
                    peer.admitted_at.pop_front();
                }
                if peer.admitted_at.len() >= self.config.ingress_per_source_per_second {
                    continue;
                }
                let Some(inbound) = peer.pending.pop_front() else {
                    continue;
                };
                peer.admitted_at.push_back(now);
                budget -= 1;
                self.processed = self.processed.saturating_add(1);
                match self
                    .bridge
                    .ingest(&self.ledger, &self.store, runtime, &inbound)
                {
                    Ok(true) => self.accepted = self.accepted.saturating_add(1),
                    Ok(false) => (),
                    Err(_) => self.rejected = self.rejected.saturating_add(1),
                }
            }
            if budget == 0 {
                break;
            }
        }
        self.next_peer = (self.next_peer + 1) % peers.len();
        // Always recheck local durable ownership, even after malformed input.
        // A local store error cannot be indefinitely disguised as peer rejects.
        let sent =
            self.bridge
                .poll(&self.ledger, &self.store, &self.config.signer, runtime, now)?;
        self.sent = self.sent.saturating_add(sent as u64);
        Ok(())
    }

    pub fn halt(&mut self, reason: &'static str) {
        self.halted = true;
        self.last_error = Some(reason);
        for peer in self.inbox.values_mut() {
            peer.pending.clear();
        }
    }

    pub fn halted(&self) -> bool {
        self.halted
    }

    pub fn status_json(&self) -> serde_json::Value {
        let status = self.bridge.status();
        serde_json::json!({
            "enabled": true, "ok": !self.halted, "halted": self.halted,
            "chain_id": self.config.chain_id,
            "block_hash": super::hex_v1(&self.config.block_hash),
            "local_validator_id": super::hex_v1(&self.config.local_validator_id),
            "scope": "single_height_prepare_only", "height": status.height, "round": status.round,
            "phase": if self.halted { "Halted".to_string() } else { format!("{:?}", status.phase) },
            "prepared": !self.halted && status.prepared,
            "qc_hash": if self.halted { None } else { status.qc_hash },
            "finalized": false, "safe": false, "proof_sealed": false, "chain_canonical": false,
            "queued_ingress": self.inbox.values().map(|peer| peer.pending.len()).sum::<usize>(),
            "accepted_ingress": self.accepted, "rejected_ingress": self.rejected,
            "dropped_ingress": self.dropped, "processed_ingress": self.processed,
            "queued_egress": self.sent, "last_error": self.last_error,
            "recipient_durable_ack_emitted": false,
        })
    }
}

fn check_runtime(
    config: &NovNativeSealServiceConfigV1,
    runtime: &ProductMainlineOverlayRuntimeV1,
) -> Result<()> {
    let expected: BTreeSet<_> = config
        .authority
        .transport_bindings
        .iter()
        .filter(|binding| binding.validator_id != config.local_validator_id)
        .map(|binding| binding.transport_peer_id.as_str())
        .collect();
    if runtime.chain_id() != config.chain_id
        || runtime.role() != ProductMainlineOverlayRoleV1::Duplex
        || runtime.startup().local_peer_id
            != config
                .authority
                .transport_peer_id(config.local_validator_id)?
        || runtime
            .remote_peer_ids()
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != expected
        || expected.len() != runtime.remote_peer_ids().len()
        || expected.is_empty()
    {
        bail!("native seal service transport configuration mismatch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_seal_service_opt_in_is_explicit_and_rejects_wrong_mode() {
        assert!(native_seal_config_selection_v1(None, None, false, false)
            .unwrap()
            .is_none());
        for enabled in [None, Some("false"), Some("yes"), Some("")] {
            assert!(
                native_seal_config_selection_v1(enabled, Some("seal.json"), true, true).is_err()
            );
        }
        assert!(native_seal_config_selection_v1(Some("1"), None, true, true).is_err());
        assert!(
            native_seal_config_selection_v1(Some("1"), Some("seal.json"), false, true).is_err()
        );
        assert!(
            native_seal_config_selection_v1(Some("1"), Some("seal.json"), true, false).is_err()
        );
        assert_eq!(
            native_seal_config_selection_v1(Some("true"), Some("seal.json"), true, true).unwrap(),
            Some(PathBuf::from("seal.json"))
        );
    }
}
