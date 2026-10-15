//! Native application read protocol v1. Binary, bounded, read-only; carried by
//! the existing authenticated NovoRUDP session, not a second transport or ledger.
//! Replies are the authenticated serving node's observations, NOT light-client
//! execution proofs. Received is volatile; NotRecent is not proof of absence.
use serde::{Deserialize, Serialize};

pub const STREAM: u64 = u64::from_be_bytes(*b"NVQUERY1");
pub const MAX_BYTES: usize = 1024;
const MAGIC: &[u8; 8] = b"NVQUERY1";
pub type Hash = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountId {
    Bytes20([u8; 20]),
    Bytes32([u8; 32]),
}
impl AccountId {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Bytes20(v) => v,
            Self::Bytes32(v) => v,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Query {
    Status,
    Receipt { hash: Hash },
    NovBalance { account: AccountId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub session: Hash,
    pub sequence: u64,
    pub query: Query,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub height: u64,
    pub block_hash: Hash,
    pub state_root: Hash,
    pub receipt_batch_commitment: Hash,
    pub state_version: u64,
    pub indexed_height: u64,
    pub current_height: u64,
    pub round: u64,
    pub recovering: bool,
    pub projection_failed: bool,
    pub pending: u64,
    pub apfl_rows: u64,
    pub integer_rows: u64,
    pub integer_plans: u64,
    pub rejected_inputs: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiptState {
    NotRecent,
    Received,
    RejectedNonce,
    /// Verified archive projection after durable BFT publication. No ZK claim.
    Finalized {
        success: bool,
        nonce_after: u64,
        charged_fee: u128,
        height: u64,
        block_hash: Hash,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balance {
    pub amount: u128,
    pub state_root: Hash,
    pub height: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryError {
    Busy,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Answer {
    Status(Status),
    Receipt {
        state: ReceiptState,
        indexed_height: u64,
    },
    /// None means read pending. Missing account at the captured root is Some(0).
    NovBalance(Option<Balance>),
    Error(QueryError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub request: Request,
    pub answer: Answer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame {
    Request(Request),
    Response(Response),
}

fn valid(frame: &Frame) -> bool {
    let request = match frame {
        Frame::Request(r) => r,
        Frame::Response(r) => &r.request,
    };
    if request.session == [0; 32] || request.sequence == 0 {
        return false;
    }
    match frame {
        Frame::Response(Response { request, answer }) => matches!(
            (request.query, answer),
            (_, Answer::Error(_))
                | (Query::Status, Answer::Status(_))
                | (Query::Receipt { .. }, Answer::Receipt { .. })
                | (Query::NovBalance { .. }, Answer::NovBalance(_))
        ),
        _ => true,
    }
}
pub fn encode(frame: Frame) -> Result<Vec<u8>, &'static str> {
    if !valid(&frame) {
        return Err("invalid native query binding");
    }
    let mut bytes = MAGIC.to_vec();
    bytes.extend(postcard::to_allocvec(&frame).map_err(|_| "native query encode")?);
    if bytes.len() > MAX_BYTES {
        return Err("native query size");
    }
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<Frame, &'static str> {
    if bytes.len() > MAX_BYTES || !bytes.starts_with(MAGIC) {
        return Err("native query header/size");
    }
    // All decoded fields have fixed bounds: no peer-controlled Vec/String allocation.
    let (frame, tail): (Frame, _) =
        postcard::take_from_bytes(&bytes[8..]).map_err(|_| "native query decode")?;
    if !tail.is_empty() || !valid(&frame) {
        return Err("native query trailing bytes/binding");
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_binary_query_preserves_full_width_and_rejects_malformed_frames() {
        let request = Request {
            session: [7; 32],
            sequence: u64::MAX,
            query: Query::Receipt { hash: [3; 32] },
        };
        let frame = Frame::Response(Response {
            request,
            answer: Answer::Receipt {
                state: ReceiptState::Finalized {
                    success: false,
                    nonce_after: u64::MAX,
                    charged_fee: u128::MAX,
                    height: 3,
                    block_hash: [9; 32],
                },
                indexed_height: 3,
            },
        });
        let bytes = encode(frame).unwrap();
        assert_eq!(decode(&bytes).unwrap(), frame);
        for n in 0..bytes.len() {
            assert!(decode(&bytes[..n]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
        assert!(decode(&vec![0; MAX_BYTES + 1]).is_err());
        assert!(encode(Frame::Request(Request {
            sequence: 0,
            ..request
        }))
        .is_err());
        assert!(encode(Frame::Response(Response {
            request,
            answer: Answer::NovBalance(None)
        }))
        .is_err());
        assert_ne!(AccountId::Bytes20([0; 20]), AccountId::Bytes32([0; 32]));
    }
}
