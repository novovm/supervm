//! Authenticated component fixture, NOT ML-DSA or AOEM runtime qualification.
//! Real cryptographic acceptance is tested by the node/exec platform consumer.
use anyhow::Result;
use novovm_consensus::round_bft::{
    crypto::{ConsensusSigner, ConsensusVerifier},
    test_vectors::{FixtureSigningKey, FixtureVerifier},
};
use novovm_network::product_overlay::{TransportIdentity, TransportSignatureProvider};
use std::sync::Arc;

struct Fixture {
    signer: FixtureSigningKey,
    public: Vec<u8>,
}

fn transcript(message: &[u8], context: &[u8]) -> Vec<u8> {
    let mut bytes = b"NOVOVM/component-fixture-not-MLDSA\0".to_vec();
    bytes.extend_from_slice(&(context.len() as u64).to_be_bytes());
    bytes.extend_from_slice(context);
    bytes.extend_from_slice(message);
    bytes
}

impl TransportSignatureProvider for Fixture {
    fn public_key(&self) -> &[u8] {
        &self.public
    }
    fn sign(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>> {
        self.signer.sign_mldsa65(&transcript(message, context))
    }
    fn verify(
        &self,
        public: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        FixtureVerifier.verify_mldsa65(public, &transcript(message, context), signature)
    }
}

pub fn key(seed: u8) -> TransportIdentity {
    let signer = FixtureSigningKey::from_bytes(&[seed; 32]);
    let public = signer.public_key().to_vec();
    TransportIdentity::new(Arc::new(Fixture { signer, public })).expect("component fixture shape")
}
