use super::Hash;
use serde::{Deserialize, Serialize};

// These bounds are the existing native Patricia codec's canonical encoding.
pub const STATE_BRANCH_BYTES: usize = 99;
pub const MAX_STATE_NODE_BYTES: usize = 35 + 256;
pub const MAX_STATE_ANCESTRY_BYTES: usize = 256 * STATE_BRANCH_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateNodeSelector {
    pub height: u64,
    pub root: Hash,
    pub hash: Hash,
    /// Canonical branch bytes, ordered from the finalized root to the requested node's parent.
    #[serde(with = "ancestry")]
    pub ancestry: Vec<u8>,
}

impl StateNodeSelector {
    pub(super) fn is_valid(&self) -> bool {
        self.root != [0; 32]
            && self.hash != [0; 32]
            && (self.root == self.hash) == self.ancestry.is_empty()
            && self.ancestry.len() <= MAX_STATE_ANCESTRY_BYTES
            && self.ancestry.len().is_multiple_of(STATE_BRANCH_BYTES)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateNodePayload(#[serde(with = "node_bytes")] pub Vec<u8>);

impl StateNodePayload {
    pub(super) fn is_valid(&self) -> bool {
        (35..=MAX_STATE_NODE_BYTES).contains(&self.0.len())
    }
}

use super::archive::bytes::{deserialize_bounded, serialize};

mod ancestry {
    pub(super) use super::serialize;
    use super::*;

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        decoder: D,
    ) -> Result<Vec<u8>, D::Error> {
        deserialize_bounded::<D, 0, MAX_STATE_ANCESTRY_BYTES>(decoder)
    }
}

mod node_bytes {
    pub(super) use super::serialize;
    use super::*;

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        decoder: D,
    ) -> Result<Vec<u8>, D::Error> {
        deserialize_bounded::<D, 35, MAX_STATE_NODE_BYTES>(decoder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_query::{
        decode, decode_response, encode, Answer, Frame, Query, Request, Response, MAX_BYTES,
    };

    #[test]
    fn bounded_state_requests_and_payloads_remain_bound_to_the_exact_selector() {
        let mut selector = StateNodeSelector {
            height: 9,
            root: [1; 32],
            hash: [2; 32],
            ancestry: vec![3; MAX_STATE_ANCESTRY_BYTES],
        };
        let request = |selector| Request {
            session: [4; 32],
            sequence: 7,
            query: Query::StateNode(selector),
        };
        let frame = Frame::Request(request(selector.clone()));
        let mut bytes = encode(frame.clone()).unwrap();
        assert!(bytes.len() < MAX_BYTES);
        assert_eq!(decode(&bytes).unwrap(), frame);
        bytes[..8].copy_from_slice(b"NVQARC10");
        assert!(decode(&bytes).is_err());

        let response = Response {
            request: request(selector.clone()),
            answer: Answer::StateNode(StateNodePayload(vec![5; MAX_STATE_NODE_BYTES])),
        };
        let bytes = encode(Frame::Response(response.clone())).unwrap();
        assert_eq!(
            decode_response(&bytes, &response.request).unwrap(),
            Some(response)
        );
        selector.hash[0] ^= 1;
        assert!(decode_response(&bytes, &request(selector.clone()))
            .unwrap()
            .is_none());
        selector.ancestry.push(0);
        assert!(encode(Frame::Request(request(selector))).is_err());
    }
}
