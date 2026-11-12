//! Thin shared adapter between existing protected custody, qualified AOEM and
//! the original network identity interface. No listener or service is started.
use crate::purpose_keys::{self, PurposeKeyMaterialV3, PurposeV1};
use anyhow::{ensure, Result};
use novovm_exec::{
    pq_signature::{AoemMldsaParameterSet, MldsaRuntime, MldsaVerifyInput, OwnedMldsaVerifier},
    AoemExecFacade,
};
use novovm_network::product_overlay::{
    TransportIdentity, TransportSignatureProvider, TransportVerifier,
};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, sync::Arc};

struct AoemTransportSigner {
    runtime: MldsaRuntime,
    verifier: OwnedMldsaVerifier,
    key: PurposeKeyMaterialV3,
}

impl TransportSignatureProvider for AoemTransportSigner {
    fn public_key(&self) -> &[u8] {
        &self.key.public_key
    }
    fn sign(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>> {
        Ok(self.runtime.sign(
            &self.key.public_key,
            &self.key.expanded_secret_key,
            message,
            context,
        )?)
    }
    fn verify(
        &self,
        public: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        self.verifier.verify(MldsaVerifyInput {
            public_key: public,
            message,
            context,
            signature,
        })?;
        Ok(())
    }
}

pub fn from_mldsa(runtime: MldsaRuntime, key: PurposeKeyMaterialV3) -> Result<TransportIdentity> {
    ensure!(
        runtime.parameters() == AoemMldsaParameterSet::MlDsa65,
        "transport requires ML-DSA-65"
    );
    runtime.sign(
        &key.public_key,
        &key.expanded_secret_key,
        &key.public_key,
        PurposeV1::Transport.pair_context(),
    )?;
    let verifier = runtime.verification_owner_v1()?;
    TransportIdentity::new(Arc::new(AoemTransportSigner {
        runtime,
        verifier,
        key,
    }))
}

fn library_digest(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Standalone owners have no loaded runtime to borrow. Require an explicit
/// approved artifact and use the same facade/custody; never use a raw-key path.
fn load_runtime(library: &Path, sha256: &str) -> Result<MldsaRuntime> {
    ensure!(
        sha256.len() == 64
            && sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "explicit lowercase AOEM SHA-256 required"
    );
    let library = library.canonicalize()?;
    ensure!(
        library_digest(&library)? == sha256,
        "AOEM SHA-256 mismatch before load"
    );
    let facade = AoemExecFacade::open(&library, Default::default())?;
    let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    ensure!(
        library_digest(&library)? == sha256,
        "AOEM changed during identity qualification"
    );
    Ok(runtime)
}

pub fn load_transport_identity(
    key_path: &Path,
    chain_id: u64,
    library: &Path,
    sha256: &str,
) -> Result<TransportIdentity> {
    let runtime = load_runtime(library, sha256)?;
    let key = purpose_keys::load_mldsa(
        key_path,
        PurposeV1::Transport,
        chain_id,
        |public, secret, challenge| {
            runtime.sign(
                public,
                secret,
                challenge,
                PurposeV1::Transport.pair_context(),
            )?;
            Ok(())
        },
    )?;
    from_mldsa(runtime, key)
}

#[derive(Clone)]
pub struct QualifiedTransportVerifier(OwnedMldsaVerifier);
impl QualifiedTransportVerifier {
    /// Reuse an already qualified runtime without loading another library or
    /// retaining any signing key. Trust and authorization remain with callers.
    pub fn from_runtime(runtime: &MldsaRuntime) -> Result<Self> {
        ensure!(
            runtime.parameters() == AoemMldsaParameterSet::MlDsa65,
            "transport requires ML-DSA-65"
        );
        Ok(Self(runtime.verification_owner_v1()?))
    }
}
impl TransportVerifier for QualifiedTransportVerifier {
    fn verify(
        &self,
        public: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        self.0.verify(MldsaVerifyInput {
            public_key: public,
            message,
            context,
            signature,
        })?;
        Ok(())
    }
}

pub fn load_transport_verifier(library: &Path, sha256: &str) -> Result<QualifiedTransportVerifier> {
    QualifiedTransportVerifier::from_runtime(&load_runtime(library, sha256)?)
}

#[cfg(test)]
#[path = "transport_identity_fixture.rs"]
mod component_fixture;

#[cfg(test)]
mod tests {
    use super::component_fixture::component_identity;

    #[test]
    fn branded_component_identity_binds_context_and_shares_public_owner() {
        let identity = component_identity([11; 32]);
        let cloned = identity.clone();
        assert!(std::ptr::eq(
            identity.public_key().as_ptr(),
            cloned.public_key().as_ptr()
        ));
        let signature = identity.sign(b"message", b"purpose-a").unwrap();
        cloned
            .verify(identity.public_key(), b"message", b"purpose-a", &signature)
            .unwrap();
        assert!(cloned
            .verify(identity.public_key(), b"message", b"purpose-b", &signature)
            .is_err());
        assert!(cloned
            .verify(identity.public_key(), b"changed", b"purpose-a", &signature)
            .is_err());
    }
}
