pub mod client;
pub mod frame;
pub mod server;

pub use client::*;
pub use frame::*;
pub use server::*;

#[cfg(test)]
mod tests {
    use super::{RelayClient, RelayServer};

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
}
