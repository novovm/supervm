#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EthWireVersion {
    V66,
    V67,
    V68,
}

impl EthWireVersion {
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::V66 => 66,
            Self::V67 => 67,
            Self::V68 => 68,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V66 => "eth/66",
            Self::V67 => "eth/67",
            Self::V68 => "eth/68",
        }
    }

    #[must_use]
    pub fn parse(raw: u8) -> Option<Self> {
        match raw {
            66 => Some(Self::V66),
            67 => Some(Self::V67),
            68 => Some(Self::V68),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapWireVersion {
    V1,
}

impl SnapWireVersion {
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::V1 => 1,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "snap/1",
        }
    }

    #[must_use]
    pub fn parse(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(Self::V1),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthSyncTransportProfile {
    /// Target final shape: same-level Ethereum native protocol stack
    /// (devp2p/discovery/RLPx/eth,snap wire).
    NativeDevp2pRlpx,
    /// Current production shape in this repository: native runtime bridge
    /// in novovm-network + gateway full-node-only policy.
    NovovmNativeBridge,
}

impl EthSyncTransportProfile {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeDevp2pRlpx => "native_devp2p_rlpx",
            Self::NovovmNativeBridge => "novovm_native_bridge",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeCapabilities {
    pub eth_versions: Vec<EthWireVersion>,
    pub snap_versions: Vec<SnapWireVersion>,
    pub tx_broadcast_enabled: bool,
    pub block_propagation_enabled: bool,
    pub state_sync_enabled: bool,
}

impl EthNativeCapabilities {
    #[must_use]
    pub fn highest_eth_version(&self) -> Option<EthWireVersion> {
        self.eth_versions.iter().copied().max()
    }

    #[must_use]
    pub fn highest_snap_version(&self) -> Option<SnapWireVersion> {
        self.snap_versions.iter().copied().max_by_key(|v| v.as_u8())
    }
}

#[must_use]
pub fn default_eth_native_capabilities() -> EthNativeCapabilities {
    EthNativeCapabilities {
        eth_versions: vec![
            EthWireVersion::V68,
            EthWireVersion::V67,
            EthWireVersion::V66,
        ],
        snap_versions: vec![SnapWireVersion::V1],
        tx_broadcast_enabled: true,
        block_propagation_enabled: true,
        state_sync_enabled: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNegotiatedCapabilities {
    pub eth_version: EthWireVersion,
    pub snap_version: Option<SnapWireVersion>,
}

#[must_use]
pub fn negotiate_eth_native_capabilities(
    local: &EthNativeCapabilities,
    remote_eth_versions: &[u8],
    remote_snap_versions: &[u8],
) -> Option<EthNegotiatedCapabilities> {
    let remote_eth = remote_eth_versions
        .iter()
        .copied()
        .filter_map(EthWireVersion::parse)
        .collect::<Vec<_>>();
    let remote_snap = remote_snap_versions
        .iter()
        .copied()
        .filter_map(SnapWireVersion::parse)
        .collect::<Vec<_>>();

    let mut shared_eth = local
        .eth_versions
        .iter()
        .copied()
        .filter(|v| remote_eth.contains(v))
        .collect::<Vec<_>>();
    if shared_eth.is_empty() {
        return None;
    }
    shared_eth.sort_unstable();
    let eth_version = shared_eth.into_iter().last()?;

    let snap_version = if local.state_sync_enabled {
        let mut shared_snap = local
            .snap_versions
            .iter()
            .copied()
            .filter(|v| remote_snap.contains(v))
            .collect::<Vec<_>>();
        shared_snap.sort_by_key(|v| v.as_u8());
        shared_snap.into_iter().last()
    } else {
        None
    };

    Some(EthNegotiatedCapabilities {
        eth_version,
        snap_version,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeParityProgress {
    pub profile: EthSyncTransportProfile,
    pub full_node_only: bool,
    pub upstream_fallback_disabled: bool,
    pub native_peer_discovery: bool,
    pub native_eth_handshake: bool,
    pub native_snap_sync_state_machine: bool,
    pub state_proof_semantics_closed: bool,
    pub rpc_core_semantics_closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthPeerSessionSnapshot {
    pub chain_id: u64,
    pub peer_id: u64,
    pub negotiated: EthNegotiatedCapabilities,
    pub last_head_height: u64,
}

#[derive(Debug, Clone)]
struct EthPeerSessionState {
    negotiated: EthNegotiatedCapabilities,
    last_head_height: u64,
}

type EthPeerSessionMap = HashMap<u64, HashMap<u64, EthPeerSessionState>>;
static ETH_PEER_SESSIONS: OnceLock<Mutex<EthPeerSessionMap>> = OnceLock::new();
static ETH_NATIVE_SYNC_EVIDENCE: OnceLock<Mutex<HashMap<u64, EthNativeSyncEvidence>>> =
    OnceLock::new();

fn eth_peer_sessions() -> &'static Mutex<EthPeerSessionMap> {
    ETH_PEER_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn eth_native_sync_evidence_map() -> &'static Mutex<HashMap<u64, EthNativeSyncEvidence>> {
    ETH_NATIVE_SYNC_EVIDENCE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeSyncEvidence {
    pub discovery_seen: bool,
    pub rlpx_auth_seen: bool,
    pub rlpx_auth_ack_seen: bool,
    pub hello_seen: bool,
    pub status_seen: bool,
    pub headers_pull_seen: bool,
    pub headers_response_seen: bool,
    pub bodies_pull_seen: bool,
    pub bodies_response_seen: bool,
    pub snap_pull_seen: bool,
    pub snap_response_seen: bool,
}

fn with_eth_native_sync_evidence_mut(chain_id: u64, f: impl FnOnce(&mut EthNativeSyncEvidence)) {
    let mut guard = eth_native_sync_evidence_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = guard.entry(chain_id).or_default();
    f(entry);
}

#[must_use]
pub fn snapshot_eth_native_sync_evidence(chain_id: u64) -> EthNativeSyncEvidence {
    let guard = eth_native_sync_evidence_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get(&chain_id).copied().unwrap_or_default()
}

pub fn observe_eth_native_headers_pull(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.headers_pull_seen = true);
}

pub fn observe_eth_native_headers_response(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.headers_response_seen = true);
}

pub fn observe_eth_native_bodies_pull(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.bodies_pull_seen = true);
}

pub fn observe_eth_native_bodies_response(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.bodies_response_seen = true);
}

pub fn observe_eth_native_snap_pull(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.snap_pull_seen = true);
}

pub fn observe_eth_native_snap_response(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.snap_response_seen = true);
}

pub fn observe_eth_native_discovery(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.discovery_seen = true);
}

pub fn observe_eth_native_rlpx_auth(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.rlpx_auth_seen = true);
}

pub fn observe_eth_native_rlpx_auth_ack(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.rlpx_auth_ack_seen = true);
}

