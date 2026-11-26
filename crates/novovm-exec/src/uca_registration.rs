//! Registration proof admission through the existing qualified ML-DSA owner.
use crate::{
    pq_signature::{AoemMldsaParameterSet, MldsaSigner},
    AoemExecFacade,
};
use anyhow::{ensure, Result};
use novovm_adapter_api::{uca_key_binding::derive_primary_key_ref_from_binding_v1, UcaKeyAlgo};
use novovm_protocol::uca_registration::{
    SignedUcaRegistrationV1, UcaRegistrationIntentV1, REGISTRATION_CONTEXT_V1,
};
use novovm_protocol::uca_spend::MLDSA65_PUBLIC_KEY_BYTES;
use sha2::{Digest, Sha256};

/// Proof of possession for a single chain and registration height. Name
/// availability and consensus activation still come from the state owner.
pub struct VerifiedUcaRegistrationV1 {
    signed: SignedUcaRegistrationV1,
    height: u64,
    proof_id: [u8; 32],
}

impl VerifiedUcaRegistrationV1 {
    pub fn intent(&self) -> &UcaRegistrationIntentV1 {
        &self.signed.intent
    }

    pub fn public_key(&self) -> &[u8] {
        &self.signed.public_key
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn proof_id(&self) -> [u8; 32] {
        self.proof_id
    }
}

impl AoemExecFacade {
    pub fn sign_uca_registration_v1(
        &self,
        intent: UcaRegistrationIntentV1,
        public_key: &[u8],
        secret: &[u8],
        chain_id: u64,
        genesis: [u8; 32],
        height: u64,
    ) -> Result<SignedUcaRegistrationV1> {
        let mut signed = SignedUcaRegistrationV1 {
            intent,
            public_key: public_key.to_vec(),
            signature: Vec::new(),
        };
        let message = registration_message(&signed, chain_id, genesis, height)?;
        signed.signature = MldsaSigner::new(&self.dynlib, AoemMldsaParameterSet::MlDsa65)?.sign(
            public_key,
            secret,
            &message,
            REGISTRATION_CONTEXT_V1,
        )?;
        signed.encode()?;
        self.verify_uca_registration_v1(signed.clone(), chain_id, genesis, height)?;
        Ok(signed)
    }

    pub fn verify_uca_registration_v1(
        &self,
        signed: SignedUcaRegistrationV1,
        chain_id: u64,
        genesis: [u8; 32],
        height: u64,
    ) -> Result<VerifiedUcaRegistrationV1> {
        let message = registration_message(&signed, chain_id, genesis, height)?;
        self.mldsa_verifier_v1(AoemMldsaParameterSet::MlDsa65)?
            .verify(
                &signed.public_key,
                &message,
                REGISTRATION_CONTEXT_V1,
                &signed.signature,
            )?;
        Ok(VerifiedUcaRegistrationV1 {
            proof_id: Sha256::digest(message).into(),
            signed,
            height,
        })
    }
}

fn registration_message(
    signed: &SignedUcaRegistrationV1,
    chain_id: u64,
    genesis: [u8; 32],
    height: u64,
) -> Result<Vec<u8>> {
    let intent = &signed.intent;
    ensure!(
        intent.chain_id == chain_id
            && intent.genesis == genesis
            && (intent.valid_from_height..intent.valid_until_height).contains(&height),
        "registration proof is outside the chain or height scope"
    );
    ensure!(
        signed.public_key.len() == MLDSA65_PUBLIC_KEY_BYTES,
        "invalid registration public key length"
    );
    let reference = derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &signed.public_key);
    ensure!(
        reference.as_slice() == intent.primary_key_ref,
        "registration primary key reference does not match the signing key"
    );
    // Legacy assigned names continue to live under their original binding.
    let mut name = String::from("uca:");
    for byte in reference {
        use std::fmt::Write;
        write!(&mut name, "{byte:02x}")?;
    }
    ensure!(
        intent.identity.as_str() == name,
        "registration identity is not derived from the root key"
    );
    Ok(intent.signing_bytes()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AoemExecOpenOptions;
    use novovm_protocol::native_query::UcaIdentityIdV1;
    use std::path::Path;

    #[test]
    #[ignore = "requires explicitly qualified AOEM library"]
    fn native_uca_registration_verifies_exact_proof_and_scope() -> Result<()> {
        let library =
            std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY").expect("set qualified library path");
        let owner = AoemExecFacade::open(Path::new(&library), AoemExecOpenOptions::default())?;
        let runtime = owner.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
        let key = runtime.generate_keypair()?;
        let reference =
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &key.public_key);
        let hex = reference
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let identity = UcaIdentityIdV1::new(&format!("uca:{hex}")).expect("canonical identity");
        let intent = UcaRegistrationIntentV1 {
            chain_id: 7,
            genesis: [9; 32],
            identity,
            primary_key_ref: reference.try_into().unwrap(),
            valid_from_height: 10,
            valid_until_height: 15,
        };
        let signature = runtime.sign(
            &key.public_key,
            &key.expanded_secret_key,
            &intent.signing_bytes()?,
            REGISTRATION_CONTEXT_V1,
        )?;
        let signed = SignedUcaRegistrationV1 {
            intent,
            public_key: key.public_key.to_vec(),
            signature,
        };
        let wire = signed.encode()?;
        assert_eq!(SignedUcaRegistrationV1::decode(&wire)?, signed);
        assert!(SignedUcaRegistrationV1::decode(&wire[..wire.len() - 1]).is_err());
        let verified = owner.verify_uca_registration_v1(signed.clone(), 7, [9; 32], 10)?;
        assert_eq!(verified.intent(), &signed.intent);
        assert_eq!(verified.height(), 10);
        assert_eq!(verified.public_key(), signed.public_key);
        assert!(owner
            .verify_uca_registration_v1(signed.clone(), 8, [9; 32], 10)
            .is_err());
        assert!(owner
            .verify_uca_registration_v1(signed.clone(), 7, [8; 32], 10)
            .is_err());
        assert!(owner
            .verify_uca_registration_v1(signed.clone(), 7, [9; 32], 15)
            .is_err());
        let mut different_name = signed.clone();
        different_name.intent.identity = UcaIdentityIdV1::new("uca:existing-name").unwrap();
        assert!(owner
            .verify_uca_registration_v1(different_name, 7, [9; 32], 10)
            .is_err());
        let mut tampered = signed;
        tampered.signature[0] ^= 1;
        assert!(owner
            .verify_uca_registration_v1(tampered, 7, [9; 32], 10)
            .is_err());
        Ok(())
    }
}
