use crate::relay::{RelayResult, RelayServer};

#[derive(Debug)]
pub struct RelayClient<'a> {
    relay_id: String,
    target: String,
    server: &'a RelayServer,
}

impl<'a> RelayClient<'a> {
    pub fn new(relay_id: String, target: String, server: &'a RelayServer) -> Self {
        Self {
            relay_id,
            target,
            server,
        }
    }

    pub fn relay_id(&self) -> &str {
        &self.relay_id
    }

    pub fn forward_with<T, F>(
        &self,
        request_id: String,
        payload: Vec<u8>,
        forward: F,
    ) -> RelayResult<T>
    where
        F: FnMut(&str, &[u8]) -> (bool, T),
    {
        self.server
            .forward_with(request_id, self.target.clone(), payload, forward)
    }
}