pub fn observe_eth_native_hello(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.hello_seen = true);
}

pub fn observe_eth_native_status(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.status_seen = true);
}

fn native_peer_discovery_ready(chain_id: u64) -> bool {
    snapshot_eth_native_sync_evidence(chain_id).discovery_seen
}

fn native_eth_handshake_ready(chain_id: u64) -> bool {
    let e = snapshot_eth_native_sync_evidence(chain_id);
    e.rlpx_auth_seen && e.rlpx_auth_ack_seen && e.hello_seen && e.status_seen
}

fn native_snap_state_machine_ready(chain_id: u64) -> bool {
    let evidence = snapshot_eth_native_sync_evidence(chain_id);
    let sessions = snapshot_network_runtime_eth_peer_sessions(chain_id);
    let has_snap_peer = sessions.iter().any(|s| s.negotiated.snap_version.is_some());
    has_snap_peer
        && evidence.headers_pull_seen
        && evidence.headers_response_seen
        && evidence.bodies_pull_seen
        && evidence.bodies_response_seen
        && evidence.snap_pull_seen
        && evidence.snap_response_seen
}

pub fn upsert_network_runtime_eth_peer_session(
    chain_id: u64,
    peer_id: u64,
    remote_eth_versions: &[u8],
    remote_snap_versions: &[u8],
    announced_head_height: Option<u64>,
) -> Option<EthNegotiatedCapabilities> {
    let local = default_eth_native_capabilities();
    let negotiated =
        negotiate_eth_native_capabilities(&local, remote_eth_versions, remote_snap_versions)?;
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain.entry(peer_id).or_insert(EthPeerSessionState {
        negotiated: negotiated.clone(),
        last_head_height: 0,
    });
    entry.negotiated = negotiated.clone();
    if let Some(height) = announced_head_height {
        entry.last_head_height = entry.last_head_height.max(height);
    }
    Some(negotiated)
}

pub fn observe_network_runtime_eth_peer_head(chain_id: u64, peer_id: u64, head_height: u64) {
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain.entry(peer_id).or_insert(EthPeerSessionState {
        negotiated: EthNegotiatedCapabilities {
            eth_version: EthWireVersion::V66,
            snap_version: None,
        },
        last_head_height: 0,
    });
    entry.last_head_height = entry.last_head_height.max(head_height);
}

