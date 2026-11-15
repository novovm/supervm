//! Application signature consumer of the original qualified AOEM ML-DSA owner.
//! Signature evidence never establishes current identity, grant state or finality.
use crate::{
    pq_signature::{AoemMldsaParameterSet, MldsaSigner, MldsaVerifyInput, OwnedMldsaVerifier},
    AoemExecFacade,
};
use anyhow::{ensure, Result};
use novovm_adapter_api::{uca_key_binding::derive_primary_key_ref_from_binding_v1, UcaKeyAlgo};
use novovm_protocol::uca_application::{
    ApplicationGrantIntentV1, SignedApplicationGrantV1, APPLICATION_CONTEXT_V1,
};

pub struct CheckedApplicationGrantV1 {
    signed: SignedApplicationGrantV1,
}
impl CheckedApplicationGrantV1 {
    pub fn intent(&self) -> &ApplicationGrantIntentV1 {
        &self.signed.intent
    }
}

#[derive(Clone)]
pub struct ApplicationGrantVerifierV1 {
    verifier: OwnedMldsaVerifier,
}
impl ApplicationGrantVerifierV1 {
    pub fn verify(&self, signed: SignedApplicationGrantV1) -> Result<CheckedApplicationGrantV1> {
        signed.validate()?;
        ensure!(
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &signed.public_key)
                == signed.intent.issuer_key_ref,
            "application issuer key reference mismatch"
        );
        self.verifier.verify(MldsaVerifyInput {
            public_key: &signed.public_key,
            message: &signed.intent.signing_bytes()?,
            context: APPLICATION_CONTEXT_V1,
            signature: &signed.signature,
        })?;
        Ok(CheckedApplicationGrantV1 { signed })
    }
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
}
