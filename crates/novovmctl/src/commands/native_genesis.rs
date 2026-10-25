//! Thin offline adapter to the original node compiler. No private keys, store,
//! journal, service manager, network socket or candidate publication is opened.
use super::native_nonce_migration::read_bounded_document_v1;
use crate::cli::native_genesis::{NativeGenesisArgs, NativeGenesisCommand, VerifyArgs};
use crate::{error::CtlError, output};
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_node::native_pipeline::service::config::uca_genesis::{
    UcaGenesisApprovalV1, UcaGenesisManifestV1, UcaInitializationConfigV1,
    INITIALIZATION_CONTEXT_V1,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

const COMMAND: &str = "native-genesis";
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

#[derive(Serialize)]
struct Review {
    #[serde(flatten)]
    metadata: Value,
    // Monetary values remain typed u128 all the way to serialization.
    review_only_startup_fragment: Option<StartupFragment>,
}

#[derive(Serialize)]
struct StartupFragment {
    genesis: novovm_node::native_pipeline::service::config::GenesisConfig,
    uca_initialization: UcaInitializationConfigV1,
}

pub fn run(args: NativeGenesisArgs) -> Result<(), CtlError> {
    let result =
        inspect(&args.command).and_then(|report| output::print_success_json(COMMAND, &report));
    if let Err(error) = &result {
        output::print_error_json(COMMAND, error);
    }
    result
}

fn document<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, CtlError> {
    let bytes = read_bounded_document_v1(path, MAX_DOCUMENT_BYTES, label)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| CtlError::InvalidArgument(format!("{label}: {error}")))
}

fn inspect(command: &NativeGenesisCommand) -> Result<Review, CtlError> {
    let path = match command {
        NativeGenesisCommand::Prepare(args) => &args.manifest,
        NativeGenesisCommand::VerifyApprovals(args) => &args.input.manifest,
    };
    let bytes = read_bounded_document_v1(path, MAX_DOCUMENT_BYTES, "UCA genesis manifest")?;
    let manifest: UcaGenesisManifestV1 = serde_json::from_slice(&bytes)
        .map_err(|e| CtlError::InvalidArgument(format!("UCA genesis manifest: {e}")))?;
    reject_ignored_grant_fields(&bytes, &manifest)?;
    let canonical = manifest.canonical_bytes().map_err(|error| {
        CtlError::InvalidArgument(format!("original manifest validation: {error:#}"))
    })?;
    let domain = manifest.signing_domain().map_err(|error| {
        CtlError::InvalidArgument(format!("original signing domain: {error:#}"))
    })?;
    let mut report = Review {
        metadata: json!({
            "schema_version": 1,
            "action": "prepare",
            "source": "novovm-node/UcaGenesisManifestV1",
            "signing_algorithm": "ML-DSA-65",
            "signing_api": "AoemExecFacade::mldsa_signer_v1(MlDsa65).sign(public_key, expanded_secret_key, message, context)",
            "message_is_unframed": true,
            "signing_context_hex": hex(INITIALIZATION_CONTEXT_V1),
            "signing_message_hex": hex(&canonical),
            "signing_message_bytes": canonical.len(),
            "observed_signing_domain": hex(&domain),
            "required_management_approvals": manifest.accounts.iter().map(|a| &a.uca_id).collect::<Vec<_>>(),
            "management_approvals_verified": false,
            "expected_domain_verified": false,
            "operator_launch_authorized": false,
            "existing_uca_authority_verified": false,
            "database_created": false,
            "activation_ready": false,
            "state_published": false,
            "consensus_signature_algorithm": "Ed25519",
            "whole_node_post_quantum_qualified": false
        }),
        review_only_startup_fragment: None,
    };
    if let NativeGenesisCommand::VerifyApprovals(args) = command {
        verify(args, &manifest, domain, &mut report)?;
    }
    Ok(report)
}

