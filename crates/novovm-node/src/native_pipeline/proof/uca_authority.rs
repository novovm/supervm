//! UCA state relation shared by the proof producer and the zkVM guest.
use crate::native_pipeline::{
    business::{uca_application::state::ApplicationGrantAccessV1, uca_identity::IdentityAccessV1},
    state::{
        frontier::{CaptureBudget, DeclaredAccess, OwnedStateInput},
        tree::{NodeHash, StateNodeReader},
    },
};
use anyhow::{ensure, Context, Result};
use novovm_protocol::{
    native_query::IdentityStatusV1, uca_application::ApplicationGrantSelectorV1,
    uca_registration::UcaRegistrationIntentV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const JOURNAL_DOMAIN: &[u8] = b"NOVOVM/UCA/finalized-authority/v1\0";
pub const MAX_CONTRACT_BYTES: usize = 1024;
pub const MAX_GUEST_INPUT_BYTES: usize = 4 * 1024 * 1024;
pub const JOURNAL_BYTES: usize = 32;
const CAPTURE_BUDGET: CaptureBudget = CaptureBudget {
    keys: 35,
    nodes: 35 * 257,
    bytes: 3 * 1024 * 1024,
};

/// The relying party supplies this request under its own application policy.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum UcaAuthorityClaimV1 {
    Registration(UcaRegistrationIntentV1),
    DeviceGrant {
        selector: ApplicationGrantSelectorV1,
        revision: u64,
    },
}

impl UcaAuthorityClaimV1 {
    pub fn domain(&self) -> (u64, NodeHash) {
        match self {
            Self::Registration(intent) => (intent.chain_id, intent.genesis),
            Self::DeviceGrant { selector, .. } => (selector.chain_id, selector.genesis),
        }
    }
}

/// The verifier fills this from its independently certified, locally selected head.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UcaAuthorityStatementV1 {
    pub protocol: NodeHash,
    pub height: u64,
    pub block_hash: NodeHash,
    pub state_root: NodeHash,
    pub claim: UcaAuthorityClaimV1,
}

enum Access {
    Registration(IdentityAccessV1),
    DeviceGrant(ApplicationGrantAccessV1),
}

impl Access {
    fn new(claim: &UcaAuthorityClaimV1) -> Result<Self> {
        Ok(match claim {
            UcaAuthorityClaimV1::Registration(intent) => {
                intent.signing_bytes()?;
                Self::Registration(IdentityAccessV1::new(
                    intent.chain_id,
                    &intent.genesis,
                    intent.identity.as_str(),
                )?)
            }
            UcaAuthorityClaimV1::DeviceGrant { selector, revision } => {
                ensure!(*revision > 0, "UCA grant revision is required");
                Self::DeviceGrant(ApplicationGrantAccessV1::new(selector.clone())?)
            }
        })
    }

    fn declarations(&self) -> Vec<DeclaredAccess> {
        let declarations = match self {
            Self::Registration(access) => access.declarations(),
            Self::DeviceGrant(access) => access.declarations(),
        };
        declarations
            .iter()
            .map(|entry| DeclaredAccess {
                key: entry.key.clone(),
                may_put: false,
                may_delete: false,
            })
            .collect()
    }

    fn verify(&self, statement: &UcaAuthorityStatementV1, input: &OwnedStateInput) -> Result<()> {
        let root = statement.state_root;
        let height = statement.height;
        match (self, &statement.claim) {
            (Self::Registration(access), UcaAuthorityClaimV1::Registration(intent)) => {
                let identity = access
                    .read(input, root, height)?
                    .context("registered UCA missing")?;
                let intent_digest: NodeHash = Sha256::digest(intent.signing_bytes()?).into();
                ensure!(
                    identity.status == IdentityStatusV1::Active
                        && identity.primary_key_ref == intent.primary_key_ref
                        && height >= intent.valid_from_height
                        && access.registration_intent_digest(input)? == intent_digest,
                    "UCA registration does not match the active identity"
                );
                Ok(())
            }
            (Self::DeviceGrant(access), UcaAuthorityClaimV1::DeviceGrant { revision, .. }) => {
                access
                    .read_under_rules(input, root, height, true)?
                    .check_at_root(root, height, *revision)
            }
            _ => unreachable!("access derives from the same claim"),
        }
    }
}

impl UcaAuthorityStatementV1 {
    fn encode(&self) -> Result<Vec<u8>> {
        ensure!(
            self.height > 0
                && self.protocol != [0; 32]
                && self.block_hash != [0; 32]
                && self.state_root != [0; 32],
            "missing UCA proof chain commitment"
        );
        let bytes = postcard::to_allocvec(self)?;
        ensure!(
            bytes.len() <= MAX_CONTRACT_BYTES,
            "UCA statement byte limit"
        );
        Ok(bytes)
    }

    pub fn journal(&self) -> Result<[u8; JOURNAL_BYTES]> {
        let mut hash = Sha256::new();
        hash.update(JOURNAL_DOMAIN);
        hash.update(self.encode()?);
        Ok(hash.finalize().into())
    }

    /// The witness stays with the prover. The portable object holds the receipt only.
    pub fn capture_guest_input(&self, source: &dyn StateNodeReader) -> Result<Vec<u8>> {
        let statement = self.encode()?;
        let access = Access::new(&self.claim)?;
        let input = OwnedStateInput::capture(
            source,
            self.state_root,
            &access.declarations(),
            CAPTURE_BUDGET,
        )?;
        // Refuse impossible requests before invoking the expensive prover.
        access.verify(self, &input)?;
        let witness = input.encode_witness(CAPTURE_BUDGET)?;
        let body_len = 2 + statement.len() + witness.len();
        ensure!(
            body_len <= MAX_GUEST_INPUT_BYTES,
            "UCA guest input byte limit"
        );
        let mut framed = Vec::with_capacity(4 + body_len);
        framed.extend_from_slice(&u32::try_from(body_len)?.to_le_bytes());
        framed.extend_from_slice(&u16::try_from(statement.len())?.to_be_bytes());
        framed.extend_from_slice(&statement);
        framed.extend_from_slice(&witness);
        Ok(framed)
    }
}

/// Exact zkVM relation. It commits only after replaying authenticated reads
/// under the supplied state root, which the relying party must certify.
pub fn verify_guest_input(body: &[u8]) -> Result<[u8; JOURNAL_BYTES]> {
    ensure!(
        (2..=MAX_GUEST_INPUT_BYTES).contains(&body.len()),
        "UCA guest input byte limit"
    );
    let len = usize::from(u16::from_be_bytes(body[..2].try_into()?));
    ensure!(
        (1..=MAX_CONTRACT_BYTES).contains(&len) && 2 + len <= body.len(),
        "invalid UCA statement length"
    );
    let (encoded, witness) = body[2..].split_at(len);
    let statement: UcaAuthorityStatementV1 = postcard::from_bytes(encoded)?;
    ensure!(statement.encode()? == encoded, "noncanonical UCA statement");
    let access = Access::new(&statement.claim)?;
    let input = OwnedStateInput::from_witness(
        statement.state_root,
        &access.declarations(),
        witness,
        CAPTURE_BUDGET,
    )?;
    access.verify(&statement, &input)?;
    statement.journal()
}
