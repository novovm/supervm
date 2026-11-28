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
const MAGIC_IDENTITY_V7: &[u8; 8] = b"NVQUERY7";
const MAGIC_APPLICATION_V8: &[u8; 8] = b"NVQUERY8";
const MAGIC_PROPOSAL_V9: &[u8; 8] = b"NVQUERY9";
const MAGIC_ARCHIVE_V10: &[u8; 8] = b"NVQARC10";
const MAGIC_STATE_V11: &[u8; 8] = b"NVQSTA11";
const MAGIC_REGISTRATION_V12: &[u8; 8] = b"NVQREG12";
const MAX_PROPOSAL_REPLY_BYTES: usize = 4096;
// Room for the canonical ML-DSA-65 round-BFT proposal, including its prefix.
const MAX_SIGNED_PROPOSAL_BYTES: usize = 3840;
const MAGIC_RESPONSE_V6: &[u8; 8] = b"NVQUERY6";
pub type Hash = [u8; 32];
pub const ARCHIVE_CHUNK_BYTES: usize = 24 * 1024;
pub const MAX_ARCHIVE_PROOF_BYTES: usize = 32 * 1024 * 1024;

mod archive;
mod state_sync;
pub use archive::{ArchiveChunk, ArchiveSelector};
pub use state_sync::{
    StateNodePayload, StateNodeSelector, MAX_STATE_NODE_BYTES, STATE_BRANCH_BYTES,
};

#[cfg(test)]
#[path = "native_query/application_tests.rs"]
mod application_tests;

/// Immutable identity identifier, not an asset account or spending authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UcaIdentityIdV1(String);
impl UcaIdentityIdV1 {
    pub fn new(value: &str) -> Result<Self, &'static str> {
        if value.is_empty()
            || value.len() > crate::uca_spend::MAX_UCA_ID_BYTES
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        {
            return Err("invalid UCA identity ID");
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Serialize for UcaIdentityIdV1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for UcaIdentityIdV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdentityVisitor;
        impl<'de> serde::de::Visitor<'de> for IdentityVisitor {
            type Value = UcaIdentityIdV1;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("nonempty bounded ASCII UCA identity ID")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                UcaIdentityIdV1::new(value).map_err(E::custom)
            }
            fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                if value.len() > crate::uca_spend::MAX_UCA_ID_BYTES {
                    return Err(E::custom("invalid UCA identity ID"));
                }
                self.visit_str(std::str::from_utf8(value).map_err(E::custom)?)
            }
        }
        deserializer.deserialize_str(IdentityVisitor)
    }
}

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
    UcaIdentity {
        chain_id: u64,
        genesis: Hash,
        identity: UcaIdentityIdV1,
    },
    /// Current published application authority, never a device login proof.
    UcaApplicationGrant {
        selector: crate::uca_application::ApplicationGrantSelectorV1,
    },
    /// Current local consensus evidence for exactly one transaction.
    ConsensusProposal {
        height: u64,
        round: u64,
        hash: Hash,
    },
    /// Same selector, with the serving owner's current signing progress.
    ConsensusProposalProgress {
        height: u64,
        round: u64,
        hash: Hash,
    },
    /// Bytes of the existing asset-receipt inclusion and finality proof.
    ReceiptProof(ArchiveSelector),
    FinalizedHeader(ArchiveSelector),
    StateNode(StateNodeSelector),
    /// Serving-node intake and recent publication for SHA-256 of signed NUIR bytes.
    UcaRegistration {
        request_hash: Hash,
    },
}

