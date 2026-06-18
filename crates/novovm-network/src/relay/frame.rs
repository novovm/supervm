use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayFrame {
    Forward {
        request_id: String,
        target: String,
        payload: Vec<u8>,
    },
    Result {
        request_id: String,
        ok: bool,
        payload: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiHopRelayFrame {
    pub request_id: String,
    pub source_peer_id: String,
    pub target_peer_id: String,
    pub hop_peer_ids: Vec<String>,
    pub route_tokens: Vec<String>,
    pub ttl: u8,
    pub payload: Vec<u8>,
}

impl MultiHopRelayFrame {
    pub fn new(
        request_id: impl Into<String>,
        source_peer_id: impl Into<String>,
        target_peer_id: impl Into<String>,
        hop_peer_ids: Vec<String>,
        route_tokens: Vec<String>,
        ttl: u8,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            source_peer_id: source_peer_id.into(),
            target_peer_id: target_peer_id.into(),
            hop_peer_ids,
            route_tokens,
            ttl,
            payload,
        }
    }

    pub fn is_no_ip_route(&self) -> bool {
        !self.source_peer_id.contains(':')
            && !self.target_peer_id.contains(':')
            && self.hop_peer_ids.iter().all(|hop| !hop.contains(':'))
    }
}
