//! Native application reads: typed requests and compact, request-bound v6 replies.
//! Binary, bounded, read-only; carried by
//! the existing authenticated NovoRUDP session, not a second transport or ledger.
//! Replies are the authenticated serving node's observations, NOT light-client
//! execution proofs. Received is volatile; NotRecent is not proof of absence.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const STREAM: u64 = u64::from_be_bytes(*b"NVQUERY1");
pub const MAX_V1_BYTES: usize = 1024;
pub const MAX_RECEIPTS: usize = 128;
// Each finalized state uses at most 73 bytes (tag/bool/u64/u128/u64/hash).
// Keep the existing admission budget; compact replies remove the echoed hash
// list without increasing either the maximum row count or numeric widths.
pub const MAX_BYTES: usize = 32 * 1024;
const MAGIC_V1: &[u8; 8] = b"NVQUERY1";
const MAGIC_V2: &[u8; 8] = b"NVQUERY2";
const MAGIC_V4: &[u8; 8] = b"NVQUERY4";
const MAGIC_RESPONSE_V6: &[u8; 8] = b"NVQUERY6";
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
    /// Order is part of the complete request commitment in the reply.
    Receipts {
        hashes: ReceiptBatch<Hash>,
    },
    /// Read-only observation at a published root; NEVER a spend permission.
    /// Concrete accounts and authority mode are committed by the v3 intent.
    AssetSpendState {
        intent: AssetReadIntentV2,
    },
}

/// The original NUAS intent bytes, bounded and validated BEFORE ownership.
/// This is neither a signed transaction nor a new signing transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetReadIntentV2(Vec<u8>);
impl AssetReadIntentV2 {
    pub fn new(intent: &crate::uca_spend::AssetSpendIntentV3) -> Result<Self, &'static str> {
        intent
            .signing_bytes()
            .map(Self)
            .map_err(|_| "invalid UCA read intent")
    }
    pub fn intent(&self) -> crate::uca_spend::AssetSpendIntentV3 {
        crate::uca_spend::AssetSpendIntentV3::decode_signing_bytes(&self.0)
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
                crate::uca_spend::AssetSpendIntentV3::decode_signing_bytes(value)
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
    pub authority: crate::uca_spend::AssetSpendAuthorityV3,
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

#[derive(Serialize, Deserialize)]
struct CompactResponse {
    session: Hash,
    sequence: u64,
    request_digest: Hash,
    answer: Answer,
}

fn request_digest(request: &Request) -> Result<Hash, &'static str> {
    let canonical = postcard::to_allocvec(request).map_err(|_| "native query encode")?;
    let mut digest = Sha256::new();
    digest.update(b"NOVOVM-NATIVE-QUERY-RESPONSE-V6\0");
    digest.update(canonical);
    Ok(digest.finalize().into())
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
        Frame::Response(Response { request, answer }) => answer_matches(request, answer),
        _ => true,
    }
}
fn answer_matches(request: &Request, answer: &Answer) -> bool {
    match (&request.query, answer) {
        (_, Answer::Error(_))
        | (Query::Status, Answer::Status(_))
        | (Query::Receipt { .. }, Answer::Receipt { .. })
        | (Query::NovBalance { .. }, Answer::NovBalance(_))
        | (Query::AssetSpendState { .. }, Answer::AssetSpendState(_)) => true,
        (Query::Receipts { hashes }, Answer::Receipts { states, .. }) => {
            hashes.len() == states.len()
        }
        _ => false,
    }
}
pub fn encode(frame: Frame) -> Result<Vec<u8>, &'static str> {
    if !valid(&frame) {
        return Err("invalid native query binding");
    }
    let batch = is_batch(&frame);
    let mut bytes = if matches!(frame, Frame::Response(_)) {
        MAGIC_RESPONSE_V6
    } else if is_uca(&frame) {
        MAGIC_V4
    } else if batch {
        MAGIC_V2
    } else {
        MAGIC_V1
    }
    .to_vec();
    let encoded = match frame {
        Frame::Response(response) => postcard::to_allocvec(&CompactResponse {
            session: response.request.session,
            sequence: response.request.sequence,
            request_digest: request_digest(&response.request)?,
            answer: response.answer,
        }),
        request => postcard::to_allocvec(&request),
    }
    .map_err(|_| "native query encode")?;
    bytes.extend(encoded);
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
    if !tail.is_empty()
        || !matches!(frame, Frame::Request(_))
        || !valid(&frame)
        || is_batch(&frame) != batch
        || is_uca(&frame) != uca
    {
        return Err("native query trailing bytes/binding");
    }
    Ok(frame)
}