fn verify(
    args: &VerifyArgs,
    manifest: &UcaGenesisManifestV1,
    domain: [u8; 32],
    report: &mut Review,
) -> Result<(), CtlError> {
    if parse_digest(&args.expected_domain)? != domain {
        return Err(CtlError::InvalidArgument(
            "manifest differs from independently reviewed signing domain; no auto-repin".into(),
        ));
    }
    let expected_library = parse_digest(&args.library_sha256)?;
    let approvals: Vec<UcaGenesisApprovalV1> = document(&args.approvals, "management approvals")?;
    let library = args
        .library
        .canonicalize()
        .map_err(|error| CtlError::FileReadFailed(format!("explicit AOEM library: {error}")))?;
    if file_digest(&library)? != expected_library {
        return Err(CtlError::InvalidArgument(
            "AOEM library SHA-256 mismatch before load".into(),
        ));
    }
    let runtime = AoemExecFacade::open(&library, AoemExecOpenOptions::default())
        .map_err(|error| CtlError::IntegrationFailed(format!("original AOEM loader: {error:#}")))?;
    // Original compiler verifies all management signatures, purpose bindings,
    // policy, initial effects and root. No second encoding/tree implementation.
    let verified = manifest
        .compile(&runtime, domain, &approvals)
        .map_err(|error| {
            CtlError::IntegrationFailed(format!("original genesis compiler: {error:#}"))
        })?;
    if file_digest(&library)? != expected_library {
        return Err(CtlError::IntegrationFailed(
            "AOEM library changed during review".into(),
        ));
    }
    let material = UcaInitializationConfigV1 {
        accounts: manifest.accounts.clone(),
        approvals,
        expected_signing_domain: verified.signing_domain(),
        expected_initial_root: verified.state().root(),
        expected_final_commitment: verified.final_commitment(),
    };
    let mut genesis = manifest.genesis.clone();
    genesis.genesis_config_commitment = domain;
    material.validate_manifest(&genesis).map_err(|error| {
        CtlError::IntegrationFailed(format!("original startup fragment validation: {error:#}"))
    })?;
    report.metadata["action"] = json!("verify-approvals");
    report.metadata["management_approvals_verified"] = json!(true);
    report.metadata["expected_domain_verified"] = json!(true);
    report.metadata["aoem_library"] = json!(library);
    report.metadata["aoem_library_sha256"] = json!(hex(&expected_library));
    report.metadata["observed_initial_root"] = json!(hex(&verified.state().root()));
    report.metadata["observed_final_commitment"] = json!(hex(&verified.final_commitment()));
    // Derived output still requires independent operator review. It is not a
    // complete service configuration or authority to install a chain.
    report.review_only_startup_fragment = Some(StartupFragment {
        genesis,
        uca_initialization: material,
    });
    Ok(())
}

fn reject_ignored_grant_fields(
    bytes: &[u8],
    manifest: &UcaGenesisManifestV1,
) -> Result<(), CtlError> {
    // Other original manifest records deny unknown fields. The shared grant
    // serves compatibility callers too, so tighten only this operator input.
    // Discover names from its Serialize implementation, not a second schema.
    // IgnoredAny skips numbers; money never passes through Value / f64.
    type Fields = BTreeMap<String, serde::de::IgnoredAny>;
    #[derive(Deserialize)]
    struct Entry {
        grant: Fields,
    }
    #[derive(Deserialize)]
    struct Account {
        grants: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Shape {
        accounts: Vec<Account>,
    }
    let invalid = |e| CtlError::InvalidArgument(format!("manifest grant fields: {e}"));
    let shape: Shape = serde_json::from_slice(bytes).map_err(invalid)?;
    for (input, account) in shape.accounts.iter().zip(&manifest.accounts) {
        for (entry, original) in input.grants.iter().zip(&account.grants) {
            let canonical = serde_json::to_vec(&original.grant).map_err(invalid)?;
            let expected: Fields = serde_json::from_slice(&canonical).map_err(invalid)?;
            if entry.grant.keys().ne(expected.keys()) {
                return Err(CtlError::InvalidArgument(
                    "unknown UCA grant field is not signed by the original contract".into(),
                ));
            }
        }
    }
    Ok(())
}

fn parse_digest(value: &str) -> Result<[u8; 32], CtlError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(CtlError::InvalidArgument(
            "digest must be 64 lowercase hex characters".into(),
        ));
    }
    let mut result = [0; 32];
    for (i, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .map_err(|error| CtlError::InvalidArgument(error.to_string()))?;
    }
    Ok(result)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[usize::from(byte >> 4)] as char);
        result.push(DIGITS[usize::from(byte & 15)] as char);
    }
    result
}

fn file_digest(path: &Path) -> Result<[u8; 32], CtlError> {
    let failed = |error| CtlError::FileReadFailed(format!("AOEM hash {}: {error}", path.display()));
    let mut file = File::open(path).map_err(failed)?;
    if !file.metadata().map_err(failed)?.is_file() {
        return Err(CtlError::InvalidArgument(
            "AOEM library must be a regular file".into(),
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(failed)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.finalize().into())
}
