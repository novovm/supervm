//! Component-test signature fixture, NOT an ML-DSA implementation or runtime
//! qualification. Real AOEM handshake tests are owned by the node/exec consumer.
//! Keeping signatures authenticated here lets codec/lifecycle tests distinguish
//! tampering without loading a platform DLL in every network unit test.

use crate::product_overlay::{TransportIdentity, TransportSignatureProvider};
use anyhow::Result;
use novovm_consensus::round_bft::{
    crypto::{ConsensusSigner, ConsensusVerifier},
    test_vectors::{FixtureSigningKey, FixtureVerifier},
};
use std::sync::Arc;

pub(crate) type SigningKey = TransportIdentity;

struct Fixture {
    key: FixtureSigningKey,
    public: Vec<u8>,
}

fn message_bytes(message: &[u8], context: &[u8]) -> Vec<u8> {
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
        self.key.sign_mldsa65(&message_bytes(message, context))
    }
    fn verify(
        &self,
        public: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        FixtureVerifier.verify_mldsa65(public, &message_bytes(message, context), signature)
    }
}

pub(crate) fn key(seed: u8) -> TransportIdentity {
    SigningKey::from_bytes(&[seed; 32])
}

impl TransportIdentity {
    pub(crate) fn from_bytes(seed: &[u8; 32]) -> Self {
        let key = FixtureSigningKey::from_bytes(seed);
        let public = key.public_key().to_vec();
        Self::new(Arc::new(Fixture { key, public })).expect("component fixture shape")
    }
}
