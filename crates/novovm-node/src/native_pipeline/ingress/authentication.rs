//! V3 Transfer authentication with the original signing domain and nonce identity.
//! This verifies signatures, not spendability, a current parent, fees or durable
//! admission. No environment lookup, signer registry or nonce reservation occurs.
//! V3 is Ed25519 only: no PQ or privacy capability is inferred from this type.

use super::apfl::ApflTransferBatch;
use super::wire::{decode_transfer_v3, TransferV3, TransferView};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

const NONCE_SCHEME: &[u8] = b"novovm-native-auth/ed25519-public-key/v2\0";

/// Only successful cryptographic verification constructs this immutable type.
/// It is not permission to execute/publish, nor evidence of nonce availability.
///
/// ```compile_fail
/// use novovm_node::native_pipeline::ingress::{authentication::SignatureCheckedTransfer, wire::TransferV3};
/// fn forge(transfer: TransferV3) -> SignatureCheckedTransfer {
///     SignatureCheckedTransfer {
///         transfer, tx_hash: [0; 32], public_key: [0; 32], nonce_identity: [0; 32],
///     }
/// }
/// ```
pub struct SignatureCheckedTransfer {
    transfer: CheckedSource,
    tx_hash: [u8; 32],
    public_key: [u8; 32],
    nonce_identity: [u8; 32],
}

/// Keep the immutable structured row alive through compilation and AOEM business
/// callbacks. Authentication never expands an APFL row into an owned Transfer.
enum CheckedSource {
    Wire(TransferV3),
    Apfl {
        batch: Arc<ApflTransferBatch>,
        index: usize,
    },
}

impl CheckedSource {
    fn view(&self) -> TransferView<'_> {
        match self {
            Self::Wire(tx) => tx.as_view(),
            Self::Apfl { batch, index } => batch.row(*index).expect("validated immutable APFL row"),
        }
    }
}

impl SignatureCheckedTransfer {
    pub(crate) fn is_apfl_view(&self) -> bool {
        matches!(self.transfer, CheckedSource::Apfl { .. })
    }

    pub fn transfer(&self) -> TransferView<'_> {
        self.transfer.view()
    }

    pub fn tx_hash(&self) -> [u8; 32] {
        self.tx_hash
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public_key
    }

    /// Original chain-separated signer identity digest; 20/32 byte balance
    /// aliases share this nonce bucket without aliasing their balance accounts.
    pub fn nonce_identity(&self) -> [u8; 32] {
        self.nonce_identity
    }
}

/// Strict resident admission. Canonical V3 signing bytes remain unchanged;
/// unlike the legacy backend-dependent verifier, weak keys and noncanonical
/// signatures are explicitly rejected. This is not an activation on an old chain.
pub fn authenticate_transfer_v3(
    raw: &[u8],
    configured_chain_id: u64,
    max_bytes: usize,
) -> Result<SignatureCheckedTransfer> {
    authenticate_source(
        CheckedSource::Wire(decode_transfer_v3(raw, max_bytes)?),
        configured_chain_id,
    )
}

/// Same verifier and signed intent as raw V3, with no reconstructed raw body or
/// receiver-generated signature. The codec is structural validation, NOT auth.
pub(crate) fn authenticate_apfl_row(
    batch: Arc<ApflTransferBatch>,
    index: usize,
    configured_chain_id: u64,
) -> Result<SignatureCheckedTransfer> {
    batch.row(index)?;
    authenticate_source(CheckedSource::Apfl { batch, index }, configured_chain_id)
}

fn authenticate_source(
    source: CheckedSource,
    configured_chain_id: u64,
) -> Result<SignatureCheckedTransfer> {
    ensure!(
        configured_chain_id != 0,
        "configured chain id must be nonzero"
    );
    let transfer = source.view();
    ensure!(
        transfer.chain_id == configured_chain_id,
        "signed chain domain mismatch"
    );
    ensure!(transfer.nonce != u64::MAX, "signer nonce exhausted");
    ensure!(
        matches!(transfer.from.len(), 20 | 32),
        "invalid payer account width"
    );
    ensure!(
        matches!(transfer.to.len(), 20 | 32),
        "invalid recipient account width"
    );
    ensure!(
        transfer.signature.len() == 96,
        "V3 requires public key (32) and Ed25519 signature (64)"
    );
    let public_key: [u8; 32] = transfer.signature[..32].try_into()?;
    let signature = Signature::from_slice(&transfer.signature[32..])?;
    let key = VerifyingKey::from_bytes(&public_key).context("invalid Ed25519 public key")?;
    key.verify_strict(&transfer.signing_message()?, &signature)
        .context("invalid native V3 signature")?;
    let account_matches = match transfer.from.len() {
        20 => transfer.from == &Sha256::digest(public_key)[12..32],
        32 => transfer.from == public_key,
        _ => false,
    };
    ensure!(
        account_matches,
        "signature does not authorize payer identity"
    );
    let mut digest = Sha256::new();
    digest.update(b"novovm-native-auth-nonce-identity-v1");
    digest.update(configured_chain_id.to_be_bytes());
    digest.update(NONCE_SCHEME);
    digest.update(public_key);
    Ok(SignatureCheckedTransfer {
        tx_hash: transfer.canonical_tx_hash()?,
        transfer: source,
        public_key,
        nonce_identity: digest.finalize().into(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonceTransition {
    pub identity: [u8; 32],
    pub before: u64,
    pub after: u64,
}

/// Ordered, all-or-error nonce planning with no mutation of the parent map.
/// Every signer must have an explicitly supplied parent value, including zero
/// for independently proven absence. Missing input never silently becomes zero.
/// The compiler must obtain these values from its exact captured parent root;
/// a caller-supplied map alone is not authenticated state or durable reservation.
pub fn check_nonce_sequence(
    transactions: &[SignatureCheckedTransfer],
    parent: &BTreeMap<[u8; 32], u64>,
) -> Result<Vec<NonceTransition>> {
    let mut next = BTreeMap::new();
    let mut transitions = Vec::with_capacity(transactions.len());
    for transaction in transactions {
        let identity = transaction.nonce_identity;
        let expected = next
            .entry(identity)
            .or_insert_with(|| parent.get(&identity).copied());
        let before = expected.context("nonce parent input missing; absence must be explicit")?;
        ensure!(
            before == transaction.transfer().nonce,
            "nonce replay, gap or conflicting signer alias"
        );
        let after = before.checked_add(1).context("signer nonce exhausted")?;
        *expected = Some(after);
        transitions.push(NonceTransition {
            identity,
            before,
            after,
        });
    }
    Ok(transitions)
}

#[cfg(test)]
mod tests;