/// Reconstruct the typed reply only from the client's exact pending request.
/// A valid reply for another request is stale, not an answer to the current one.
/// There is no fallback to the former full-request response wire formats.
pub fn decode_response(bytes: &[u8], expected: &Request) -> Result<Option<Response>, &'static str> {
    if !bytes.starts_with(MAGIC_RESPONSE_V6) || bytes.len() > MAX_BYTES {
        return Err("native query response header/size");
    }
    let (response, tail): (CompactResponse, _) =
        postcard::take_from_bytes(&bytes[8..]).map_err(|_| "native query response decode")?;
    if !tail.is_empty() || response.session == [0; 32] || response.sequence == 0 {
        return Err("native query response trailing bytes/binding");
    }
    if response.session != expected.session
        || response.sequence != expected.sequence
        || response.request_digest != request_digest(expected)?
    {
        return Ok(None);
    }
    let limit = if matches!(expected.query, Query::Receipts { .. }) {
        MAX_BYTES
    } else {
        MAX_V1_BYTES
    };
    if bytes.len() > limit || !answer_matches(expected, &response.answer) {
        return Err("native query response size/answer binding");
    }
    Ok(Some(Response {
        request: expected.clone(),
        answer: response.answer,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_query_commits_authority_mode_and_rejects_the_old_grant_only_intent() {
        let delegated = uca_intent();
        let mut controller = delegated.clone();
        controller.authority = crate::uca_spend::AssetSpendAuthorityV3::Controller;
        let request = |intent: &crate::uca_spend::AssetSpendIntentV3| Request {
            session: [7; 32],
            sequence: 1,
            query: Query::AssetSpendState {
                intent: AssetReadIntentV2::new(intent).unwrap(),
            },
        };
        let delegated_request = request(&delegated);
        let controller_request = request(&controller);
        assert_ne!(
            request_digest(&delegated_request).unwrap(),
            request_digest(&controller_request).unwrap()
        );
        let reply = encode(Frame::Response(Response {
            request: controller_request.clone(),
            answer: Answer::AssetSpendState(AssetState::Pending),
        }))
        .unwrap();
        assert!(reply.starts_with(b"NVQUERY6"));
        assert!(decode_response(&reply, &controller_request)
            .unwrap()
            .is_some());
        assert!(decode_response(&reply, &delegated_request)
            .unwrap()
            .is_none());
        let mut old = delegated.signing_bytes().unwrap();
        old[4] = 2;
        old.remove(7);
        assert!(
            postcard::from_bytes::<AssetReadIntentV2>(&postcard::to_allocvec(&old).unwrap())
                .is_err()
        );
    }

    fn assert_response_round_trip(bytes: &[u8], frame: &Frame) {
        let Frame::Response(response) = frame else {
            panic!("expected response fixture");
        };
        assert!(bytes.starts_with(MAGIC_RESPONSE_V6));
        assert_eq!(
            decode_response(bytes, &response.request).unwrap(),
            Some(response.clone())
        );
        assert!(decode(bytes).is_err(), "request ingress rejects a reply");
    }

    fn uca_intent() -> crate::uca_spend::AssetSpendIntentV3 {
        crate::uca_spend::AssetSpendIntentV3 {
            authority: crate::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
            asset: 1,
            authority_update: None,
            recipient_creation: None,
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
            crate::uca_spend::AssetSpendIntentV3::decode_signing_bytes(&raw).unwrap(),
            intent
        );
        for len in 0..raw.len() {
            assert!(
                crate::uca_spend::AssetSpendIntentV3::decode_signing_bytes(&raw[..len]).is_err()
            );
        }
        let mut tail = raw.clone();
        tail.push(0);
        assert!(crate::uca_spend::AssetSpendIntentV3::decode_signing_bytes(&tail).is_err());
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
                authority: crate::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
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
            assert_response_round_trip(&bytes, &frame);
            for label in [MAGIC_V1, MAGIC_V2, b"NVQUERY3"] {
                let mut wrong = bytes.clone();
                wrong[..8].copy_from_slice(label);
                assert!(decode(&wrong).is_err());
                assert!(decode_response(&wrong, &request).is_err());
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
            raw[7] = 255;
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
        assert_response_round_trip(&bytes, &frame);
        for n in 0..bytes.len() {
            assert!(decode_response(&bytes[..n], &request).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_response(&trailing, &request).is_err());
        assert!(decode_response(&vec![0; MAX_BYTES + 1], &request).is_err());
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
            assert!(bytes.len() <= MAX_RECEIPTS * 73 + 128);
            assert_response_round_trip(&bytes, &frame);
            assert_eq!(
                decode(&encode(Frame::Request(request.clone())).unwrap()).unwrap(),
                Frame::Request(request.clone())
            );
            let error = Frame::Response(Response {
                request: request.clone(),
                answer: Answer::Error(QueryError::Busy),
            });
            assert_response_round_trip(&encode(error.clone()).unwrap(), &error);
            let mut old_label = bytes.clone();
            old_label[..8].copy_from_slice(MAGIC_V1);
            assert!(decode_response(&old_label, &request).is_err());
            let mut trailing = bytes;
            trailing.push(0);
            assert!(decode_response(&trailing, &request).is_err());
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
            let Frame::Response(response) = frame else {
                unreachable!();
            };
            let mut bytes = MAGIC_RESPONSE_V6.to_vec();
            bytes.extend(
                postcard::to_allocvec(&CompactResponse {
                    session: response.request.session,
                    sequence: response.request.sequence,
                    request_digest: request_digest(&response.request).unwrap(),
                    answer: response.answer,
                })
                .unwrap(),
            );
            assert!(decode_response(&bytes, &response.request).is_err());
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
    fn request_golden_bytes_are_unchanged_and_old_responses_are_rejected() {
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
            let mut digest = Sha256::new();
            digest.update(b"NOVOVM-NATIVE-QUERY-RESPONSE-V6\0");
            digest.update(&expected[9..]); // Frozen complete Request, no Frame tag.
            let compact = [
                MAGIC_RESPONSE_V6.to_vec(),
                vec![7; 32],
                vec![1],
                digest.finalize().to_vec(),
                vec![3, 0], // Answer::Error(QueryError::Busy)
            ]
            .concat();
            expected[8] = 1; // Frame::Response echoes exactly the same request
            expected.extend([3, 0]); // Answer::Error(QueryError::Busy)
            assert!(decode(&expected).is_err());
            assert!(decode_response(&expected, &request).is_err());
            assert_eq!(
                encode(Frame::Response(Response {
                    request: request.clone(),
                    answer: Answer::Error(QueryError::Busy)
                }))
                .unwrap(),
                compact
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
        let Frame::Response(response) = &frame else {
            unreachable!();
        };
        assert!(decode(&expected).is_err());
        assert!(decode_response(&expected, &response.request).is_err());
        assert_response_round_trip(&encode(frame.clone()).unwrap(), &frame);
        let mut oversized = expected;
        oversized.resize(MAX_V1_BYTES + 1, 0);
        assert!(decode(&oversized).is_err());
    }

    #[test]
    fn compact_batch_wire_removes_only_echoed_request_and_preserves_row_order() {
        let mut request = batch_request(MAX_RECEIPTS);
        request.sequence = 1;
        let response = Response {
            request: request.clone(),
            answer: Answer::Receipts {
                states: ReceiptBatch::try_from(vec![ReceiptState::Received; MAX_RECEIPTS]).unwrap(),
                indexed_height: 1,
            },
        };
        let frame = Frame::Response(response);
        let compact = encode(frame.clone()).unwrap();
        let old_bytes = 8 + postcard::to_allocvec(&frame).unwrap().len();
        assert_eq!(old_bytes, 4273);
        assert_eq!(compact.len(), 205);
        assert_eq!(old_bytes - compact.len(), 4068);
        assert_response_round_trip(&compact, &frame);

        let frame = Frame::Response(Response {
            request: batch_request(MAX_RECEIPTS),
            answer: Answer::Receipts {
                states: ReceiptBatch::try_from(vec![finalized(); MAX_RECEIPTS]).unwrap(),
                indexed_height: u64::MAX,
            },
        });
        let compact = encode(frame.clone()).unwrap();
        assert_eq!(compact.len(), 9439);
        assert_response_round_trip(&compact, &frame);

        let frame = Frame::Response(Response {
            request: batch_request(4),
            answer: Answer::Receipts {
                states: ReceiptBatch::try_from(vec![
                    ReceiptState::Received,
                    finalized(),
                    ReceiptState::NotRecent,
                    ReceiptState::RejectedNonce,
                ])
                .unwrap(),
                indexed_height: 3,
            },
        });
        assert_response_round_trip(&encode(frame.clone()).unwrap(), &frame);
    }

    #[test]
    fn compact_response_binds_complete_request_not_only_session_and_sequence() {
        let request = batch_request(2);
        let bytes = encode(Frame::Response(Response {
            request: request.clone(),
            answer: Answer::Receipts {
                states: ReceiptBatch::try_from(vec![ReceiptState::Received; 2]).unwrap(),
                indexed_height: 1,
            },
        }))
        .unwrap();
        for change in 0..6 {
            let mut wrong = request.clone();
            match change {
                0 => wrong.session[0] ^= 1,
                1 => wrong.sequence -= 1,
                2 => wrong.query = Query::Status,
                3..=5 => {
                    let Query::Receipts { hashes } = &mut wrong.query else {
                        unreachable!();
                    };
                    match change {
                        3 => hashes.0[0][31] ^= 1,
                        4 => hashes.0.reverse(),
                        _ => {
                            hashes.0.pop();
                        }
                    }
                }
                _ => unreachable!(),
            }
            assert_eq!(decode_response(&bytes, &wrong).unwrap(), None);
        }
        for query in [
            Query::NovBalance {
                account: AccountId::Bytes20([3; 20]),
            },
            Query::AssetSpendState {
                intent: AssetReadIntentV2::new(&uca_intent()).unwrap(),
            },
        ] {
            let mut expected = Request {
                session: [7; 32],
                sequence: 1,
                query,
            };
            let bytes = encode(Frame::Response(Response {
                request: expected.clone(),
                answer: Answer::Error(QueryError::Unavailable),
            }))
            .unwrap();
            match &mut expected.query {
                Query::NovBalance { account } => *account = AccountId::Bytes32([3; 32]),
                Query::AssetSpendState { intent } => {
                    let mut other = intent.intent();
                    other.chain_id += 1;
                    *intent = AssetReadIntentV2::new(&other).unwrap();
                }
                _ => unreachable!(),
            }
            assert_eq!(decode_response(&bytes, &expected).unwrap(), None);
        }
        let mut bad_digest = bytes;
        // Eight-byte magic, fixed session, and ten-byte u64::MAX sequence.
        bad_digest[8 + 32 + 10] ^= 1;
        assert_eq!(decode_response(&bad_digest, &request).unwrap(), None);
    }

    #[test]
    fn compact_response_rejects_oversize_malformed_counts_and_all_old_response_versions() {
        let request = batch_request(2);
        for count in [0, 1, MAX_RECEIPTS + 1] {
            let compact = CompactResponse {
                session: request.session,
                sequence: request.sequence,
                request_digest: request_digest(&request).unwrap(),
                answer: Answer::Receipts {
                    states: ReceiptBatch(vec![ReceiptState::Received; count]),
                    indexed_height: 1,
                },
            };
            let mut bytes = MAGIC_RESPONSE_V6.to_vec();
            bytes.extend(postcard::to_allocvec(&compact).unwrap());
            assert!(decode_response(&bytes, &request).is_err());
        }
        let frame = Frame::Response(Response {
            request: request.clone(),
            answer: Answer::Error(QueryError::Busy),
        });
        let bytes = encode(frame.clone()).unwrap();
        for length in 0..bytes.len() {
            assert!(decode_response(&bytes[..length], &request).is_err());
        }
        let mut oversized = bytes;
        oversized.resize(MAX_BYTES + 1, 0);
        assert!(decode_response(&oversized, &request).is_err());
        for magic in [MAGIC_V1, MAGIC_V2, MAGIC_V4, b"NVQUERY5"] {
            let mut legacy = magic.to_vec();
            legacy.extend(postcard::to_allocvec(&frame).unwrap());
            assert!(decode(&legacy).is_err());
            assert!(decode_response(&legacy, &request).is_err());
        }
        let request_bytes = encode(Frame::Request(request.clone())).unwrap();
        assert!(decode_response(&request_bytes, &request).is_err());
    }
}
