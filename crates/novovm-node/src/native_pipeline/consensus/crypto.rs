//! Thin original-node adapter to the existing qualified AOEM ML-DSA facade.
//! No cryptographic implementation, alternate signer journal or state owner.
use anyhow::{ensure, Result};
use novovm_consensus::round_bft::crypto::{ConsensusSigner, ConsensusVerifier, MLDSA65_CONTEXT};
use novovm_exec::{
    pq_signature::{AoemMldsaParameterSet, MldsaRuntime},
    AoemExecFacade,
};
use novovm_relay::purpose_keys::{ValidatorKeyMaterialV2, VALIDATOR_PAIR_CONTEXT};
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
