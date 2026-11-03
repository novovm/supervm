use anyhow::Result;
use novovm_network::product_overlay::{TransportIdentity, TransportSignatureProvider};
use std::sync::Arc;

/// Component lifecycle fixture, not ML-DSA qualification. The existing
/// consensus fixture owns its test signature encoding and verification.
pub(crate) fn component_identity(seed: [u8; 32]) -> TransportIdentity {
    use novovm_consensus::round_bft::{
        crypto::ConsensusVerifier,
        test_vectors::{FixtureSigningKey, FixtureVerifier},
    };
    struct FixtureProvider {
        key: FixtureSigningKey,
        public: Vec<u8>,
    }
    fn framed(message: &[u8], context: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + context.len() + message.len());
        bytes.extend_from_slice(&(context.len() as u64).to_be_bytes());
        bytes.extend_from_slice(context);
        bytes.extend_from_slice(message);
        bytes
    }
    impl TransportSignatureProvider for FixtureProvider {
        fn public_key(&self) -> &[u8] {
            &self.public
        }
        fn sign(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>> {
            Ok(self.key.sign(&framed(message, context)).to_bytes())
        }
        fn verify(
            &self,
            public: &[u8],
            message: &[u8],
            context: &[u8],
            signature: &[u8],
        ) -> Result<()> {
            FixtureVerifier.verify_mldsa65(public, &framed(message, context), signature)
        }
    }
    let key = FixtureSigningKey::from_bytes(&seed);
    let public = key.verifying_key().to_bytes();
    TransportIdentity::new(Arc::new(FixtureProvider { key, public }))
        .expect("current-width component fixture")
}
