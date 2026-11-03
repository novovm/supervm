//! Actual AOEM and original protected-purpose custody for child-process tests.
//! Numeric labels identify independently generated keys; they are not seeds or
//! a component signature algorithm. No deployment identity is read or written.
use anyhow::{Context, Result};
use novovm_exec::{
    pq_signature::{AoemMldsaParameterSet, MldsaKeyPair, MldsaRuntime},
    AoemExecFacade,
};
use novovm_network::product_overlay::TransportIdentity;
use novovm_relay::purpose_keys::{self, PurposeKeyMaterialV3, PurposeV1};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

struct Key {
    material: MldsaKeyPair,
    identity: TransportIdentity,
}
struct Runtime {
    library: PathBuf,
    digest: String,
    crypto: MldsaRuntime,
    keys: Mutex<BTreeMap<u8, Arc<Key>>>,
}
fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        let library = PathBuf::from(
            std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
                .expect("actual process tests require explicit NOVOVM_AOEM_TEST_LIBRARY"),
        )
        .canonicalize()
        .expect("approved AOEM test library must exist");
        let digest = format!(
            "{:x}",
            Sha256::digest(std::fs::read(&library).expect("read AOEM artifact"))
        );
        let facade = AoemExecFacade::open(&library, Default::default())
            .expect("open actual AOEM test artifact");
        let crypto = facade
            .mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)
            .expect("qualify actual ML-DSA-65");
        Runtime {
            library,
            digest,
            crypto,
            keys: Mutex::new(BTreeMap::new()),
        }
    })
}
fn material(label: u8) -> Arc<Key> {
    let runtime = runtime();
    let mut keys = runtime.keys.lock().expect("fixture key lock");
    keys.entry(label)
        .or_insert_with(|| {
            let material = runtime
                .crypto
                .generate_keypair()
                .expect("actual ML-DSA-65 fixture key generation");
            let owned = PurposeKeyMaterialV3::new(
                material.public_key.clone(),
                material.expanded_secret_key.clone(),
            )
            .expect("actual key shape");
            let identity =
                novovm_relay::transport_identity::from_mldsa(runtime.crypto.clone(), owned)
                    .expect("qualify actual fixture identity");
            Arc::new(Key { material, identity })
        })
        .clone()
}
pub fn identity(label: u8) -> TransportIdentity {
    material(label).identity.clone()
}
pub fn library() -> PathBuf {
    runtime().library.clone()
}
pub fn digest() -> String {
    runtime().digest.clone()
}
pub fn protect_for_chain(label: u8, path: &Path, chain_id: u64) -> Result<Vec<u8>> {
    let key = material(label);
    let crypto = &runtime().crypto;
    let public = purpose_keys::generate_mldsa(
        path,
        PurposeV1::Transport,
        chain_id,
        || {
            PurposeKeyMaterialV3::new(
                key.material.public_key.clone(),
                key.material.expanded_secret_key.clone(),
            )
        },
        |public, secret, challenge| {
            crypto
                .sign(
                    public,
                    secret,
                    challenge,
                    PurposeV1::Transport.pair_context(),
                )
                .context("qualify process purpose key")?;
            Ok(())
        },
    )?;
    Ok(public.public_key)
}
