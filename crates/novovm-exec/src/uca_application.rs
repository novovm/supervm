//! Application signature consumer of the original qualified AOEM ML-DSA owner.
//! Signature evidence never establishes current identity, grant state or finality.
use crate::{
    pq_signature::{
        AoemMldsaParameterSet, MldsaSigner, MldsaVerificationBackendV1,
        MldsaVerificationExecutionV1, MldsaVerifyInput, OwnedMldsaVerifier,
        MLDSA_VERIFY_BATCH_MAX_ITEMS,
    },
    AoemExecFacade,
};
use anyhow::{ensure, Result};
use novovm_adapter_api::{uca_key_binding::derive_primary_key_ref_from_binding_v1, UcaKeyAlgo};
use novovm_protocol::uca_application::{
    ApplicationGrantIntentV1, SignedApplicationGrantV1, APPLICATION_CONTEXT_V1,
};
use sha2::{Digest, Sha256};

/// Compact signature evidence, not current identity or application authority.
/// The private binding commits to the exact canonical intent, ML-DSA-65
/// external-pure purpose, public key and signature. Only a checked result can
/// create it; no constructor accepts a caller-supplied binding.
///
/// ```compile_fail
/// use novovm_exec::uca_application::ApplicationGrantSignatureFactV1;
/// use novovm_protocol::uca_application::ApplicationGrantIntentV1;
/// fn forge(intent: ApplicationGrantIntentV1) -> ApplicationGrantSignatureFactV1 {
///     ApplicationGrantSignatureFactV1 { intent, binding: [0; 32] }
/// }
/// ```
///
/// ```compile_fail
/// use novovm_exec::uca_application::ApplicationGrantSignatureFactV1;
/// fn require_deserialize<T: for<'de> serde::Deserialize<'de>>() {}
/// require_deserialize::<ApplicationGrantSignatureFactV1>();
/// ```
///
/// ```compile_fail
/// use novovm_exec::uca_application::ApplicationGrantSignatureFactV1;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<ApplicationGrantSignatureFactV1>();
/// ```
#[derive(Clone)]
pub struct ApplicationGrantSignatureFactV1 {
    intent: ApplicationGrantIntentV1,
    binding: [u8; 32],
}

impl ApplicationGrantSignatureFactV1 {
    pub fn intent(&self) -> &ApplicationGrantIntentV1 {
        &self.intent
    }

    /// Reattach only the original verified bytes without another signature
    /// operation. Current identity, grant state and finality remain unchecked.
    pub fn reattach(&self, signed: SignedApplicationGrantV1) -> Result<CheckedApplicationGrantV1> {
        ensure!(
            signed.intent == self.intent && signature_binding(&signed)? == self.binding,
            "application signature fact does not bind these signed bytes"
        );
        Ok(CheckedApplicationGrantV1 { signed })
    }
}

