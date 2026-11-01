//! The shared spend-signature consumer of the existing AOEM crypto capability.
//! A checked signature is NOT a trusted UCA state proof or permission to publish.
use crate::pq_signature::{
    AoemMldsaParameterSet, MldsaSigner, MldsaVerificationBackendV1, MldsaVerificationExecutionV1,
    MldsaVerifyInput, OwnedMldsaVerifier, MLDSA_VERIFY_BATCH_MAX_ITEMS,
};
use crate::AoemExecFacade;
use anyhow::{ensure, Result};
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV3, SignedAssetSpendV3, SPEND_CONTEXT_V3,
};
use sha2::{Digest, Sha256};

/// Compact signature evidence, not account authority. Its binding commits to
/// the exact canonical intent, ML-DSA-65 pure-purpose framing, key and signature.
/// Only a successful original verifier result can create it; no wire decoder or
/// constructor accepts a caller-supplied digest.
///
/// ```compile_fail
/// use novovm_exec::uca_spend::AssetSpendSignatureFactV3;
/// use novovm_protocol::uca_spend::AssetSpendIntentV3;
/// fn forge(intent: AssetSpendIntentV3) -> AssetSpendSignatureFactV3 {
///     AssetSpendSignatureFactV3 { intent, binding: [0; 32] }
/// }
/// ```
#[derive(Clone)]
pub struct AssetSpendSignatureFactV3 {
    intent: AssetSpendIntentV3,
    binding: [u8; 32],
}

impl AssetSpendSignatureFactV3 {
    pub fn intent(&self) -> &AssetSpendIntentV3 {
        &self.intent
    }

    /// Reattach unchanged bytes to this verified fact without repeating the
    /// cryptographic signature operation. Pipeline/source ownership and current
    /// account authorization remain the original consumer's obligations.
    pub fn reattach(&self, signed: SignedAssetSpendV3) -> Result<SignatureCheckedAssetSpendV3> {
        ensure!(
            signed.intent == self.intent && signature_binding(&signed)? == self.binding,
            "asset signature fact does not bind these signed bytes"
        );
        Ok(SignatureCheckedAssetSpendV3 { signed })
    }
}

