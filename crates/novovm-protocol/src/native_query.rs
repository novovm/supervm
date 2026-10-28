//! Native application reads: unchanged single-item v1 and bounded batch v2.
//! Binary, bounded, read-only; carried by
//! the existing authenticated NovoRUDP session, not a second transport or ledger.
//! Replies are the authenticated serving node's observations, NOT light-client
//! execution proofs. Received is volatile; NotRecent is not proof of absence.
use serde::{Deserialize, Serialize};

pub const STREAM: u64 = u64::from_be_bytes(*b"NVQUERY1");
pub const MAX_V1_BYTES: usize = 1024;
pub const MAX_RECEIPTS: usize = 128;
// Largest batch response echoes 128 hashes and 128 finalized states. Each
// state uses at most 73 bytes (tag/bool/u64/u128/u64/hash), plus <128 bytes of
// envelope/lengths. 128 * (32 + 73) + 128 < 32 KiB; no numeric narrowing.
pub const MAX_BYTES: usize = 32 * 1024;
const MAGIC_V1: &[u8; 8] = b"NVQUERY1";
const MAGIC_V2: &[u8; 8] = b"NVQUERY2";
const MAGIC_V4: &[u8; 8] = b"NVQUERY4";
pub type Hash = [u8; 32];

/// Nonempty bounded sequence. Its wire representation is an ordinary postcard
/// sequence, but the advertised length is checked BEFORE allocating elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptBatch<T>(Vec<T>);
impl<T> ReceiptBatch<T> {
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }
    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}
impl<T> TryFrom<Vec<T>> for ReceiptBatch<T> {
    type Error = &'static str;
    fn try_from(values: Vec<T>) -> Result<Self, Self::Error> {
        if values.is_empty() || values.len() > MAX_RECEIPTS {
            return Err("native receipt batch count");
        }
        Ok(Self(values))
    }
}
impl<T: Serialize> Serialize for ReceiptBatch<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ReceiptBatch<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BoundedVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for BoundedVisitor<T> {
            type Value = ReceiptBatch<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "1..={MAX_RECEIPTS} receipt batch entries")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                // Postcard exposes its length prefix. Require it rather than
                // allocating an unknown-size sequence then checking afterwards.
                let count = sequence
                    .size_hint()
                    .filter(|n| (1..=MAX_RECEIPTS).contains(n))
                    .ok_or_else(|| serde::de::Error::custom("native receipt batch count"))?;
                let mut values = Vec::with_capacity(count);
                for _ in 0..count {
                    values.push(sequence.next_element()?.ok_or_else(|| {
                        serde::de::Error::custom("truncated native receipt batch")
                    })?);
                }
                if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(serde::de::Error::custom("native receipt batch overflow"));
                }
                Ok(ReceiptBatch(values))
            }
        }
        deserializer.deserialize_seq(BoundedVisitor(std::marker::PhantomData))
    }
}

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Query {
    Status,
    Receipt {
        hash: Hash,
    },
    NovBalance {
        account: AccountId,
    },
    /// Order is part of the exact echoed request. Only NVQUERY2 may carry it.
    Receipts {
        hashes: ReceiptBatch<Hash>,
    },
    /// Read-only observation at a published root; NEVER a spend permission.
    /// Concrete accounts are committed by the canonical v2 intent, not hints.
    AssetSpendState {
        intent: AssetReadIntentV2,
    },
}

/// The original NUAS intent bytes, bounded and validated BEFORE ownership.
/// This is neither a signed transaction nor a new signing transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetReadIntentV2(Vec<u8>);
impl AssetReadIntentV2 {
    pub fn new(intent: &crate::uca_spend::AssetSpendIntentV2) -> Result<Self, &'static str> {
        intent
            .signing_bytes()
            .map(Self)
            .map_err(|_| "invalid UCA read intent")
    }
    pub fn intent(&self) -> crate::uca_spend::AssetSpendIntentV2 {
        crate::uca_spend::AssetSpendIntentV2::decode_signing_bytes(&self.0)
            .expect("validated immutable UCA read intent")
    }
}
impl Serialize for AssetReadIntentV2 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}
impl<'de> Deserialize<'de> for AssetReadIntentV2 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IntentVisitor;
        impl<'de> serde::de::Visitor<'de> for IntentVisitor {
            type Value = AssetReadIntentV2;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bounded canonical NUAS intent")
            }
            fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                crate::uca_spend::AssetSpendIntentV2::decode_signing_bytes(value)
                    .map_err(|_| E::custom("invalid UCA read intent"))?;
                Ok(AssetReadIntentV2(value.to_vec()))
            }
        }
        deserializer.deserialize_bytes(IntentVisitor)
    }
}

