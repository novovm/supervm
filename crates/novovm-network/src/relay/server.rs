use crate::relay::RelayFrame;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayResult<T> {
    pub request_id: String,
    pub relay_id: String,
    pub ok: bool,
    pub response: T,
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
            RelayFrame::Result { .. } => unreachable!("RelayServer::forward_with only accepts Forward frame"),
        }
    }
}