fn signature_binding(signed: &SignedAssetSpendV3) -> Result<[u8; 32]> {
    let message = signed.intent.signing_bytes()?;
    let mut digest = Sha256::new();
    digest.update(b"NOVOVM/asset-spend/signature-fact/v3");
    digest.update(AoemMldsaParameterSet::MlDsa65.level().to_be_bytes());
    digest.update([0, SPEND_CONTEXT_V3.len() as u8]);
    digest.update(SPEND_CONTEXT_V3);
    for bytes in [message.as_slice(), &signed.public_key, &signed.signature] {
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    Ok(digest.finalize().into())
}

/// Cannot be deserialized or constructed by a caller. Does not certify the
/// key's current authority, sufficient funds, nonce eligibility or finality.
#[derive(Clone)]
pub struct SignatureCheckedAssetSpendV3 {
    signed: SignedAssetSpendV3,
}

impl SignatureCheckedAssetSpendV3 {
    /// Release owned key/signature buffers when the immutable input owner will
    /// retain their original bytes for later candidate reconstruction.
    pub fn into_signature_fact(self) -> Result<AssetSpendSignatureFactV3> {
        let binding = signature_binding(&self.signed)?;
        Ok(AssetSpendSignatureFactV3 {
            intent: self.signed.intent,
            binding,
        })
    }

    pub fn intent(&self) -> &AssetSpendIntentV3 {
        &self.signed.intent
    }
    pub fn signed(&self) -> &SignedAssetSpendV3 {
        &self.signed
    }

    /// Combines the checked signature with pure policy preflight. Supplied state
    /// is still NOT authenticated by this API; this returns no spend capability.
    /// A native owner must establish its trusted parent and atomically apply it.
    pub fn check_supplied_state(
        &self,
        state: Option<&novovm_adapter_api::uca_spend_policy::SuppliedAssetSpendStateV2<'_>>,
        expected_parent: &[u8; 32],
        execution_height: u64,
    ) -> std::result::Result<(), novovm_adapter_api::uca_spend_policy::SpendPolicyError> {
        novovm_adapter_api::uca_spend_policy::check_supplied_spend_state_v2(
            &self.signed.intent,
            state,
            expected_parent,
            execution_height,
        )
    }
}

/// Reuse this qualified verification-only owner across admission batches. Clones
/// retain the same already-loaded AOEM and do not re-run compatibility vectors.
#[derive(Clone)]
pub struct AssetSpendVerifierV3 {
    verifier: OwnedMldsaVerifier,
}

impl AssetSpendVerifierV3 {
    /// Latest completed semantic call on this shared owner, including startup
    /// qualification. Diagnostic only: concurrent callers must not associate
    /// this snapshot with a particular transaction or use it as authority.
    pub fn last_execution_v1(&self) -> Option<MldsaVerificationExecutionV1> {
        self.verifier.last_execution_v1()
    }

    pub fn verify(&self, signed: SignedAssetSpendV3) -> Result<SignatureCheckedAssetSpendV3> {
        signed.validate()?;
        self.verifier.verify(MldsaVerifyInput {
            public_key: &signed.public_key,
            message: &signed.intent.signing_bytes()?,
            context: SPEND_CONTEXT_V3,
            signature: &signed.signature,
        })?;
        Ok(SignatureCheckedAssetSpendV3 { signed })
    }

    /// Verifies one bounded admission unit through the qualified AOEM batch
    /// semantic entry. Per-row failures preserve input order; an outer failure invalidates
    /// the whole call, without scalar retries. A checked signature remains only
    /// cryptographic evidence, never a trusted parent/nonce/budget permission.
    pub fn verify_batch(
        &self,
        signed: Vec<SignedAssetSpendV3>,
    ) -> Result<Vec<Result<SignatureCheckedAssetSpendV3>>> {
        ensure!(
            signed.len() <= MLDSA_VERIFY_BATCH_MAX_ITEMS,
            "UCA signature batch exceeds limit"
        );
        let mut outcomes: Vec<Option<Result<SignatureCheckedAssetSpendV3>>> =
            std::iter::repeat_with(|| None).take(signed.len()).collect();
        let mut prepared = Vec::with_capacity(signed.len());
        for (index, signed) in signed.into_iter().enumerate() {
            match signed
                .validate()
                .and_then(|()| signed.intent.signing_bytes())
            {
                Ok(message) => prepared.push((index, signed, message)),
                Err(error) => outcomes[index] = Some(Err(error.into())),
            }
        }
        let inputs: Vec<_> = prepared
            .iter()
            .map(|(_, signed, message)| MldsaVerifyInput {
                public_key: &signed.public_key,
                message,
                context: SPEND_CONTEXT_V3,
                signature: &signed.signature,
            })
            .collect();
        let verified = self.verifier.verify_batch(&inputs)?;
        ensure!(
            verified.len() == prepared.len(),
            "UCA batch result count mismatch"
        );
        for ((index, signed, _), result) in prepared.into_iter().zip(verified) {
            outcomes[index] = Some(
                result
                    .map(|()| SignatureCheckedAssetSpendV3 { signed })
                    .map_err(Into::into),
            );
        }
        outcomes
            .into_iter()
            .map(|result| result.ok_or_else(|| anyhow::anyhow!("UCA batch result missing")))
            .collect()
    }
}

pub struct AssetSpendSignerV3<'a> {
    signer: MldsaSigner<'a>,
}

impl AssetSpendSignerV3<'_> {
    /// Key custody and user consent remain with the existing purpose-key owner.
    /// No secret key copy or persistence, and no inference of asset permission.
    pub fn sign(
        &self,
        intent: AssetSpendIntentV3,
        public_key: &[u8],
        expanded_secret_key: &[u8],
    ) -> Result<SignedAssetSpendV3> {
        ensure!(
            spend_key_ref_v1(public_key)? == intent.spend_key_ref,
            "spend key binding mismatch"
        );
        let signature = self.signer.sign(
            public_key,
            expanded_secret_key,
            &intent.signing_bytes()?,
            SPEND_CONTEXT_V3,
        )?;
        let signed = SignedAssetSpendV3 {
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
    pub fn asset_spend_verifier_v3(&self) -> Result<AssetSpendVerifierV3> {
        let verifier = OwnedMldsaVerifier::new(
            std::sync::Arc::clone(&self.dynlib),
            AoemMldsaParameterSet::MlDsa65,
        )?;
        Ok(AssetSpendVerifierV3 { verifier })
    }

    /// Explicit backend qualification for isolated comparisons using the same
    /// original consumer. No fallback or change to the default Auto policy.
    pub fn asset_spend_verifier_with_backend_v3(
        &self,
        backend: MldsaVerificationBackendV1,
    ) -> Result<AssetSpendVerifierV3> {
        let verifier = OwnedMldsaVerifier::new_with_backend(
            std::sync::Arc::clone(&self.dynlib),
            AoemMldsaParameterSet::MlDsa65,
            backend,
        )?;
        Ok(AssetSpendVerifierV3 { verifier })
    }

    pub fn asset_spend_signer_v3(&self) -> Result<AssetSpendSignerV3<'_>> {
        Ok(AssetSpendSignerV3 {
            signer: MldsaSigner::new(&self.dynlib, AoemMldsaParameterSet::MlDsa65)?,
        })
    }
}

#[cfg(test)]
mod tests;
