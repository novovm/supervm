use crate::relay::{MultiHopRelayFrame, RelayFrame};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayResult<T> {
    pub request_id: String,
    pub relay_id: String,
    pub ok: bool,
    pub response: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiHopRelayResult {
    pub request_id: String,
    pub target_peer_id: String,
    pub delivered: bool,
    pub visited_hops: Vec<String>,
    pub remaining_ttl: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RelayServer {
    relay_id: String,
}

impl RelayServer {
    pub fn new(relay_id: impl Into<String>) -> Self {
        Self {
            relay_id: relay_id.into(),
        }
    }

    pub fn relay_id(&self) -> &str {
        &self.relay_id
    }

    pub fn forward_with<T, F>(
        &self,
        request_id: String,
        target: String,
        payload: Vec<u8>,
        mut forward: F,
    ) -> RelayResult<T>
    where
        F: FnMut(&str, &[u8]) -> (bool, T),
    {
        let frame = RelayFrame::Forward {
            request_id,
            target,
            payload,
        };

        match frame {
            RelayFrame::Forward {
                request_id,
                target,
                payload,
            } => {
                let (ok, response) = forward(&target, &payload);
                RelayResult {
                    request_id,
                    relay_id: self.relay_id.clone(),
                    ok,
                    response,
                }
            }
            RelayFrame::Result { .. } => {
                unreachable!("RelayServer::forward_with only accepts Forward frame")
            }
        }
    }

    pub fn forward_multihop(&self, mut frame: MultiHopRelayFrame) -> MultiHopRelayResult {
        if frame.ttl == 0 || !frame.is_no_ip_route() {
            return MultiHopRelayResult {
                request_id: frame.request_id,
                target_peer_id: frame.target_peer_id,
                delivered: false,
                visited_hops: Vec::new(),
                remaining_ttl: frame.ttl,
                payload: frame.payload,
            };
        }

        let mut visited = Vec::new();
        for hop in frame.hop_peer_ids.iter() {
            if frame.ttl == 0 {
                return MultiHopRelayResult {
                    request_id: frame.request_id,
                    target_peer_id: frame.target_peer_id,
                    delivered: false,
                    visited_hops: visited,
                    remaining_ttl: 0,
                    payload: frame.payload,
                };
            }
            frame.ttl = frame.ttl.saturating_sub(1);
            visited.push(hop.clone());
        }

        MultiHopRelayResult {
            request_id: frame.request_id,
            target_peer_id: frame.target_peer_id,
            delivered: frame.ttl > 0 || frame.hop_peer_ids.is_empty(),
            visited_hops: visited,
            remaining_ttl: frame.ttl,
            payload: frame.payload,
        }
    }
}
