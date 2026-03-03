#![forbid(unsafe_code)]

use crate::ProtocolMessage;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WireError {
    #[error("encode failed: {0}")]
    Encode(String),
    #[error("decode failed: {0}")]
    Decode(String),
}

/// Encode a protocol message for transport.
pub fn encode(msg: &ProtocolMessage) -> Result<Vec<u8>, WireError> {
    bincode::serialize(msg).map_err(|e| WireError::Encode(e.to_string()))
}

/// Decode a protocol message from transport bytes.
pub fn decode(bytes: &[u8]) -> Result<ProtocolMessage, WireError> {
    bincode::deserialize(bytes).map_err(|e| WireError::Decode(e.to_string()))
}
