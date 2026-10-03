#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdaptivePeerRoutePolicy {
    Auto,
    PrimaryOnly,
    PluginOnly,
}

impl AdaptivePeerRoutePolicy {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::PrimaryOnly => "primary_only",
            Self::PluginOnly => "plugin_only",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPeerEndpoint {
    pub endpoint: String,
    pub node_hint: u64,
    pub addr_hint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptivePeerRoutes {
    pub primary_peers: Vec<(u64, String)>,
    pub plugin_peers: Vec<PluginPeerEndpoint>,
}

#[must_use]
pub fn parse_u64_with_optional_hex_prefix(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16).ok();
    }
    trimmed.parse::<u64>().ok()
}

#[must_use]
pub fn parse_port_from_addr_hint(addr: &str) -> Option<u16> {
    if let Ok(sock) = addr.parse::<std::net::SocketAddr>() {
        return Some(sock.port());
    }
    let (_, port_raw) = addr.rsplit_once(':')?;
    port_raw.parse::<u16>().ok()
}

#[must_use]
pub fn parse_port_list(raw: &str) -> Vec<u16> {
    let mut ports = BTreeSet::<u16>::new();
    for token in raw.split([',', ';', ' ', '\t', '\n', '\r']) {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(port) = trimmed.parse::<u16>() {
            ports.insert(port);
        }
    }
    ports.into_iter().collect()
}

#[must_use]
pub fn route_uses_plugin_by_port(addr_hint: &str, plugin_ports: &[u16]) -> bool {
    let Some(port) = parse_port_from_addr_hint(addr_hint) else {
        return false;
    };
    plugin_ports.contains(&port)
}

fn derive_node_hint_from_enode_pubkey(pubkey_hex: &str) -> u64 {
    let head = if pubkey_hex.len() >= 16 {
        &pubkey_hex[..16]
    } else {
        pubkey_hex
    };
    u64::from_str_radix(head, 16).unwrap_or(0).max(1)
}

#[must_use]
pub fn parse_enode_endpoint(entry: &str) -> Option<(u64, String)> {
    let trimmed = entry.trim();
    if !trimmed.to_ascii_lowercase().starts_with("enode://") {
        return None;
    }
    let without_scheme = &trimmed["enode://".len()..];
    let (pubkey_hex, addr_and_query) = without_scheme.split_once('@')?;
    if pubkey_hex.is_empty()
        || !pubkey_hex.chars().all(|ch| ch.is_ascii_hexdigit())
        || pubkey_hex.len() < 16
    {
        return None;
    }
    let addr = addr_and_query
        .split_once('?')
        .map(|(base, _)| base)
        .unwrap_or(addr_and_query)
        .trim();
    if addr.is_empty() || parse_port_from_addr_hint(addr).is_none() {
        return None;
    }
    Some((
        derive_node_hint_from_enode_pubkey(pubkey_hex),
        addr.to_string(),
    ))
}

#[must_use]
pub fn classify_adaptive_peer_routes(
    raw: &str,
    route_policy: AdaptivePeerRoutePolicy,
    plugin_ports: &[u16],
) -> AdaptivePeerRoutes {
    let mut primary_peers = Vec::<(u64, String)>::new();
    let mut plugin_peers = Vec::<PluginPeerEndpoint>::new();

    for token in raw.split([',', ';', '\n', '\r', '\t', ' ']) {
        let entry = token.trim();
        if entry.is_empty() {
            continue;
        }

        if let Some((node_hint, addr_hint)) = parse_enode_endpoint(entry) {
            if matches!(route_policy, AdaptivePeerRoutePolicy::PrimaryOnly) {
                primary_peers.push((node_hint, addr_hint));
            } else {
                plugin_peers.push(PluginPeerEndpoint {
                    endpoint: entry.to_string(),
                    node_hint,
                    addr_hint,
                });
            }
            continue;
        }

        let (node_raw, addr_raw) = entry
            .split_once('@')
            .or_else(|| entry.split_once('='))
            .unwrap_or(("", ""));
        if node_raw.is_empty() || addr_raw.trim().is_empty() {
            continue;
        }
        let Some(node_id) = parse_u64_with_optional_hex_prefix(node_raw) else {
            continue;
        };
        let addr_hint = addr_raw.trim().to_string();
        let to_plugin = match route_policy {
            AdaptivePeerRoutePolicy::PrimaryOnly => false,
            AdaptivePeerRoutePolicy::PluginOnly => true,
            AdaptivePeerRoutePolicy::Auto => {
                route_uses_plugin_by_port(addr_hint.as_str(), plugin_ports)
            }
        };
        if to_plugin {
            plugin_peers.push(PluginPeerEndpoint {
                endpoint: entry.to_string(),
                node_hint: node_id,
                addr_hint,
            });
        } else {
            primary_peers.push((node_id, addr_hint));
        }
    }

    AdaptivePeerRoutes {
        primary_peers,
        plugin_peers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_routes_auto_splits_primary_and_plugin() {
        let enode = "enode://0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef@127.0.0.1:30303?discport=30301";
        let raw = format!("2@127.0.0.1:39001,3@127.0.0.1:30304,{}", enode);
        let routes = classify_adaptive_peer_routes(
            raw.as_str(),
            AdaptivePeerRoutePolicy::Auto,
            &[30303, 30304],
        );
        assert_eq!(
            routes.primary_peers,
            vec![(2, "127.0.0.1:39001".to_string())]
        );
        assert_eq!(routes.plugin_peers.len(), 2);
        assert_eq!(routes.plugin_peers[0].endpoint, "3@127.0.0.1:30304");
        assert!(routes.plugin_peers[1].endpoint.starts_with("enode://"));
    }

    #[test]
    fn classify_routes_primary_only_forces_primary_route() {
        let enode = "enode://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@10.0.0.1:30303";
        let raw = format!("8@127.0.0.1:30303,{}", enode);
        let routes = classify_adaptive_peer_routes(
            raw.as_str(),
            AdaptivePeerRoutePolicy::PrimaryOnly,
            &[30303],
        );
        assert_eq!(routes.primary_peers.len(), 2);
        assert!(routes.plugin_peers.is_empty());
    }

    #[test]
    fn classify_routes_plugin_only_forces_plugin_route() {
        let routes = classify_adaptive_peer_routes(
            "9@127.0.0.1:39001",
            AdaptivePeerRoutePolicy::PluginOnly,
            &[],
        );
        assert!(routes.primary_peers.is_empty());
        assert_eq!(routes.plugin_peers.len(), 1);
        assert_eq!(routes.plugin_peers[0].node_hint, 9);
        assert_eq!(routes.plugin_peers[0].addr_hint, "127.0.0.1:39001");
    }
}
