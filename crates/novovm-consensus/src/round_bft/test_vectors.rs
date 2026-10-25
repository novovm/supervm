//! Opt-in dev fixture helpers, NOT a product signing API. Raw production sign
//! methods and safety State stay crate-private even with this feature enabled.
use super::crypto::{
    ConsensusSigner, ConsensusVerifier, MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
};
use super::wire::{Context, Hash, Phase, Proposal, ValidatorSet, Vote};
use anyhow::{ensure, Result};
use ed25519_dalek::{Signer, SigningKey as EdSigningKey};

/// Explicit non-production protocol fixture. This is NOT ML-DSA cryptography.
/// Sized fixture bytes exercise the current wire/limits; they cannot pass the
/// actual AOEM verifier and are accepted only by the branded FixtureVerifier.
#[derive(Clone)]
pub struct FixtureSigningKey {
    key: EdSigningKey,
    public_key: Vec<u8>,
}

impl FixtureSigningKey {
    pub fn from_bytes(seed: &[u8; 32]) -> Self {
        let key = EdSigningKey::from_bytes(seed);
        let mut public_key = vec![0xa5; MLDSA65_PUBLIC_KEY_BYTES];
        public_key[..32].copy_from_slice(key.verifying_key().as_bytes());
        Self { key, public_key }
    }
    pub fn verifying_key(&self) -> FixtureVerifyingKey {
        FixtureVerifyingKey(self.public_key.clone())
    }
    pub fn sign(&self, message: &[u8]) -> FixtureSignature {
        let mut signature = vec![0; MLDSA65_SIGNATURE_BYTES];
        signature[..64].copy_from_slice(&self.key.sign(message).to_bytes());
        FixtureSignature(signature)
    }
}
impl ConsensusSigner for FixtureSigningKey {
    fn public_key(&self) -> &[u8] {
        &self.public_key
    }
    fn sign_mldsa65(&self, message: &[u8]) -> Result<Vec<u8>> {
        Ok(self.sign(message).to_bytes())
    }
    fn is_test_only(&self) -> bool {
        true
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureVerifyingKey(Vec<u8>);
impl FixtureVerifyingKey {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn verify_strict(&self, message: &[u8], signature: &FixtureSignature) -> Result<()> {
        FixtureVerifier.verify_mldsa65(&self.0, message, &signature.0)
    }
}
pub struct FixtureSignature(Vec<u8>);
impl FixtureSignature {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() == MLDSA65_SIGNATURE_BYTES,
            "bad fixture signature shape"
        );
        Ok(Self(bytes.to_vec()))
    }
}
pub struct FixtureVerifier;
impl ConsensusVerifier for FixtureVerifier {
    fn verify_mldsa65(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
        ensure!(
            public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
                && public_key[32..].iter().all(|byte| *byte == 0xa5),
            "invalid fixture public key"
        );
        ensure!(
            signature.len() == MLDSA65_SIGNATURE_BYTES
                && signature[64..].iter().all(|byte| *byte == 0),
            "invalid fixture signature padding"
        );
        ed25519_dalek::VerifyingKey::from_bytes(public_key[..32].try_into()?)?.verify_strict(
            message,
            &ed25519_dalek::Signature::from_slice(&signature[..64])?,
        )?;
        Ok(())
    }
    fn is_test_only(&self) -> bool {
        true
    }
}

/// Advance a deterministic timer fixture without widening the live timer API.
pub fn round_wait_elapsed<B: super::journal::JournalBackend>(
    journal: &mut super::journal::ValidatorJournal<B>,
    expected_round: u64,
) -> Result<()> {
    journal.round_wait_elapsed(expected_round)
}

pub fn sign_proposal(
    context: Context,
    round: u64,
    value: Hash,
    valid_round: Option<u64>,
    set: &ValidatorSet,
    key: &dyn ConsensusSigner,
) -> Result<Proposal> {
    Proposal::sign(context, round, value, valid_round, set, key)
}

pub fn sign_vote(
    context: Context,
    round: u64,
    phase: Phase,
    value: Option<Hash>,
    set: &ValidatorSet,
    key: &dyn ConsensusSigner,
) -> Result<Vote> {
    Vote::sign(context, round, phase, value, set, key)
}