#[must_use]
pub fn snapshot_network_runtime_eth_peer_sessions(chain_id: u64) -> Vec<EthPeerSessionSnapshot> {
    let guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .get(&chain_id)
        .map(|peers| {
            peers
                .iter()
                .map(|(peer_id, state)| EthPeerSessionSnapshot {
                    chain_id,
                    peer_id: *peer_id,
                    negotiated: state.negotiated.clone(),
                    last_head_height: state.last_head_height,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

impl EthNativeParityProgress {
    #[must_use]
    pub fn completed_count(&self) -> u64 {
        let flags = [
            self.full_node_only,
            self.upstream_fallback_disabled,
            self.native_peer_discovery,
            self.native_eth_handshake,
            self.native_snap_sync_state_machine,
            self.state_proof_semantics_closed,
            self.rpc_core_semantics_closed,
        ];
        flags.iter().filter(|v| **v).count() as u64
    }

    #[must_use]
    pub fn total_count(&self) -> u64 {
        7
    }

    #[must_use]
    pub fn progress_pct(&self) -> f64 {
        if self.total_count() == 0 {
            return 0.0;
        }
        (self.completed_count() as f64) * 100.0 / (self.total_count() as f64)
    }
}

#[must_use]
pub fn current_eth_native_parity_progress() -> EthNativeParityProgress {
    EthNativeParityProgress {
        profile: EthSyncTransportProfile::NovovmNativeBridge,
        full_node_only: true,
        upstream_fallback_disabled: true,
        native_peer_discovery: true,
        native_eth_handshake: true,
        native_snap_sync_state_machine: false,
        state_proof_semantics_closed: true,
        rpc_core_semantics_closed: true,
    }
}

#[must_use]
pub fn current_eth_native_parity_progress_for_chain(chain_id: u64) -> EthNativeParityProgress {
    let mut progress = current_eth_native_parity_progress();
    progress.profile = if native_eth_handshake_ready(chain_id) {
        EthSyncTransportProfile::NativeDevp2pRlpx
    } else {
        EthSyncTransportProfile::NovovmNativeBridge
    };
    progress.native_peer_discovery = native_peer_discovery_ready(chain_id);
    progress.native_eth_handshake = native_eth_handshake_ready(chain_id);
    progress.native_snap_sync_state_machine = native_snap_state_machine_ready(chain_id);
    progress
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiate_eth_native_caps_prefers_highest_shared() {
        let local = default_eth_native_capabilities();
        let negotiated =
            negotiate_eth_native_capabilities(&local, &[66, 67], &[1]).expect("must negotiate");
        assert_eq!(negotiated.eth_version, EthWireVersion::V67);
        assert_eq!(negotiated.snap_version, Some(SnapWireVersion::V1));
    }

    #[test]
    fn negotiate_eth_native_caps_none_if_no_eth_intersection() {
        let local = default_eth_native_capabilities();
        let negotiated = negotiate_eth_native_capabilities(&local, &[64, 65], &[1]);
        assert!(negotiated.is_none());
    }

    #[test]
    fn parity_progress_matches_expected_bootstrap_state() {
        let progress = current_eth_native_parity_progress();
        assert_eq!(
            progress.profile,
            EthSyncTransportProfile::NovovmNativeBridge
        );
        assert!(progress.full_node_only);
        assert!(progress.upstream_fallback_disabled);
        assert!(progress.native_peer_discovery);
        assert!(progress.native_eth_handshake);
        assert!(!progress.native_snap_sync_state_machine);
        assert!(progress.progress_pct() > 50.0);
    }

    #[test]
    fn session_snapshot_keeps_max_head_and_caps() {
        let chain_id = 1;
        let peer_id = 42;
        let negotiated =
            upsert_network_runtime_eth_peer_session(chain_id, peer_id, &[66, 68], &[1], Some(128))
                .expect("negotiated");
        assert_eq!(negotiated.eth_version, EthWireVersion::V68);
        observe_network_runtime_eth_peer_head(chain_id, peer_id, 256);
        let snapshots = snapshot_network_runtime_eth_peer_sessions(chain_id);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].last_head_height, 256);
        assert_eq!(snapshots[0].negotiated.eth_version, EthWireVersion::V68);
    }

    #[test]
    fn parity_progress_for_chain_marks_native_snap_state_machine_ready_after_full_cycle() {
        let chain_id = 99_160_316_u64;
        let peer_id = 88_u64;
        let _ = upsert_network_runtime_eth_peer_session(chain_id, peer_id, &[66, 68], &[1], None)
            .expect("negotiated");

        observe_eth_native_headers_pull(chain_id);
        observe_eth_native_headers_response(chain_id);
        observe_eth_native_bodies_pull(chain_id);
        observe_eth_native_bodies_response(chain_id);
        observe_eth_native_snap_pull(chain_id);
        observe_eth_native_snap_response(chain_id);

        let progress = current_eth_native_parity_progress_for_chain(chain_id);
        assert!(progress.native_snap_sync_state_machine);
    }

    #[test]
    fn parity_progress_for_chain_marks_native_rlpx_profile_after_handshake_cycle() {
        let chain_id = 99_160_317_u64;
        observe_eth_native_discovery(chain_id);
        observe_eth_native_rlpx_auth(chain_id);
        observe_eth_native_rlpx_auth_ack(chain_id);
        observe_eth_native_hello(chain_id);
        observe_eth_native_status(chain_id);
        let progress = current_eth_native_parity_progress_for_chain(chain_id);
        assert_eq!(progress.profile, EthSyncTransportProfile::NativeDevp2pRlpx);
        assert!(progress.native_peer_discovery);
        assert!(progress.native_eth_handshake);
    }
}
