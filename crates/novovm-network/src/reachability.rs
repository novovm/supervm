use serde::{Deserialize, Serialize};

use crate::routing::{L4LocalRoutingTable, L4PeerRef, Reachability, RoutingSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatingPortMode {
    Fixed,
    EphemeralAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointScope {
    Public,
    Private,
    Loopback,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReachabilityProbeStatus {
    DirectReachable,
    LanReachable,
    RelayOnly,
    Unreachable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReachabilityProbeInput {
    pub peer_id: String,
    pub configured_addr_hint: Option<String>,
    pub observed_addr: Option<String>,
    pub local_bind_addr: Option<String>,
    pub floating_port_mode: FloatingPortMode,
    pub direct_probe_sent: bool,
    pub direct_probe_ack: bool,
    pub relay_available: bool,
    pub rtt_ms: Option<u32>,
    pub observed_unix_ms: u64,
    pub source: RoutingSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReachabilityProbeDecision {
    pub peer_id: String,
    pub addr_hint: Option<String>,
    pub observed_addr: Option<String>,
    pub endpoint_scope: EndpointScope,
    pub floating_port_active: bool,
    pub status: ReachabilityProbeStatus,
    pub reachability: Reachability,
    pub rtt_ms: Option<u32>,
    pub source: RoutingSource,
}

pub fn decide_reachability_probe_v0(input: ReachabilityProbeInput) -> ReachabilityProbeDecision {
    let endpoint_scope = input
        .observed_addr
        .as_deref()
        .or(input.configured_addr_hint.as_deref())
        .map(classify_endpoint_scope_v0)
        .unwrap_or(EndpointScope::Unknown);
    let floating_port_active = floating_port_active_v0(
        input.configured_addr_hint.as_deref(),
        input.observed_addr.as_deref(),
        input.local_bind_addr.as_deref(),
        input.floating_port_mode,
    );

    let status = if input.direct_probe_ack {
        if endpoint_scope == EndpointScope::Private || endpoint_scope == EndpointScope::Loopback {
            ReachabilityProbeStatus::LanReachable
        } else {
            ReachabilityProbeStatus::DirectReachable
        }
    } else if input.direct_probe_sent && input.relay_available {
        ReachabilityProbeStatus::RelayOnly
    } else if input.direct_probe_sent {
        ReachabilityProbeStatus::Unreachable
    } else {
        ReachabilityProbeStatus::Unknown
    };
    let reachability = match status {
        ReachabilityProbeStatus::DirectReachable => Reachability::Reachable,
        ReachabilityProbeStatus::LanReachable => Reachability::LanOnly,
        ReachabilityProbeStatus::RelayOnly => Reachability::RelayOnly,
        ReachabilityProbeStatus::Unreachable => Reachability::Unreachable,
        ReachabilityProbeStatus::Unknown => Reachability::Unknown,
    };
    let addr_hint = input
        .observed_addr
        .clone()
        .or(input.configured_addr_hint.clone());

    ReachabilityProbeDecision {
        peer_id: input.peer_id,
        addr_hint,
        observed_addr: input.observed_addr,
        endpoint_scope,
        floating_port_active,
        status,
        reachability,
        rtt_ms: input.rtt_ms,
        source: input.source,
    }
}

pub fn apply_reachability_probe_v0(
    table: &L4LocalRoutingTable,
    input: ReachabilityProbeInput,
) -> ReachabilityProbeDecision {
    let observed_unix_ms = input.observed_unix_ms;
    let decision = decide_reachability_probe_v0(input);
    let mut peer = table
        .get_peer(&decision.peer_id)
        .unwrap_or_else(|| L4PeerRef::new(decision.peer_id.clone()));
    peer.addr_hint = decision.addr_hint.clone();
    peer.reachability = decision.reachability;
    peer.latency_ms = decision.rtt_ms;
    peer.source = decision.source;
    peer.last_seen_unix_ms = observed_unix_ms;
    table.upsert_peer(peer);
    decision
}

pub fn classify_endpoint_scope_v0(addr: &str) -> EndpointScope {
    let host = addr
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(addr)
        .trim_matches(['[', ']']);
    if host.eq_ignore_ascii_case("localhost") {
        return EndpointScope::Loopback;
    }
    let Ok(ip) = host.parse::<std::net::IpAddr>() else {
        return EndpointScope::Unknown;
    };
    match ip {
        std::net::IpAddr::V4(v4) if v4.is_loopback() => EndpointScope::Loopback,
        std::net::IpAddr::V6(v6) if v6.is_loopback() => EndpointScope::Loopback,
        std::net::IpAddr::V4(v4)
            if v4.is_private() || v4.is_link_local() || v4.is_unspecified() =>
        {
            EndpointScope::Private
        }
        std::net::IpAddr::V6(v6) if v6.is_unspecified() => EndpointScope::Private,
        _ => EndpointScope::Public,
    }
}

pub fn floating_port_active_v0(
    configured_addr_hint: Option<&str>,
    observed_addr: Option<&str>,
    local_bind_addr: Option<&str>,
    mode: FloatingPortMode,
) -> bool {
    if mode != FloatingPortMode::EphemeralAllowed {
        return false;
    }
    if local_bind_addr.and_then(extract_port_v0) == Some(0) {
        return true;
    }
    match (
        configured_addr_hint.and_then(extract_port_v0),
        observed_addr.and_then(extract_port_v0),
    ) {
        (Some(configured), Some(observed)) => configured != observed,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn extract_port_v0(addr: &str) -> Option<u16> {
    addr.rsplit_once(':')?.1.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_reachability_probe_v0, classify_endpoint_scope_v0, decide_reachability_probe_v0,
        floating_port_active_v0, EndpointScope, FloatingPortMode, ReachabilityProbeInput,
        ReachabilityProbeStatus,
    };
    use crate::routing::{L4LocalRoutingTable, Reachability, RoutingSource};

    #[test]
    fn direct_public_probe_marks_reachable() {
        let decision = decide_reachability_probe_v0(ReachabilityProbeInput {
            peer_id: "peer-a".into(),
            configured_addr_hint: Some("8.8.8.8:39011".into()),
            observed_addr: Some("8.8.8.8:39011".into()),
            local_bind_addr: Some("0.0.0.0:39010".into()),
            floating_port_mode: FloatingPortMode::Fixed,
            direct_probe_sent: true,
            direct_probe_ack: true,
            relay_available: false,
            rtt_ms: Some(12),
            observed_unix_ms: 100,
            source: RoutingSource::LocalObserved,
        });

        assert_eq!(decision.status, ReachabilityProbeStatus::DirectReachable);
        assert_eq!(decision.reachability, Reachability::Reachable);
        assert_eq!(decision.endpoint_scope, EndpointScope::Public);
        assert!(!decision.floating_port_active);
    }

    #[test]
    fn lan_probe_marks_lan_only() {
        let decision = decide_reachability_probe_v0(ReachabilityProbeInput {
            peer_id: "peer-a".into(),
            configured_addr_hint: Some("192.168.71.117:39011".into()),
            observed_addr: Some("192.168.71.117:39011".into()),
            local_bind_addr: Some("0.0.0.0:39010".into()),
            floating_port_mode: FloatingPortMode::Fixed,
            direct_probe_sent: true,
            direct_probe_ack: true,
            relay_available: true,
            rtt_ms: Some(3),
            observed_unix_ms: 100,
            source: RoutingSource::LocalObserved,
        });

        assert_eq!(decision.status, ReachabilityProbeStatus::LanReachable);
        assert_eq!(decision.reachability, Reachability::LanOnly);
        assert_eq!(decision.endpoint_scope, EndpointScope::Private);
    }

    #[test]
    fn failed_direct_probe_with_relay_marks_relay_only() {
        let decision = decide_reachability_probe_v0(ReachabilityProbeInput {
            peer_id: "peer-a".into(),
            configured_addr_hint: Some("203.0.113.10:39011".into()),
            observed_addr: None,
            local_bind_addr: Some("0.0.0.0:0".into()),
            floating_port_mode: FloatingPortMode::EphemeralAllowed,
            direct_probe_sent: true,
            direct_probe_ack: false,
            relay_available: true,
            rtt_ms: None,
            observed_unix_ms: 100,
            source: RoutingSource::LocalObserved,
        });

        assert_eq!(decision.status, ReachabilityProbeStatus::RelayOnly);
        assert_eq!(decision.reachability, Reachability::RelayOnly);
        assert!(decision.floating_port_active);
    }

    #[test]
    fn apply_probe_updates_l4_table() {
        let table = L4LocalRoutingTable::new("self", 8);
        let decision = apply_reachability_probe_v0(
            &table,
            ReachabilityProbeInput {
                peer_id: "peer-a".into(),
                configured_addr_hint: Some("8.8.4.4:39011".into()),
                observed_addr: Some("8.8.4.4:39199".into()),
                local_bind_addr: Some("0.0.0.0:0".into()),
                floating_port_mode: FloatingPortMode::EphemeralAllowed,
                direct_probe_sent: true,
                direct_probe_ack: true,
                relay_available: false,
                rtt_ms: Some(7),
                observed_unix_ms: 123,
                source: RoutingSource::LocalObserved,
            },
        );

        let peer = table.get_peer("peer-a").expect("peer inserted");
        assert_eq!(decision.reachability, Reachability::Reachable);
        assert!(decision.floating_port_active);
        assert_eq!(peer.reachability, Reachability::Reachable);
        assert_eq!(peer.addr_hint.as_deref(), Some("8.8.4.4:39199"));
        assert_eq!(peer.latency_ms, Some(7));
        assert_eq!(peer.last_seen_unix_ms, 123);
    }

    #[test]
    fn endpoint_scope_classification_handles_common_addresses() {
        assert_eq!(
            classify_endpoint_scope_v0("127.0.0.1:1"),
            EndpointScope::Loopback
        );
        assert_eq!(
            classify_endpoint_scope_v0("192.168.0.1:1"),
            EndpointScope::Private
        );
        assert_eq!(
            classify_endpoint_scope_v0("8.8.8.8:1"),
            EndpointScope::Public
        );
        assert_eq!(
            classify_endpoint_scope_v0("peer.example:1"),
            EndpointScope::Unknown
        );
    }

    #[test]
    fn floating_port_detects_ephemeral_bind_or_changed_observed_port() {
        assert!(floating_port_active_v0(
            Some("8.8.8.8:39011"),
            Some("8.8.8.8:39100"),
            Some("0.0.0.0:39010"),
            FloatingPortMode::EphemeralAllowed,
        ));
        assert!(floating_port_active_v0(
            Some("8.8.8.8:39011"),
            Some("8.8.8.8:39011"),
            Some("0.0.0.0:0"),
            FloatingPortMode::EphemeralAllowed,
        ));
        assert!(!floating_port_active_v0(
            Some("8.8.8.8:39011"),
            Some("8.8.8.8:39100"),
            Some("0.0.0.0:39010"),
            FloatingPortMode::Fixed,
        ));
    }
}
