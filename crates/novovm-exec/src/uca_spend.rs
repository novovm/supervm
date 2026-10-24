//! The shared spend-signature consumer of the existing AOEM crypto capability.
//! A checked signature is NOT a trusted UCA state proof or permission to publish.
use crate::pq_signature::{AoemMldsaParameterSet, MldsaSigner, MldsaVerifier};
use crate::AoemExecFacade;
use anyhow::{ensure, Result};
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, SignedUcaSpendV1, UcaSpendIntentV1, SPEND_CONTEXT_V1,
};

/// Cannot be deserialized or constructed by a caller. Does not certify the
/// key's current authority, sufficient funds, nonce eligibility or finality.
pub struct SignatureCheckedUcaSpendV1 {
    signed: SignedUcaSpendV1,
}

impl SignatureCheckedUcaSpendV1 {
    pub fn intent(&self) -> &UcaSpendIntentV1 {
        &self.signed.intent
    }
    pub fn signed(&self) -> &SignedUcaSpendV1 {
        &self.signed
    }

    /// Combines the checked signature with pure policy preflight. Supplied state
    /// is still NOT authenticated by this API; this returns no spend capability.
    /// A native owner must establish its trusted parent and atomically apply it.
    pub fn check_supplied_state(
        &self,
        state: Option<&novovm_adapter_api::uca_spend_policy::SuppliedSpendStateV1<'_>>,
        expected_parent: &[u8; 32],
        execution_height: u64,
    ) -> std::result::Result<(), novovm_adapter_api::uca_spend_policy::SpendPolicyError> {
        novovm_adapter_api::uca_spend_policy::check_supplied_spend_state_v1(
            &self.signed.intent,
            state,
            expected_parent,
            execution_height,
        )
    }
}

/// Reuse one qualified verifier for a batch/session; don't re-run compatibility
/// vectors for every transaction. Its lifetime is bounded by the loaded facade.
pub struct UcaSpendVerifierV1<'a> {
    verifier: MldsaVerifier<'a>,
}

impl UcaSpendVerifierV1<'_> {
    pub fn verify(&self, signed: SignedUcaSpendV1) -> Result<SignatureCheckedUcaSpendV1> {
        signed.validate()?;
        self.verifier.verify(
            &signed.public_key,
            &signed.intent.signing_bytes()?,
            SPEND_CONTEXT_V1,
            &signed.signature,
        )?;
        Ok(SignatureCheckedUcaSpendV1 { signed })
    }
}

pub struct UcaSpendSignerV1<'a> {
    signer: MldsaSigner<'a>,
}

impl UcaSpendSignerV1<'_> {
    /// Key custody and user consent remain with the existing purpose-key owner.
    /// No secret key copy or persistence, and no inference of asset permission.
    pub fn sign(
        &self,
        intent: UcaSpendIntentV1,
        public_key: &[u8],
        expanded_secret_key: &[u8],
    ) -> Result<SignedUcaSpendV1> {
        ensure!(
            spend_key_ref_v1(public_key)? == intent.spend_key_ref,
            "spend key binding mismatch"
        );
        let signature = self.signer.sign(
            public_key,
            expanded_secret_key,
            &intent.signing_bytes()?,
            SPEND_CONTEXT_V1,
        )?;
        let signed = SignedUcaSpendV1 {
            intent,
            public_key: public_key.to_vec(),
            signature,
        };
        signed.validate()?;
        Ok(signed)
    }
}

impl AoemExecFacade {
    /// Requires the existing positive/tampered known-answer checks and explicit
    /// 65 sizes. No negotiation or fallback to another suite or algorithm.
    pub fn uca_spend_verifier_v1(&self) -> Result<UcaSpendVerifierV1<'_>> {
        Ok(UcaSpendVerifierV1 {
            verifier: MldsaVerifier::new(&self.dynlib, AoemMldsaParameterSet::MlDsa65)?,
        })
    }

    pub fn uca_spend_signer_v1(&self) -> Result<UcaSpendSignerV1<'_>> {
        Ok(UcaSpendSignerV1 {
            signer: MldsaSigner::new(&self.dynlib, AoemMldsaParameterSet::MlDsa65)?,
        })
    }
}