impl Query {
    pub fn proposal_selector(&self) -> Option<(u64, u64, Hash)> {
        match *self {
            Self::ConsensusProposal {
                height,
                round,
                hash,
            }
            | Self::ConsensusProposalProgress {
                height,
                round,
                hash,
            } => Some((height, round, hash)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityStatusV1 {
    Active,
    Suspended,
    Recovering,
    Revoked,
}

/// Serving-node observation at one published root, not an identity proof.
/// Exposes only a key reference, never raw keys or linked asset-account data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UcaIdentityViewV1 {
    pub state_root: Hash,
    pub height: u64,
    pub status: IdentityStatusV1,
    pub primary_key_ref: Hash,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityStateV1 {
    Pending,
    Missing { state_root: Hash, height: u64 },
    Present(UcaIdentityViewV1),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistrationStatusV1 {
    NotRecent,
    Received,
    Verified,
    RetryQueueFull,
    RetryVerification,
    OutsideValidityWindow,
    IdentityAlreadyRegistered,
    Finalized { height: u64, block_hash: Hash },
}

/// Metadata at one published root. Possession, head trust and project/tool
/// permissions must be established independently by the application owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationGrantViewV1 {
    pub state_root: Hash,
    pub height: u64,
    pub revision: u64,
    pub issuer_key_ref: Hash,
    pub identity_key_ref: Hash,
    pub identity_status: IdentityStatusV1,
    pub revoked: bool,
    pub not_before_height: u64,
    pub expires_at_height: u64,
}

impl ApplicationGrantViewV1 {
    /// Checks grant metadata for its already authenticated selector and root.
    /// The caller must correlate the view with the exact selector, establish
    /// root trust and freshness, and verify possession of the device key.
    pub fn permits_device_at_observed_height(
        &self,
        selector: &crate::uca_application::ApplicationGrantSelectorV1,
        expected_revision: u64,
    ) -> bool {
        selector.validate().is_ok()
            && self.state_root != [0; 32]
            && self.revision > 0
            && self.revision == expected_revision
            && !self.revoked
            && self.identity_status == IdentityStatusV1::Active
            && self.issuer_key_ref != [0; 32]
            && self.issuer_key_ref == self.identity_key_ref
            && self.issuer_key_ref != selector.device
            && self.not_before_height <= self.height
            && self.height < self.expires_at_height
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationGrantStateV1 {
    Pending,
    IdentityMissing { state_root: Hash, height: u64 },
    Missing { state_root: Hash, height: u64 },
    Present(ApplicationGrantViewV1),
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
/// Stages of the existing outbound proposal object on this process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalDispatch {
    Unobserved,
    Prepared,
    /// The transport took custody. Wire transmission remains unconfirmed.
    Admitted,
    /// All initial proposal fragments were written by the original sender.
    Sent,
    /// A peer returned authenticated transport completion for the proposal.
    Delivered,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedProposalObservation {
    pub proposer: Hash,
    /// Canonical original round-BFT proposal bytes, including its signature.
    #[serde(with = "signed_proposal_bytes")]
    pub wire: Vec<u8>,
    pub dispatch: ProposalDispatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateObservation {
    pub value: Hash,
    pub body_id: Hash,
    pub signed_proposal: Option<SignedProposalObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalObservation {
    pub in_pending_pool: bool,
    /// Last selection gate at this position. This is a batch-level reason.
    #[serde(deserialize_with = "decode_selection_gate")]
    pub selection_gate: Option<String>,
    /// Only present when the local offer for this exact position is retained.
    pub in_local_offer: Option<bool>,
    pub candidate: Option<CandidateObservation>,
}

mod signed_proposal_bytes {
    use super::*;

    pub(super) fn serialize<S: serde::Serializer>(
        wire: &[u8],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if wire.is_empty() || wire.len() > MAX_SIGNED_PROPOSAL_BYTES {
            return Err(serde::ser::Error::custom("invalid signed proposal length"));
        }
        serializer.serialize_bytes(wire)
    }

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let wire = <&[u8]>::deserialize(d)?;
        if wire.is_empty() || wire.len() > MAX_SIGNED_PROPOSAL_BYTES {
            return Err(serde::de::Error::custom("invalid signed proposal length"));
        }
        Ok(wire.to_vec())
    }
}

fn decode_selection_gate<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    let gate = Option::<&str>::deserialize(d)?;
    if gate.is_some_and(|reason| reason.len() > 64) {
        return Err(serde::de::Error::custom("invalid selection gate length"));
    }
    Ok(gate.map(str::to_owned))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusProposalStatus {
    pub current_height: u64,
    pub current_round: u64,
    /// The node retains a current-position view, not a history.
    pub observation: Option<ProposalObservation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStep {
    Propose,
    Prevote,
    Precommit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalJournalAction {
    Proposal { round: u64, value: Hash },
    Prevote,
    Precommit,
    Decision,
    AdvanceRound { round: u64 },
    AdvanceHeight { height: u64 },
    StateOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalJournalTransition {
    pub action: ProposalJournalAction,
    /// False means staged before obtaining a write ticket. True means the
    /// owner has not consumed the acknowledgement for the submitted write.
    pub submitted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalPrevoteQc {
    pub round: u64,
    pub value: Hash,
    /// The original proposal path can look up this exact value in its candidate set.
    pub candidate_ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalProgress {
    pub step: ProposalStep,
    pub pending_journal: Option<ProposalJournalTransition>,
    pub proposed_round: Option<u64>,
    /// The journal's retained, verified valid-value prevote QC.
    pub prevote_qc: Option<ProposalPrevoteQc>,
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
    UcaIdentity(IdentityStateV1),
    UcaApplicationGrant(ApplicationGrantStateV1),
    ConsensusProposal(ConsensusProposalStatus),
    ConsensusProposalProgress {
        status: ConsensusProposalStatus,
        progress: Option<ProposalProgress>,
    },
    ReceiptProof(ArchiveChunk),
    FinalizedHeader(ArchiveChunk),
    StateNode(StateNodePayload),
    UcaRegistration(RegistrationStatusV1),
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
fn is_identity(frame: &Frame) -> bool {
    matches!(request(frame).query, Query::UcaIdentity { .. })
}
fn is_application(frame: &Frame) -> bool {
    matches!(request(frame).query, Query::UcaApplicationGrant { .. })
}
fn is_registration(frame: &Frame) -> bool {
    matches!(request(frame).query, Query::UcaRegistration { .. })
}
fn is_proposal(query: &Query) -> bool {
    query.proposal_selector().is_some()
}
fn is_archive(query: &Query) -> bool {
    matches!(query, Query::ReceiptProof(_) | Query::FinalizedHeader(_))
}
fn reply_limit(query: &Query) -> usize {
    match query {
        Query::Receipts { .. } | Query::ReceiptProof(_) | Query::FinalizedHeader(_) => MAX_BYTES,
        Query::ConsensusProposal { .. } | Query::ConsensusProposalProgress { .. } => {
            MAX_PROPOSAL_REPLY_BYTES
        }
        _ => MAX_V1_BYTES,
    }
}
fn valid_request(request: &Request) -> bool {
    if request.session == [0; 32] || request.sequence == 0 {
        return false;
    }
    if let Query::UcaIdentity {
        chain_id, genesis, ..
    } = &request.query
    {
        if *chain_id == 0 || *genesis == [0; 32] {
            return false;
        }
    }
    if let Query::UcaApplicationGrant { selector } = &request.query {
        if selector.validate().is_err() {
            return false;
        }
    }
    if let Some((height, _, hash)) = request.query.proposal_selector() {
        if height == 0 || hash == [0; 32] {
            return false;
        }
    }
    if let Query::ReceiptProof(selector) | Query::FinalizedHeader(selector) = &request.query {
        return selector.is_valid();
    }
    if let Query::StateNode(selector) = &request.query {
        return selector.is_valid();
    }
    if let Query::UcaRegistration { request_hash } = &request.query {
        return *request_hash != [0; 32];
    }
    true
}
fn valid(frame: &Frame) -> bool {
    if !valid_request(request(frame)) {
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
        | (Query::AssetSpendState { .. }, Answer::AssetSpendState(_))
        | (Query::UcaIdentity { .. }, Answer::UcaIdentity(_)) => true,
        (Query::UcaRegistration { .. }, Answer::UcaRegistration(status)) => match status {
            RegistrationStatusV1::Finalized { height, block_hash } => {
                *height > 0 && *block_hash != [0; 32]
            }
            _ => true,
        },
        (Query::UcaApplicationGrant { .. }, Answer::UcaApplicationGrant(state)) => match state {
            ApplicationGrantStateV1::Pending => true,
            ApplicationGrantStateV1::IdentityMissing { state_root, .. }
            | ApplicationGrantStateV1::Missing { state_root, .. } => *state_root != [0; 32],
            ApplicationGrantStateV1::Present(view) => {
                view.state_root != [0; 32]
                    && view.revision > 0
                    && (view.revision < u64::MAX || view.revoked)
                    && view.issuer_key_ref != [0; 32]
                    && view.identity_key_ref != [0; 32]
                    && view.expires_at_height > view.not_before_height
            }
        },
        (Query::ConsensusProposal { height, round, .. }, Answer::ConsensusProposal(status)) => {
            proposal_status_matches(*height, *round, status)
        }
        (
            Query::ConsensusProposalProgress { height, round, .. },
            Answer::ConsensusProposalProgress { status, progress },
        ) => {
            progress.is_some() == status.observation.is_some()
                && proposal_status_matches(*height, *round, status)
        }
        (Query::Receipts { hashes }, Answer::Receipts { states, .. }) => {
            hashes.len() == states.len()
        }
        (Query::ReceiptProof(selector), Answer::ReceiptProof(chunk))
        | (Query::FinalizedHeader(selector), Answer::FinalizedHeader(chunk)) => {
            chunk.matches(selector)
        }
        (Query::StateNode(_), Answer::StateNode(node)) => node.is_valid(),
        _ => false,
    }
}
fn proposal_status_matches(height: u64, round: u64, status: &ConsensusProposalStatus) -> bool {
    status.current_height > 0
        && status.observation.is_some()
            == (height == status.current_height && round == status.current_round)
        && status.observation.as_ref().is_none_or(|observation| {
            observation
                .selection_gate
                .as_ref()
                .is_none_or(|gate| gate.len() <= 64)
                && observation.candidate.as_ref().is_none_or(|candidate| {
                    candidate.value != [0; 32]
                        && candidate.body_id != [0; 32]
                        && candidate.signed_proposal.as_ref().is_none_or(|proposal| {
                            proposal.proposer != [0; 32] && !proposal.wire.is_empty()
                        })
                })
        })
}

pub fn encode(frame: Frame) -> Result<Vec<u8>, &'static str> {
    if !valid(&frame) {
        return Err("invalid native query binding");
    }
    let response = matches!(frame, Frame::Response(_));
    let batch = is_batch(&frame);
    let state = matches!(request(&frame).query, Query::StateNode(_));
    let limit = if response {
        reply_limit(&request(&frame).query)
    } else if batch || state {
        MAX_BYTES
    } else {
        MAX_V1_BYTES
    };
    let mut bytes = if response {
        MAGIC_RESPONSE_V6
    } else if is_registration(&frame) {
        MAGIC_REGISTRATION_V12
    } else if state {
        MAGIC_STATE_V11
    } else if is_archive(&request(&frame).query) {
        MAGIC_ARCHIVE_V10
    } else if is_proposal(&request(&frame).query) {
        MAGIC_PROPOSAL_V9
    } else if is_application(&frame) {
        MAGIC_APPLICATION_V8
    } else if is_identity(&frame) {
        MAGIC_IDENTITY_V7
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
    if bytes.len() > limit {
        return Err("native query size");
    }
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<Frame, &'static str> {
    let uca = bytes.starts_with(MAGIC_V4);
    let identity = bytes.starts_with(MAGIC_IDENTITY_V7);
    let application = bytes.starts_with(MAGIC_APPLICATION_V8);
    let proposal = bytes.starts_with(MAGIC_PROPOSAL_V9);
    let archive = bytes.starts_with(MAGIC_ARCHIVE_V10);
    let state = bytes.starts_with(MAGIC_STATE_V11);
    let registration = bytes.starts_with(MAGIC_REGISTRATION_V12);
    let batch = if bytes.starts_with(MAGIC_V1) {
        false
    } else if bytes.starts_with(MAGIC_V2) {
        true
    } else if uca || identity || application || proposal || archive || state || registration {
        false
    } else {
        return Err("native query header/size");
    };
    if bytes.len()
        > if batch || state {
            MAX_BYTES
        } else {
            MAX_V1_BYTES
        }
    {
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
        || is_identity(&frame) != identity
        || is_application(&frame) != application
        || is_registration(&frame) != registration
        || is_proposal(&request(&frame).query) != proposal
        || is_archive(&request(&frame).query) != archive
        || matches!(request(&frame).query, Query::StateNode(_)) != state
    {
        return Err("native query trailing bytes/binding");
    }
    Ok(frame)
}

/// Reconstruct the typed reply only from the client's exact pending request.
/// A valid reply for another request is stale, not an answer to the current one.
/// There is no fallback to the former full-request response wire formats.
pub fn decode_response(bytes: &[u8], expected: &Request) -> Result<Option<Response>, &'static str> {
    if !valid_request(expected) {
        return Err("invalid native query binding");
    }
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
    let limit = reply_limit(&expected.query);
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

    fn identity_request() -> Request {
        Request {
            session: [7; 32],
            sequence: u64::MAX,
            query: Query::UcaIdentity {
                chain_id: u64::MAX,
                genesis: [2; 32],
                identity: UcaIdentityIdV1::new("uca:Alice_01.test-id").unwrap(),
            },
        }
    }

    #[test]
    fn registration_status_is_bound_to_the_signed_request_hash() {
        let request = Request {
            query: Query::UcaRegistration {
                request_hash: [9; 32],
            },
            ..identity_request()
        };
        let frame = Frame::Request(request.clone());
        let mut wire = encode(frame.clone()).unwrap();
        assert!(wire.starts_with(MAGIC_REGISTRATION_V12));
        assert_eq!(decode(&wire).unwrap(), frame);
        wire[..8].copy_from_slice(MAGIC_IDENTITY_V7);
        assert!(decode(&wire).is_err());
        let other = Request {
            query: Query::UcaRegistration {
                request_hash: [8; 32],
            },
            ..request.clone()
        };
        for status in [
            RegistrationStatusV1::RetryQueueFull,
            RegistrationStatusV1::Finalized {
                height: 10,
                block_hash: [7; 32],
            },
        ] {
            let response = Response {
                request: request.clone(),
                answer: Answer::UcaRegistration(status),
            };
            let wire = encode(Frame::Response(response.clone())).unwrap();
            assert_eq!(decode_response(&wire, &request).unwrap(), Some(response));
            assert_eq!(decode_response(&wire, &other).unwrap(), None);
        }
    }

    #[test]
    fn identity_request_round_trip_and_exclusive_v7_header() {
        let request = identity_request();
        let frame = Frame::Request(request.clone());
        let bytes = encode(frame.clone()).unwrap();
        assert!(bytes.starts_with(MAGIC_IDENTITY_V7));
        assert_eq!(decode(&bytes).unwrap(), frame);
        for header in [MAGIC_V1, MAGIC_V2, MAGIC_V4, MAGIC_RESPONSE_V6, b"NVQUERY3"] {
            let mut wrong = bytes.clone();
            wrong[..8].copy_from_slice(header);
            assert!(decode(&wrong).is_err());
        }
        for (query, header) in [
            (Query::Status, MAGIC_V1),
            (batch_request(1).query, MAGIC_V2),
            (
                Query::AssetSpendState {
                    intent: AssetReadIntentV2::new(&uca_intent()).unwrap(),
                },
                MAGIC_V4,
            ),
        ] {
            let legacy = Frame::Request(Request {
                query,
                ..request.clone()
            });
            let mut wire = encode(legacy.clone()).unwrap();
            assert_eq!(&wire[..8], header);
            assert_eq!(&wire[8..], postcard::to_allocvec(&legacy).unwrap());
            wire[..8].copy_from_slice(MAGIC_IDENTITY_V7);
            assert!(decode(&wire).is_err());
        }
        for n in 0..bytes.len() {
            assert!(decode(&bytes[..n]).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode(&trailing).is_err());
    }

    #[test]
    fn identity_observations_round_trip_all_statuses_without_account_or_raw_key_data() {
        let request = identity_request();
        let mut states = vec![
            IdentityStateV1::Pending,
            IdentityStateV1::Missing {
                state_root: [9; 32],
                height: u64::MAX,
            },
        ];
        for (tag, status) in [
            IdentityStatusV1::Active,
            IdentityStatusV1::Suspended,
            IdentityStatusV1::Recovering,
            IdentityStatusV1::Revoked,
        ]
        .into_iter()
        .enumerate()
        {
            let view = UcaIdentityViewV1 {
                state_root: [9; 32],
                height: u64::MAX,
                status,
                primary_key_ref: [6; 32],
                created_at_ms: u64::MAX - 1,
                updated_at_ms: u64::MAX,
            };
            // Exact public wire schema: two hashes, status and three integers.
            // No raw key, asset account, balance, grant or spending fields.
            let expected = [
                vec![9; 32],
                postcard::to_allocvec(&u64::MAX).unwrap(),
                vec![tag as u8],
                vec![6; 32],
                postcard::to_allocvec(&(u64::MAX - 1)).unwrap(),
                postcard::to_allocvec(&u64::MAX).unwrap(),
            ]
            .concat();
            assert_eq!(postcard::to_allocvec(&view).unwrap(), expected);
            states.push(IdentityStateV1::Present(view));
        }
        for state in states {
            let frame = Frame::Response(Response {
                request: request.clone(),
                answer: Answer::UcaIdentity(state),
            });
            let bytes = encode(frame.clone()).unwrap();
            assert_response_round_trip(&bytes, &frame);
            assert!(bytes.len() <= MAX_V1_BYTES);
            for n in 0..bytes.len() {
                assert!(decode_response(&bytes[..n], &request).is_err());
            }
            let mut wrong = bytes;
            wrong[..8].copy_from_slice(MAGIC_IDENTITY_V7);
            assert!(decode_response(&wrong, &request).is_err());
            assert!(decode(&wrong).is_err());
        }
    }

    #[test]
    fn identity_id_checks_borrowed_strings_and_bytes_before_ownership() {
        use serde::de::value::{BytesDeserializer, Error, StrDeserializer};
        let max = "a".repeat(crate::uca_spend::MAX_UCA_ID_BYTES);
        for value in ["Az09._:-", max.as_str()] {
            let id = UcaIdentityIdV1::new(value).unwrap();
            assert_eq!(id.as_str(), value);
            let encoded = postcard::to_allocvec(&id).unwrap();
            assert_eq!(encoded, postcard::to_allocvec(value).unwrap());
            assert_eq!(
                postcard::from_bytes::<UcaIdentityIdV1>(&encoded).unwrap(),
                id
            );
            assert_eq!(
                UcaIdentityIdV1::deserialize(StrDeserializer::<Error>::new(value)).unwrap(),
                id
            );
            assert_eq!(
                UcaIdentityIdV1::deserialize(BytesDeserializer::<Error>::new(value.as_bytes()))
                    .unwrap(),
                id
            );
        }
        let oversized = "a".repeat(crate::uca_spend::MAX_UCA_ID_BYTES + 1);
        for value in [
            "",
            "a b",
            "a/b",
            "a\0b",
            "a\nb",
            "\u{e9}",
            oversized.as_str(),
        ] {
            assert!(UcaIdentityIdV1::new(value).is_err());
            assert!(UcaIdentityIdV1::deserialize(StrDeserializer::<Error>::new(value)).is_err());
            assert!(
                UcaIdentityIdV1::deserialize(BytesDeserializer::<Error>::new(value.as_bytes()))
                    .is_err()
            );
            assert!(postcard::from_bytes::<UcaIdentityIdV1>(
                &postcard::to_allocvec(value).unwrap()
            )
            .is_err());
            let mut request = identity_request();
            let Query::UcaIdentity { identity, .. } = &mut request.query else {
                unreachable!()
            };
            // Bypass the public constructor only to manufacture hostile ingress.
            *identity = UcaIdentityIdV1(value.to_owned());
            let mut wire = MAGIC_IDENTITY_V7.to_vec();
            wire.extend(postcard::to_allocvec(&Frame::Request(request)).unwrap());
            assert!(decode(&wire).is_err());
        }
        assert!(UcaIdentityIdV1::deserialize(BytesDeserializer::<Error>::new(&[0xff])).is_err());
        for count in [crate::uca_spend::MAX_UCA_ID_BYTES + 1, usize::MAX] {
            let prefix = postcard::to_allocvec(&count).unwrap();
            assert!(postcard::from_bytes::<UcaIdentityIdV1>(&prefix).is_err());
        }
    }

    #[test]
    fn identity_domain_validation_and_complete_response_binding() {
        let request = identity_request();
        let bytes = encode(Frame::Response(Response {
            request: request.clone(),
            answer: Answer::UcaIdentity(IdentityStateV1::Pending),
        }))
        .unwrap();
        for change in 0..6 {
            let mut wrong = request.clone();
            let Query::UcaIdentity {
                chain_id,
                genesis,
                identity,
            } = &mut wrong.query
            else {
                unreachable!()
            };
            match change {
                0 => *chain_id -= 1,
                1 => genesis[0] ^= 1,
                2 => *identity = UcaIdentityIdV1::new("uca:alice_01.test-id").unwrap(),
                3 => wrong.session[0] ^= 1,
                4 => wrong.sequence -= 1,
                _ => wrong.query = Query::Status,
            }
            assert_eq!(decode_response(&bytes, &wrong).unwrap(), None);
        }
        for zero_chain in [true, false] {
            let mut invalid = request.clone();
            let Query::UcaIdentity {
                chain_id, genesis, ..
            } = &mut invalid.query
            else {
                unreachable!()
            };
            if zero_chain {
                *chain_id = 0;
            } else {
                *genesis = [0; 32];
            }
            let frame = Frame::Request(invalid.clone());
            assert!(encode(frame.clone()).is_err());
            let mut wire = MAGIC_IDENTITY_V7.to_vec();
            wire.extend(postcard::to_allocvec(&frame).unwrap());
            assert!(decode(&wire).is_err());
            assert!(encode(Frame::Response(Response {
                request: invalid.clone(),
                answer: Answer::UcaIdentity(IdentityStateV1::Pending),
            }))
            .is_err());
            assert!(decode_response(&bytes, &invalid).is_err());
        }
        for (request, answer) in [
            (request.clone(), Answer::NovBalance(None)),
            (
                Request {
                    query: Query::Status,
                    ..request.clone()
                },
                Answer::UcaIdentity(IdentityStateV1::Pending),
            ),
        ] {
            assert!(encode(Frame::Response(Response {
                request: request.clone(),
                answer: answer.clone()
            }))
            .is_err());
            let mut wire = MAGIC_RESPONSE_V6.to_vec();
            wire.extend(
                postcard::to_allocvec(&CompactResponse {
                    session: request.session,
                    sequence: request.sequence,
                    request_digest: request_digest(&request).unwrap(),
                    answer,
                })
                .unwrap(),
            );
            assert!(decode_response(&wire, &request).is_err());
        }
        let frame = Frame::Response(Response {
            request,
            answer: Answer::Error(QueryError::Unavailable),
        });
        assert_response_round_trip(&encode(frame.clone()).unwrap(), &frame);
    }

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
    fn consensus_proposal_progress_keeps_legacy_binding_and_reply_budget() {
        let query = Query::ConsensusProposalProgress {
            height: 1,
            round: u64::MAX,
            hash: [2; 32],
        };
        let request = Request {
            session: [1; 32],
            sequence: u64::MAX,
            query,
        };
        let request_frame = Frame::Request(request.clone());
        assert_eq!(
            decode(&encode(request_frame.clone()).unwrap()),
            Ok(request_frame)
        );
        let status = ConsensusProposalStatus {
            current_height: 1,
            current_round: u64::MAX,
            observation: Some(ProposalObservation {
                in_pending_pool: true,
                selection_gate: Some("a".repeat(64)),
                in_local_offer: Some(true),
                candidate: Some(CandidateObservation {
                    value: [3; 32],
                    body_id: [4; 32],
                    signed_proposal: Some(SignedProposalObservation {
                        proposer: [5; 32],
                        // Canonical signed proposal wire size with headroom.
                        wire: vec![7; 3587],
                        dispatch: ProposalDispatch::Sent,
                    }),
                }),
            }),
        };
        let progress = ProposalProgress {
            step: ProposalStep::Propose,
            pending_journal: Some(ProposalJournalTransition {
                action: ProposalJournalAction::Proposal {
                    round: u64::MAX,
                    value: [6; 32],
                },
                submitted: true,
            }),
            proposed_round: Some(u64::MAX),
            prevote_qc: Some(ProposalPrevoteQc {
                round: u64::MAX - 1,
                value: [8; 32],
                candidate_ready: false,
            }),
        };
        let reply = Response {
            request: request.clone(),
            answer: Answer::ConsensusProposalProgress {
                status: status.clone(),
                progress: Some(progress),
            },
        };
        let bytes = encode(Frame::Response(reply.clone())).unwrap();
        assert!(bytes.len() <= MAX_PROPOSAL_REPLY_BYTES);
        assert_eq!(decode_response(&bytes, &request), Ok(Some(reply.clone())));
        let legacy = Request {
            query: Query::ConsensusProposal {
                height: 1,
                round: u64::MAX,
                hash: [2; 32],
            },
            ..request.clone()
        };
        assert_eq!(decode_response(&bytes, &legacy), Ok(None));
        assert!(encode(Frame::Response(Response {
            request: legacy.clone(),
            ..reply
        }))
        .is_err());
        assert!(encode(Frame::Response(Response {
            request: request.clone(),
            answer: Answer::ConsensusProposalProgress {
                status: status.clone(),
                progress: None,
            },
        }))
        .is_err());
        let legacy_reply = Response {
            request: legacy.clone(),
            answer: Answer::ConsensusProposal(status.clone()),
        };
        let bytes = encode(Frame::Response(legacy_reply.clone())).unwrap();
        assert_eq!(decode_response(&bytes, &legacy), Ok(Some(legacy_reply)));
        assert_eq!(decode_response(&bytes, &request), Ok(None));

        let stale = Response {
            request: request.clone(),
            answer: Answer::ConsensusProposalProgress {
                status: ConsensusProposalStatus {
                    current_round: 0,
                    observation: None,
                    ..status
                },
                progress: None,
            },
        };
        let bytes = encode(Frame::Response(stale.clone())).unwrap();
        assert_eq!(decode_response(&bytes, &request), Ok(Some(stale)));
    }

    #[test]
    fn consensus_proposal_query_bounds_and_binds_exact_position_and_hash() {
        let request = Request {
            session: [1; 32],
            sequence: 1,
            query: Query::ConsensusProposal {
                height: 1,
                round: 580,
                hash: [2; 32],
            },
        };
        let mut frame = encode(Frame::Request(request.clone())).unwrap();
        assert!(frame.starts_with(MAGIC_PROPOSAL_V9));
        assert_eq!(decode(&frame), Ok(Frame::Request(request.clone())));
        frame[..8].copy_from_slice(MAGIC_V1);
        assert!(decode(&frame).is_err());

        let make_reply = |current_round, wire_len| Response {
            request: request.clone(),
            answer: Answer::ConsensusProposal(ConsensusProposalStatus {
                current_height: 1,
                current_round,
                observation: Some(ProposalObservation {
                    in_pending_pool: true,
                    selection_gate: Some("body_owner_accepted".into()),
                    in_local_offer: Some(true),
                    candidate: Some(CandidateObservation {
                        value: [3; 32],
                        body_id: [4; 32],
                        signed_proposal: Some(SignedProposalObservation {
                            proposer: [5; 32],
                            wire: vec![7; wire_len],
                            dispatch: ProposalDispatch::Sent,
                        }),
                    }),
                }),
            }),
        };
        let reply = make_reply(580, 3587);
        let bytes = encode(Frame::Response(reply.clone())).unwrap();
        assert!(bytes.len() > MAX_V1_BYTES);
        assert_eq!(decode_response(&bytes, &request), Ok(Some(reply)));
        for (round, hash) in [(581, [2; 32]), (580, [8; 32])] {
            let other = Request {
                query: Query::ConsensusProposal {
                    height: 1,
                    round,
                    hash,
                },
                ..request.clone()
            };
            assert_eq!(decode_response(&bytes, &other), Ok(None));
        }
        assert!(encode(Frame::Response(make_reply(581, 3587))).is_err());
        assert!(encode(Frame::Response(make_reply(580, MAX_PROPOSAL_REPLY_BYTES))).is_err());
        for (height, hash) in [(0, [2; 32]), (1, [0; 32])] {
            let invalid = Request {
                query: Query::ConsensusProposal {
                    height,
                    round: 580,
                    hash,
                },
                ..request.clone()
            };
            assert!(encode(Frame::Request(invalid)).is_err());
        }
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
