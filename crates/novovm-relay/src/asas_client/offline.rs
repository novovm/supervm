//! Explicit offline signatures over reviewed native intents; never submits or opens a network.
use super::ClientConfigV1;
use crate::purpose_keys::{self, PurposeV1};
use anyhow::{ensure, Context, Result};
use novovm_adapter_api::{uca_key_binding::derive_primary_key_ref_from_binding_v1, UcaKeyAlgo};
use novovm_protocol::{
    uca_application::ApplicationGrantIntentV1, uca_registration::UcaRegistrationIntentV1,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Clone, Copy)]
pub enum SigningKind {
    Registration,
    Application,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Review<I> {
    version: u8,
    execution_height: u64,
    intent: I,
}

#[derive(Serialize)]
pub struct SigningReceipt {
    version: u8,
    wire: &'static str,
    bytes: usize,
    sha256: String,
    submitted: bool,
}

fn explicit_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute()
            && !path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir)),
        "explicit non-traversing signing path required"
    );
    Ok(())
}

fn reviewed_bytes(path: &Path, expected: &str) -> Result<Vec<u8>> {
    explicit_path(path)?;
    ensure!(
        expected.len() == 64
            && expected
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "explicit lowercase request digest required"
    );
    #[cfg(target_os = "linux")]
    let bytes = crate::linux_owner_files::read_bounded(path, 8192)?;
    #[cfg(windows)]
    let bytes = crate::windows_owner_files::read_bounded(path, 8192)?;
    #[cfg(not(any(target_os = "linux", windows)))]
    let bytes: Vec<u8> = anyhow::bail!("protected signing unavailable on this platform");
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == expected,
        "reviewed signing request changed"
    );
    Ok(bytes)
}

fn review<I: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<Review<I>> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let review: Review<I> = serde_json::from_slice(bytes)?;
    ensure!(
        serde_json::to_value(&review)? == value,
        "unsupported signing request fields"
    );
    ensure!(
        review.version == 1 && review.execution_height > 0,
        "explicit reviewed execution height required"
    );
    Ok(review)
}

fn output_available(output: &Path) -> Result<()> {
    explicit_path(output)?;
    #[cfg(target_os = "linux")]
    {
        crate::linux_owner_files::check_path(output)?;
        crate::linux_owner_files::check_private_root(output.parent().context("output parent")?)?;
    }
    #[cfg(windows)]
    {
        crate::windows_owner_files::check_path(output)?;
        ensure!(
            output.parent().context("output parent")?.is_dir(),
            "existing private output directory required"
        );
    }
    match std::fs::symlink_metadata(output) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => anyhow::bail!("signing output exists; refusing replacement"),
    }
}

/// Profile and request bytes are independently pinned by the CLI. A reviewed
/// height is not a live activation certificate; no chain authority is inferred.
pub fn sign_to_file(
    config: &ClientConfigV1,
    kind: SigningKind,
    request: &Path,
    request_sha256: &str,
    identity_key: &Path,
    output: &Path,
) -> Result<SigningReceipt> {
    config.validate()?;
    config.check_custody_environment()?;
    explicit_path(identity_key)?;
    output_available(output)?;
    let bytes = reviewed_bytes(request, request_sha256)?;
    let selection = config
        .uca
        .as_ref()
        .context("pinned UCA selection required")?;
    let (owner, runtime) =
        crate::transport_identity::load_qualified_runtime(&config.library, &config.library_sha256)?;
    let key = purpose_keys::load_mldsa(
        identity_key,
        PurposeV1::Identity,
        config.chain_id,
        |public, secret, challenge| {
            runtime.sign(
                public,
                secret,
                challenge,
                PurposeV1::Identity.pair_context(),
            )?;
            Ok(())
        },
    )?;
    let reference: [u8; 32] =
        derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &key.public_key)
            .try_into()
            .map_err(|_| anyhow::anyhow!("identity reference width"))?;
    let (wire, encoded) = match kind {
        SigningKind::Registration => {
            let reviewed: Review<UcaRegistrationIntentV1> = review(&bytes)?;
            let intent = reviewed.intent;
            ensure!(
                intent.chain_id == config.chain_id
                    && intent.genesis == selection.genesis
                    && intent.identity == selection.identity
                    && intent.primary_key_ref == reference,
                "registration intent differs from pinned UCA or root"
            );
            ensure!(
                (intent.valid_from_height..intent.valid_until_height)
                    .contains(&reviewed.execution_height),
                "registration execution height outside reviewed window"
            );
            let signed = owner.sign_uca_registration_v1(
                intent,
                &key.public_key,
                &key.expanded_secret_key,
                config.chain_id,
                selection.genesis,
                reviewed.execution_height,
            )?;
            ("NUIR", signed.encode()?)
        }
        SigningKind::Application => {
            let reviewed: Review<ApplicationGrantIntentV1> = review(&bytes)?;
            let intent = reviewed.intent;
            ensure!(
                intent.selector == config.uca_selector()?
                    && intent.issuer_key_ref == reference
                    && reference != intent.selector.device,
                "application intent differs from pinned selector or root"
            );
            ensure!(
                (intent.valid_from_height..intent.valid_until_height)
                    .contains(&reviewed.execution_height),
                "application execution height outside reviewed window"
            );
            intent.validate()?;
            let signed = owner.application_grant_signer_v1()?.sign(
                intent,
                &key.public_key,
                &key.expanded_secret_key,
            )?;
            owner
                .application_grant_verifier_v1()?
                .verify(signed.clone())?;
            ("NUAP", signed.encode()?)
        }
    };
    #[cfg(target_os = "linux")]
    crate::linux_owner_files::create(output, &encoded)?;
    #[cfg(windows)]
    crate::windows_owner_files::create(output, &encoded)?;
    Ok(SigningReceipt {
        version: 1,
        wire,
        bytes: encoded.len(),
        sha256: format!("{:x}", Sha256::digest(&encoded)),
        submitted: false,
    })
}

#[cfg(test)]
mod tests;
