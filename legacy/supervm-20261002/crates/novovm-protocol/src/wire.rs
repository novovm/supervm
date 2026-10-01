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
    crate::bincode_compat::serialize(msg).map_err(|e| WireError::Encode(e.to_string()))
}

/// Decode a protocol message from transport bytes.
pub fn decode(bytes: &[u8]) -> Result<ProtocolMessage, WireError> {
    crate::bincode_compat::deserialize(bytes).map_err(|e| WireError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeId, PacemakerMessage};

    #[test]
    fn pacemaker_message_roundtrip() {
        let msg = ProtocolMessage::Pacemaker(PacemakerMessage::ViewSync {
            from: NodeId(2),
            height: 7,
            view: 3,
            leader: NodeId(1),
        });
        let encoded = encode(&msg).expect("encode pacemaker should succeed");
        let decoded = decode(&encoded).expect("decode pacemaker should succeed");
        match decoded {
            ProtocolMessage::Pacemaker(PacemakerMessage::ViewSync {
                from,
                height,
                view,
                leader,
            }) => {
                assert_eq!(from, NodeId(2));
                assert_eq!(height, 7);
                assert_eq!(view, 3);
                assert_eq!(leader, NodeId(1));
            }
            _ => panic!("decoded wrong protocol message kind"),
        }
    }
}
