pub mod client;
pub mod frame;
pub mod server;

pub use client::*;
pub use frame::*;
pub use server::*;

#[cfg(test)]
mod tests {
    use super::{MultiHopRelayFrame, RelayClient, RelayServer};

    #[test]
    fn single_relay_forward_roundtrip() {
        let relay_server = RelayServer::new("relay-01");
        let relay_client = RelayClient::new(
            "relay-01".to_string(),
            "target-a".to_string(),
            &relay_server,
        );

        let result =
            relay_client.forward_with("req-1".to_string(), b"ping".to_vec(), |target, payload| {
                assert_eq!(target, "target-a");
                let mut response = b"echo:".to_vec();
                response.extend_from_slice(payload);
                (true, response)
            });

        assert!(result.ok);
        assert_eq!(result.relay_id, "relay-01");
        assert_eq!(result.request_id, "req-1");
        assert_eq!(result.response, b"echo:ping".to_vec());
    }

    #[test]
    fn multihop_relay_preserves_no_ip_identity_route() {
        let relay_server = RelayServer::new("relay-root");
        let frame = MultiHopRelayFrame::new(
            "req-2",
            "peer-source",
            "peer-target",
            vec!["peer-relay-a".to_string(), "peer-relay-b".to_string()],
            vec!["token-a".to_string(), "token-b".to_string()],
            3,
            b"novorudp-frame".to_vec(),
        );

        let result = relay_server.forward_multihop(frame);
        assert!(result.delivered);
        assert_eq!(
            result.visited_hops,
            vec!["peer-relay-a".to_string(), "peer-relay-b".to_string()]
        );
        assert_eq!(result.remaining_ttl, 1);
        assert_eq!(result.payload, b"novorudp-frame".to_vec());
    }

    #[test]
    fn multihop_relay_rejects_ip_addressed_hop() {
        let relay_server = RelayServer::new("relay-root");
        let frame = MultiHopRelayFrame::new(
            "req-3",
            "peer-source",
            "peer-target",
            vec!["192.168.1.10:39001".to_string()],
            vec!["token-a".to_string()],
            3,
            b"novorudp-frame".to_vec(),
        );

        let result = relay_server.forward_multihop(frame);
        assert!(!result.delivered);
        assert!(result.visited_hops.is_empty());
    }

    #[test]
    fn multihop_relay_enforces_ttl() {
        let relay_server = RelayServer::new("relay-root");
        let frame = MultiHopRelayFrame::new(
            "req-4",
            "peer-source",
            "peer-target",
            vec!["peer-relay-a".to_string(), "peer-relay-b".to_string()],
            vec!["token-a".to_string(), "token-b".to_string()],
            1,
            b"novorudp-frame".to_vec(),
        );

        let result = relay_server.forward_multihop(frame);
        assert!(!result.delivered);
        assert_eq!(result.visited_hops, vec!["peer-relay-a".to_string()]);
        assert_eq!(result.remaining_ttl, 0);
    }
}