fn signature_binding(signed: &SignedApplicationGrantV1) -> Result<[u8; 32]> {
    signed.validate()?;
    let message = signed.intent.signing_bytes()?;
    let mut digest = Sha256::new();
    digest.update(b"NOVOVM/application-grant/signature-fact/v1");
    digest.update(AoemMldsaParameterSet::MlDsa65.level().to_be_bytes());
    digest.update([0, APPLICATION_CONTEXT_V1.len() as u8]);
    digest.update(APPLICATION_CONTEXT_V1);
    for bytes in [message.as_slice(), &signed.public_key, &signed.signature] {
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    Ok(digest.finalize().into())
}

/// Private verified bytes, not a proof of current application permission.
pub struct CheckedApplicationGrantV1 {
    signed: SignedApplicationGrantV1,
}
impl CheckedApplicationGrantV1 {
    /// Release owned crypto buffers while the original input owner retains
    /// the exact bytes needed for later reconstruction.
    pub fn into_signature_fact(self) -> Result<ApplicationGrantSignatureFactV1> {
        let binding = signature_binding(&self.signed)?;
        Ok(ApplicationGrantSignatureFactV1 {
            intent: self.signed.intent,
            binding,
        })
    }

    pub fn intent(&self) -> &ApplicationGrantIntentV1 {
        &self.signed.intent
    }
}

#[derive(Clone)]
pub struct ApplicationGrantVerifierV1 {
    verifier: OwnedMldsaVerifier,
}
impl ApplicationGrantVerifierV1 {
    /// Latest completed call on this shared owner, including qualification.
    /// Diagnostic only: concurrent callers cannot associate it with a grant or
    /// treat the snapshot as authority.
    pub fn last_execution_v1(&self) -> Option<MldsaVerificationExecutionV1> {
        self.verifier.last_execution_v1()
    }

    pub fn verify(&self, signed: SignedApplicationGrantV1) -> Result<CheckedApplicationGrantV1> {
        let message = prepare_verification(&signed)?;
        self.verifier.verify(MldsaVerifyInput {
            public_key: &signed.public_key,
            message: &message,
            context: APPLICATION_CONTEXT_V1,
            signature: &signed.signature,
        })?;
        Ok(CheckedApplicationGrantV1 { signed })
    }

    /// One bounded admission unit through the same qualified AOEM batch owner.
    /// Format/issuer/signature failures retain their input positions. An outer
    /// runtime failure rejects the call without scalar retries or fallback.
    /// Signature evidence alone never grants current application authority.
    pub fn verify_batch(
        &self,
        signed: Vec<SignedApplicationGrantV1>,
    ) -> Result<Vec<Result<CheckedApplicationGrantV1>>> {
        ensure!(
            signed.len() <= MLDSA_VERIFY_BATCH_MAX_ITEMS,
            "application signature batch exceeds limit"
        );
        let mut outcomes: Vec<Option<Result<CheckedApplicationGrantV1>>> =
            std::iter::repeat_with(|| None).take(signed.len()).collect();
        let mut prepared = Vec::with_capacity(signed.len());
        for (index, signed) in signed.into_iter().enumerate() {
            match prepare_verification(&signed) {
                Ok(message) => prepared.push((index, signed, message)),
                Err(error) => outcomes[index] = Some(Err(error)),
            }
        }
        let inputs: Vec<_> = prepared
            .iter()
            .map(|(_, signed, message)| MldsaVerifyInput {
                public_key: &signed.public_key,
                message,
                context: APPLICATION_CONTEXT_V1,
                signature: &signed.signature,
            })
            .collect();
        let verified = self.verifier.verify_batch(&inputs)?;
        ensure!(
            verified.len() == prepared.len(),
            "application batch result count mismatch"
        );
        for ((index, signed, _), result) in prepared.into_iter().zip(verified) {
            outcomes[index] = Some(
                result
                    .map(|()| CheckedApplicationGrantV1 { signed })
                    .map_err(Into::into),
            );
        }
        outcomes
            .into_iter()
            .map(|result| result.ok_or_else(|| anyhow::anyhow!("application batch result missing")))
            .collect()
    }
}

fn prepare_verification(signed: &SignedApplicationGrantV1) -> Result<Vec<u8>> {
    signed.validate()?;
    ensure!(
        derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &signed.public_key)
            == signed.intent.issuer_key_ref,
        "application issuer key reference mismatch"
    );
    Ok(signed.intent.signing_bytes()?)
}

pub struct ApplicationGrantSignerV1<'a> {
    signer: MldsaSigner<'a>,
}
impl ApplicationGrantSignerV1<'_> {
    pub fn sign(
        &self,
        intent: ApplicationGrantIntentV1,
        public_key: &[u8],
        secret: &[u8],
    ) -> Result<SignedApplicationGrantV1> {
        ensure!(
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, public_key)
                == intent.issuer_key_ref,
            "application issuer key reference mismatch"
        );
        let signature = self.signer.sign(
            public_key,
            secret,
            &intent.signing_bytes()?,
            APPLICATION_CONTEXT_V1,
        )?;
        let signed = SignedApplicationGrantV1 {
            intent,
            public_key: public_key.to_vec(),
            signature,
        };
        signed.validate()?;
        Ok(signed)
    }
}
impl AoemExecFacade {
    pub fn application_grant_signer_v1(&self) -> Result<ApplicationGrantSignerV1<'_>> {
        Ok(ApplicationGrantSignerV1 {
            signer: MldsaSigner::new(&self.dynlib, AoemMldsaParameterSet::MlDsa65)?,
        })
    }
    pub fn application_grant_verifier_v1(&self) -> Result<ApplicationGrantVerifierV1> {
        Ok(ApplicationGrantVerifierV1 {
            verifier: OwnedMldsaVerifier::new(
                std::sync::Arc::clone(&self.dynlib),
                AoemMldsaParameterSet::MlDsa65,
            )?,
        })
    }

    /// Explicit backend qualification on the original consumer. Does not
    /// change the default Auto policy or allow fallback after a failure.
    pub fn application_grant_verifier_with_backend_v1(
        &self,
        backend: MldsaVerificationBackendV1,
    ) -> Result<ApplicationGrantVerifierV1> {
        Ok(ApplicationGrantVerifierV1 {
            verifier: OwnedMldsaVerifier::new_with_backend(
                std::sync::Arc::clone(&self.dynlib),
                AoemMldsaParameterSet::MlDsa65,
                backend,
            )?,
        })
    }
}

#[cfg(test)]
mod tests;