/// Serving-node observation only, not a proof or reservation. All fields are
/// read from the SAME published root, not assembled from caller declarations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSpendStateViewV2 {
    pub state_root: Hash,
    pub height: u64,
    pub account_active: bool,
    pub current_spend_epoch: u64,
    pub management_key_ref: Hash,
    pub recovery_key_ref: Option<Hash>,
    pub grant_present: bool,
    pub grant_epoch: u64,
    pub permits_nov_transfer: bool,
    pub revoked: bool,
    pub not_before_height: u64,
    pub expires_at_height: u64,
    pub max_debit_per_intent: u128,
    pub remaining_debit_budget: u128,
    pub next_nonce: u64,
    pub payer_balance: u128,
    pub recipient_balance: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetState {
    Pending,
    /// A required account or grant record was absent at this exact root. Not
    /// unknown IO, malformed state or a claim that the UCA never existed.
    Missing {
        state_root: Hash,
        height: u64,
    },
    Present(Box<AssetSpendStateViewV2>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Answer {
    Status(Status),
    Receipt {
        state: ReceiptState,
        indexed_height: u64,
    },
    /// None means read pending. Missing account at the captured root is Some(0).
    NovBalance(Option<Balance>),
    Error(QueryError),
    Receipts {
        states: ReceiptBatch<ReceiptState>,
        indexed_height: u64,
    },
    AssetSpendState(AssetState),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub request: Request,
    pub answer: Answer,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame {
    Request(Request),
    Response(Response),
}

fn request(frame: &Frame) -> &Request {
    match frame {
        Frame::Request(r) => r,
        Frame::Response(r) => &r.request,
    }
}
fn is_batch(frame: &Frame) -> bool {
    matches!(request(frame).query, Query::Receipts { .. })
}
fn is_uca(frame: &Frame) -> bool {
    matches!(request(frame).query, Query::AssetSpendState { .. })
}
fn valid(frame: &Frame) -> bool {
    let request = request(frame);
    if request.session == [0; 32] || request.sequence == 0 {
        return false;
    }
    match frame {
        Frame::Response(Response { request, answer }) => match (&request.query, answer) {
            (_, Answer::Error(_))
            | (Query::Status, Answer::Status(_))
            | (Query::Receipt { .. }, Answer::Receipt { .. })
            | (Query::NovBalance { .. }, Answer::NovBalance(_))
            | (Query::AssetSpendState { .. }, Answer::AssetSpendState(_)) => true,
            (Query::Receipts { hashes }, Answer::Receipts { states, .. }) => {
                hashes.len() == states.len()
            }
            _ => false,
        },
        _ => true,
    }
}
pub fn encode(frame: Frame) -> Result<Vec<u8>, &'static str> {
    if !valid(&frame) {
        return Err("invalid native query binding");
    }
    let batch = is_batch(&frame);
    let mut bytes = if is_uca(&frame) {
        MAGIC_V4
    } else if batch {
        MAGIC_V2
    } else {
        MAGIC_V1
    }
    .to_vec();
    bytes.extend(postcard::to_allocvec(&frame).map_err(|_| "native query encode")?);
    if bytes.len() > if batch { MAX_BYTES } else { MAX_V1_BYTES } {
        return Err("native query size");
    }
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<Frame, &'static str> {
    let uca = bytes.starts_with(MAGIC_V4);
    let batch = if bytes.starts_with(MAGIC_V1) {
        false
    } else if bytes.starts_with(MAGIC_V2) {
        true
    } else if uca {
        false
    } else {
        return Err("native query header/size");
    };
    if bytes.len() > if batch { MAX_BYTES } else { MAX_V1_BYTES } {
        return Err("native query header/size");
    }
    // Fixed fields and checked length prefixes only; no unbounded Vec/String.
    let (frame, tail): (Frame, _) =
        postcard::take_from_bytes(&bytes[8..]).map_err(|_| "native query decode")?;
    if !tail.is_empty() || !valid(&frame) || is_batch(&frame) != batch || is_uca(&frame) != uca {
        return Err("native query trailing bytes/binding");
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn uca_intent() -> crate::uca_spend::AssetSpendIntentV2 {
        crate::uca_spend::AssetSpendIntentV2 {
            asset: 1,
            authority_update: None,
            chain_id: 7,
            genesis: [2; 32],
            payer: vec![1; 20],
            authorization_epoch: 3,
            spend_key_ref: [4; 32],
            recipient: vec![2; 32],
            amount: u128::MAX - 1,
            max_fee: 1,
            nonce: u64::MAX - 1,
            not_before_height: 1,
            expires_at_height: 10,
        }
    }

    #[test]
    fn uca_query_canonical_intent_full_width_and_version_binding() {
        let intent = uca_intent();
        let raw = intent.signing_bytes().unwrap();
        assert_eq!(
            crate::uca_spend::AssetSpendIntentV2::decode_signing_bytes(&raw).unwrap(),
            intent
        );
        for len in 0..raw.len() {
            assert!(
                crate::uca_spend::AssetSpendIntentV2::decode_signing_bytes(&raw[..len]).is_err()
            );
        }
        let mut tail = raw.clone();
        tail.push(0);
        assert!(crate::uca_spend::AssetSpendIntentV2::decode_signing_bytes(&tail).is_err());
        let request = Request {
            session: [7; 32],
            sequence: 1,
            query: Query::AssetSpendState {
                intent: AssetReadIntentV2::new(&intent).unwrap(),
            },
        };
        for answer in [
            AssetState::Pending,
            AssetState::Missing {
                state_root: [9; 32],
                height: 1,
            },
            AssetState::Present(Box::new(AssetSpendStateViewV2 {
                state_root: [9; 32],
                height: u64::MAX,
                account_active: false,
                current_spend_epoch: u64::MAX,
                management_key_ref: [6; 32],
                recovery_key_ref: None,
                grant_present: true,
                grant_epoch: u64::MAX,
                permits_nov_transfer: false,
                revoked: true,
                not_before_height: 0,
                expires_at_height: u64::MAX,
                max_debit_per_intent: u128::MAX,
                remaining_debit_budget: u128::MAX,
                next_nonce: u64::MAX,
                payer_balance: u128::MAX,
                recipient_balance: u128::MAX,
            })),
        ] {
            let frame = Frame::Response(Response {
                request: request.clone(),
                answer: Answer::AssetSpendState(answer),
            });
            let bytes = encode(frame.clone()).unwrap();
            assert!(bytes.starts_with(MAGIC_V4));
            assert_eq!(decode(&bytes).unwrap(), frame);
            for label in [MAGIC_V1, MAGIC_V2, b"NVQUERY3"] {
                let mut wrong = bytes.clone();
                wrong[..8].copy_from_slice(label);
                assert!(decode(&wrong).is_err());
            }
        }
        assert!(encode(Frame::Response(Response {
            request,
            answer: Answer::NovBalance(None)
        }))
        .is_err());
    }

    #[test]
    fn uca_read_intent_rejects_unbounded_or_invalid_body_before_ownership() {
        let mut raw = uca_intent().signing_bytes().unwrap();
        for bytes in [vec![0; 4096], {
            raw[7] = 0;
            raw[14] = 0;
            raw
        }] {
            let encoded = postcard::to_allocvec(&bytes).unwrap();
            assert!(postcard::from_bytes::<AssetReadIntentV2>(&encoded).is_err());
        }
        assert!(postcard::from_bytes::<AssetReadIntentV2>(
            &postcard::to_allocvec(&usize::MAX).unwrap()
        )
        .is_err());
    }
    #[test]
    fn bounded_binary_query_preserves_full_width_and_rejects_malformed_frames() {
        let request = Request {
            session: [7; 32],
            sequence: u64::MAX,
            query: Query::Receipt { hash: [3; 32] },
        };
        let frame = Frame::Response(Response {
            request: request.clone(),
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
        let bytes = encode(frame.clone()).unwrap();
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
            ..request.clone()
        }))
        .is_err());
        assert!(encode(Frame::Response(Response {
            request,
            answer: Answer::NovBalance(None)
        }))
        .is_err());
        assert_ne!(AccountId::Bytes20([0; 20]), AccountId::Bytes32([0; 32]));
    }

    fn batch_request(count: usize) -> Request {
        Request {
            session: [7; 32],
            sequence: u64::MAX,
            query: Query::Receipts {
                hashes: ReceiptBatch::try_from(
                    (0..count).map(|i| [i as u8; 32]).collect::<Vec<_>>(),
                )
                .unwrap(),
            },
        }
    }
    fn finalized() -> ReceiptState {
        ReceiptState::Finalized {
            success: true,
            nonce_after: u64::MAX,
            charged_fee: u128::MAX,
            height: u64::MAX,
            block_hash: [255; 32],
        }
    }

    #[test]
    fn batch_maximum_full_width_order_and_version_round_trip() {
        for count in [1, MAX_RECEIPTS] {
            let request = batch_request(count);
            let frame = Frame::Response(Response {
                request: request.clone(),
                answer: Answer::Receipts {
                    states: ReceiptBatch::try_from(vec![finalized(); count]).unwrap(),
                    indexed_height: u64::MAX,
                },
            });
            let bytes = encode(frame.clone()).unwrap();
            assert!(bytes.starts_with(MAGIC_V2));
            assert!(bytes.len() <= MAX_RECEIPTS * (32 + 73) + 128);
            assert_eq!(decode(&bytes).unwrap(), frame);
            assert_eq!(
                decode(&encode(Frame::Request(request.clone())).unwrap()).unwrap(),
                Frame::Request(request.clone())
            );
            let error = Frame::Response(Response {
                request,
                answer: Answer::Error(QueryError::Busy),
            });
            assert_eq!(decode(&encode(error.clone()).unwrap()).unwrap(), error);
            let mut old_label = bytes.clone();
            old_label[..8].copy_from_slice(MAGIC_V1);
            assert!(decode(&old_label).is_err());
            let mut trailing = bytes;
            trailing.push(0);
            assert!(decode(&trailing).is_err());
        }
        let frame = Frame::Request(batch_request(2));
        let mut reversed = batch_request(2);
        if let Query::Receipts { hashes } = &mut reversed.query {
            hashes.0.reverse();
        }
        assert_ne!(
            encode(frame).unwrap(),
            encode(Frame::Request(reversed)).unwrap()
        );
    }

    #[test]
    fn batch_count_rejected_before_element_decode_or_allocation() {
        struct Never;
        impl<'de> Deserialize<'de> for Never {
            fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
                panic!("invalid advertised length must fail before decoding any element")
            }
        }
        // Postcard unsigned length prefixes: 0, 129 and usize::MAX. No body is
        // supplied: rejection must follow the prefix, not collecting elements.
        for count in [0usize, MAX_RECEIPTS + 1, usize::MAX] {
            let prefix = postcard::to_allocvec(&count).unwrap();
            assert!(postcard::take_from_bytes::<ReceiptBatch<Never>>(&prefix).is_err());
        }
        // Enough remaining bytes for postcard to expose Some(129), not merely
        // reject a truncated sequence through its remaining-input size hint.
        let mut oversized = postcard::to_allocvec(&(MAX_RECEIPTS + 1)).unwrap();
        oversized.extend(vec![0; MAX_RECEIPTS + 1]);
        assert!(postcard::take_from_bytes::<ReceiptBatch<Never>>(&oversized).is_err());
        assert!(ReceiptBatch::<Hash>::try_from(Vec::new()).is_err());
        assert!(ReceiptBatch::try_from(vec![[0; 32]; MAX_RECEIPTS + 1]).is_err());
        let truncated = postcard::to_allocvec(&2usize).unwrap();
        assert!(postcard::take_from_bytes::<ReceiptBatch<Hash>>(&truncated).is_err());
    }

    #[test]
    fn batch_response_count_type_and_version_are_bound_to_exact_request() {
        for answer in [
            Answer::Receipts {
                states: ReceiptBatch::try_from(vec![ReceiptState::Received]).unwrap(),
                indexed_height: 1,
            },
            Answer::Receipt {
                state: finalized(),
                indexed_height: 1,
            },
            Answer::NovBalance(None),
        ] {
            let frame = Frame::Response(Response {
                request: batch_request(2),
                answer,
            });
            assert!(encode(frame.clone()).is_err());
            let mut bytes = MAGIC_V2.to_vec();
            bytes.extend(postcard::to_allocvec(&frame).unwrap());
            assert!(decode(&bytes).is_err());
        }
        let single = Frame::Request(Request {
            session: [7; 32],
            sequence: 1,
            query: Query::Status,
        });
        let mut bytes = encode(single).unwrap();
        bytes[..8].copy_from_slice(MAGIC_V2);
        assert!(decode(&bytes).is_err());
        let mut unknown = encode(Frame::Request(batch_request(1))).unwrap();
        unknown[7] = b'3';
        assert!(decode(&unknown).is_err());
    }

    #[test]
    fn original_v1_request_and_receipt_golden_bytes_are_unchanged() {
        // Frozen postcard v1 enum ordinals and fixed-width byte fields; these
        // expected bytes do not serialize the new Query/Frame implementation.
        for (query, query_bytes) in [
            (Query::Status, vec![0]),
            (
                Query::Receipt { hash: [3; 32] },
                [vec![1], vec![3; 32]].concat(),
            ),
            (
                Query::NovBalance {
                    account: AccountId::Bytes20([4; 20]),
                },
                [vec![2, 0], vec![4; 20]].concat(),
            ),
            (
                Query::NovBalance {
                    account: AccountId::Bytes32([5; 32]),
                },
                [vec![2, 1], vec![5; 32]].concat(),
            ),
        ] {
            let request = Request {
                session: [7; 32],
                sequence: 1,
                query,
            };
            let mut expected = MAGIC_V1.to_vec();
            expected.push(0); // Frame::Request
            expected.extend([7; 32]);
            expected.push(1); // sequence
            expected.extend(&query_bytes);
            assert_eq!(encode(Frame::Request(request.clone())).unwrap(), expected);
            assert_eq!(decode(&expected).unwrap(), Frame::Request(request.clone()));
            expected[8] = 1; // Frame::Response echoes exactly the same request
            expected.extend([3, 0]); // Answer::Error(QueryError::Busy)
            assert_eq!(
                encode(Frame::Response(Response {
                    request,
                    answer: Answer::Error(QueryError::Busy)
                }))
                .unwrap(),
                expected
            );
        }
        let frame = Frame::Response(Response {
            request: Request {
                session: [7; 32],
                sequence: 1,
                query: Query::Receipt { hash: [3; 32] },
            },
            answer: Answer::Receipt {
                state: ReceiptState::Finalized {
                    success: false,
                    nonce_after: 2,
                    charged_fee: 3,
                    height: 4,
                    block_hash: [9; 32],
                },
                indexed_height: 4,
            },
        });
        let expected = [
            MAGIC_V1.to_vec(),
            vec![1],
            vec![7; 32],
            vec![1, 1],
            vec![3; 32],
            vec![1, 3, 0, 2, 3, 4],
            vec![9; 32],
            vec![4],
        ]
        .concat();
        assert_eq!(encode(frame.clone()).unwrap(), expected);
        assert_eq!(decode(&expected).unwrap(), frame);
        let mut oversized = expected;
        oversized.resize(MAX_V1_BYTES + 1, 0);
        assert!(decode(&oversized).is_err());
    }
}
