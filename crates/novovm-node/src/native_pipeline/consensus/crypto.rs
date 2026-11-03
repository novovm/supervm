//! Thin original-node adapter to the existing qualified AOEM ML-DSA facade.
//! No cryptographic implementation, alternate signer journal or state owner.
use anyhow::{ensure, Result};
use novovm_consensus::round_bft::crypto::{ConsensusSigner, ConsensusVerifier, MLDSA65_CONTEXT};
use novovm_exec::{
    pq_signature::{AoemMldsaParameterSet, MldsaRuntime},
    AoemExecFacade,
};
use novovm_network::product_overlay::TransportIdentity;
use novovm_relay::purpose_keys::{
    PurposeKeyMaterialV3, PurposeV1, ValidatorKeyMaterialV2, VALIDATOR_PAIR_CONTEXT,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AoemConsensusCrypto {
    runtime: MldsaRuntime,
}

impl AoemConsensusCrypto {
    /// Runtime qualification is performed once, not on each vote or QC member.
    pub fn new(runtime: &AoemExecFacade) -> Result<Self> {
        Ok(Self {
            runtime: runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?,
        })
    }

    pub fn check_key_pair(&self, public: &[u8], secret: &[u8], challenge: &[u8]) -> Result<()> {
        self.runtime
            .sign(public, secret, challenge, VALIDATOR_PAIR_CONTEXT)?;
        Ok(())
    }

    pub fn signer(&self, key: ValidatorKeyMaterialV2) -> Result<Arc<AoemConsensusSigner>> {
        // Revalidate ownership on handoff, including callers other than custody.
        self.check_key_pair(&key.public_key, &key.expanded_secret_key, &key.public_key)?;
        Ok(Arc::new(AoemConsensusSigner {
            crypto: self.clone(),
            key,
        }))
    }

    pub fn check_transport_key_pair(
        &self,
        public: &[u8],
        secret: &[u8],
        challenge: &[u8],
    ) -> Result<()> {
        self.runtime.sign(
            public,
            secret,
            challenge,
            PurposeV1::Transport.pair_context(),
        )?;
        Ok(())
    }

    /// A transport identity shares this qualified runtime, but owns a separate
    /// zeroizing key. Reconnect clones its handle, not private key bytes.
    pub fn transport_identity(&self, key: PurposeKeyMaterialV3) -> Result<TransportIdentity> {
        novovm_relay::transport_identity::from_mldsa(self.runtime.clone(), key)
    }

    #[cfg(test)]
    pub(crate) fn generate_test_key(&self) -> Result<ValidatorKeyMaterialV2> {
        let pair = self.runtime.generate_keypair()?;
        ValidatorKeyMaterialV2::new(pair.public_key, pair.expanded_secret_key)
    }
}

impl ConsensusVerifier for AoemConsensusCrypto {
    fn verify_mldsa65(&self, public: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
        self.runtime
            .verify(public, message, MLDSA65_CONTEXT, signature)?;
        Ok(())
    }
}

/// The original journal alone grants permission to sign a consensus message.
/// This object owns only the zeroizing purpose key and existing AOEM capability.
pub struct AoemConsensusSigner {
    crypto: AoemConsensusCrypto,
    key: ValidatorKeyMaterialV2,
}

impl ConsensusSigner for AoemConsensusSigner {
    fn public_key(&self) -> &[u8] {
        &self.key.public_key
    }

    fn sign_mldsa65(&self, message: &[u8]) -> Result<Vec<u8>> {
        ensure!(!message.is_empty(), "empty consensus signing transcript");
        Ok(self.crypto.runtime.sign(
            &self.key.public_key,
            &self.key.expanded_secret_key,
            message,
            MLDSA65_CONTEXT,
        )?)
    }
}

/// Sized component fixture only, not ML-DSA cryptography or a production
/// provider. Reuses the existing branded consensus fixture; real AOEM endpoint
/// and process tests qualify the transport signature implementation separately.
#[cfg(test)]
pub(crate) fn component_transport_identity(seed: [u8; 32]) -> TransportIdentity {
    use novovm_consensus::round_bft::{
        crypto::ConsensusVerifier,
        test_vectors::{FixtureSigningKey, FixtureVerifier},
    };
    use novovm_network::product_overlay::TransportSignatureProvider;
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
        .expect("component fixture public key has the canonical current width")
}
