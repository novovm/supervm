//! Isolated test-only custody fixture shared by original process/startup tests.
//! Never uses or changes any deployment credential. The Linux environment
//! change is serialized for the entire guard lifetime and restored on unwind.
use anyhow::Result;
use std::path::Path;

#[path = "test_credential.rs"]
mod credential;
pub(crate) use credential::TestCredentialGuard;

/// Actual AOEM ML-DSA validator and separately generated transport identity.
/// No plaintext validator format or test signature implementation is exposed.
pub(crate) struct TestNodeKeys {
    pub(crate) signer:
        std::sync::Arc<crate::native_pipeline::consensus::crypto::AoemConsensusSigner>,
    pub(crate) validator_path: std::path::PathBuf,
    pub(crate) transport: novovm_network::product_overlay::TransportIdentity,
    pub(crate) transport_path: std::path::PathBuf,
}

/// The caller keeps TestCredentialGuard alive while creating and loading
/// protected custody; original child processes receive its directory explicitly.
pub(crate) fn generate_node_keys(
    root: &Path,
    chain: u64,
    index: usize,
    crypto: &crate::native_pipeline::consensus::crypto::AoemConsensusCrypto,
) -> Result<TestNodeKeys> {
    use novovm_relay::purpose_keys;
    let validator_path = root.join(format!("test-validator-{index}.sealed"));
    purpose_keys::generate_validator(
        &validator_path,
        chain,
        || crypto.generate_test_key(),
        |public, secret, challenge| crypto.check_key_pair(public, secret, challenge),
    )?;
    let material =
        purpose_keys::load_validator(&validator_path, chain, |public, secret, challenge| {
            crypto.check_key_pair(public, secret, challenge)
        })?;
    let (transport, transport_path) =
        generate_transport_key(root, chain, &index.to_string(), crypto)?;
    Ok(TestNodeKeys {
        signer: crypto.signer(material)?,
        validator_path,
        transport,
        transport_path,
    })
}

pub(crate) fn generate_transport_key(
    root: &Path,
    chain: u64,
    label: &str,
    crypto: &crate::native_pipeline::consensus::crypto::AoemConsensusCrypto,
) -> Result<(
    novovm_network::product_overlay::TransportIdentity,
    std::path::PathBuf,
)> {
    use novovm_relay::purpose_keys::{self, PurposeV1};
    let path = root.join(format!("test-transport-{label}.sealed"));
    purpose_keys::generate_mldsa(
        &path,
        PurposeV1::Transport,
        chain,
        || crypto.generate_test_key(),
        |public, secret, challenge| crypto.check_transport_key_pair(public, secret, challenge),
    )?;
    let material = purpose_keys::read_mldsa_material(&path, PurposeV1::Transport, chain)?;
    Ok((crypto.transport_identity(material)?, path))
}
