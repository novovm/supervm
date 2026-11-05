//! Offline, purpose-bound node identity preparation using the existing custody.
use crate::{error::CtlError, output};
use clap::{Args, Subcommand};
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade, AoemExecOpenOptions};
use novovm_relay::purpose_keys::{self, PublicIdentityV1, PurposeV1};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, Args)]
#[command(
    about = "Prepare protected purpose keys or qualify their runtime; never starts a chain or service"
)]
pub struct NativeIdentityArgs {
    #[command(subcommand)]
    pub command: NativeIdentityCommand,
}

#[derive(Debug, Subcommand)]
pub enum NativeIdentityCommand {
    /// Explicitly create one local protected key. Refuses overwrite; outputs public material only.
    Generate(KeyArgs),
    /// Read-only check of existing protected material, expected purpose and isolated chain id.
    Inspect(KeyArgs),
    /// Read-only qualification of an explicit AOEM runtime; no keys, custody or network.
    QualifyRuntime(RuntimeArgs),
}

#[derive(Debug, Args)]
pub struct RuntimeArgs {
    /// Explicit AOEM library to qualify through the original native verification owner.
    #[arg(long)]
    pub library: PathBuf,
    /// Independently approved lowercase SHA-256 of this exact platform artifact.
    #[arg(long)]
    pub library_sha256: String,
}

#[derive(Debug, Args)]
pub struct KeyArgs {
    /// Absolute key path in a private directory under its intended holder or service account.
    #[arg(long)]
    pub key_file: PathBuf,
    /// Independent key purpose; creating a key does not register identity or asset authority.
    #[arg(long, value_parser = parse_purpose)]
    pub purpose: PurposeV1,
    /// Explicit independent test-chain domain; this is not genesis activation.
    #[arg(long)]
    pub chain_id: u64,
    /// Explicit trusted AOEM library. Required for either ML-DSA-65 purpose.
    #[arg(long, requires = "library_sha256")]
    pub library: Option<PathBuf>,
    /// Independently approved lowercase SHA-256; checked before and after AOEM use.
    #[arg(long, requires = "library")]
    pub library_sha256: Option<String>,
}

fn parse_purpose(value: &str) -> Result<PurposeV1, String> {
    match value {
        "validator" => Ok(PurposeV1::Validator),
        "transport" => Ok(PurposeV1::Transport),
        "identity" => Ok(PurposeV1::Identity),
        "asset-management" => Ok(PurposeV1::AssetManagement),
        _ => Err("purpose must be validator, transport, identity or asset-management".into()),
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum Report {
    Identity(IdentityReport),
    Runtime(RuntimeReport),
}

#[derive(Serialize)]
struct IdentityReport {
    action: &'static str,
    identity: PublicIdentityV1,
    /// Local genesis descriptor address, not an on-chain opening or a balance.
    #[serde(skip_serializing_if = "Option::is_none")]
    asset_account_hex: Option<String>,
    protected_key_created: bool,
    private_material_exported: bool,
    database_created: bool,
    validator_started: bool,
    service_started: bool,
    genesis_activated: bool,
    consensus_signed: bool,
    asset_authority_granted: bool,
    whole_node_post_quantum_qualified: bool,
}

#[derive(Serialize)]
struct RuntimeReport {
    action: &'static str,
    qualification: &'static str,
    algorithm: &'static str,
    signature_suite: u16,
    library: PathBuf,
    library_sha256: String,
    verification_qualified: bool,
    scalar_verification_kat_passed: bool,
    unified_semantic_verification_kat_passed: bool,
    mixed_batch_verification_kat_passed: bool,
    primitive_sign_interface_size_checked: bool,
    signing_tested: bool,
    keygen_tested: bool,
    protected_key_created: bool,
    custody_accessed: bool,
    database_created: bool,
    service_started: bool,
    whole_node_post_quantum_qualified: bool,
}

pub fn run(args: NativeIdentityArgs) -> Result<(), CtlError> {
    let result =
        execute(args).and_then(|report| output::print_success_json("native-identity", &report));
    if let Err(error) = &result {
        output::print_error_json("native-identity", error);
    }
    result
}

fn execute(args: NativeIdentityArgs) -> Result<Report, CtlError> {
    let (action, args, create) = match args.command {
        NativeIdentityCommand::Generate(args) => ("generate", args, true),
        NativeIdentityCommand::Inspect(args) => ("inspect", args, false),
        NativeIdentityCommand::QualifyRuntime(args) => {
            return qualify_runtime(args).map(Report::Runtime);
        }
    };
    let identity = mldsa_identity(&args, create)?;
    // Genesis accepts opaque 20/32-byte account locations. This names a newly
    // prepared descriptor only; current authority still comes from the signed
    // initialization and its existing state owner, not this derivation.
    let asset_account_hex = (args.purpose == PurposeV1::AssetManagement).then(|| {
        let mut hash = Sha256::new();
        hash.update(b"NOVOVM/asset/local-genesis-descriptor/v1\0");
        hash.update(&identity.public_key);
        super::native_genesis::hex(&hash.finalize())
    });
    Ok(Report::Identity(IdentityReport {
        action,
        identity,
        asset_account_hex,
        protected_key_created: create,
        private_material_exported: false,
        database_created: false,
        validator_started: false,
        service_started: false,
        genesis_activated: false,
        consensus_signed: false,
        asset_authority_granted: false,
        whole_node_post_quantum_qualified: false,
    }))
}

fn qualify_runtime(args: RuntimeArgs) -> Result<RuntimeReport, CtlError> {
    let library = args.library.canonicalize().map_err(|error| {
        CtlError::FileReadFailed(format!("explicit AOEM runtime library: {error}"))
    })?;
    novovm_relay::transport_identity::load_transport_verifier(&library, &args.library_sha256)
        .map_err(|error| {
            CtlError::IntegrationFailed(format!(
                "native ML-DSA-65 runtime qualification: {error:#}"
            ))
        })?;
    Ok(RuntimeReport {
        action: "qualify-runtime",
        qualification: "ml-dsa-65-native-verification-owner",
        algorithm: "ML-DSA-65",
        signature_suite: 65,
        library,
        library_sha256: args.library_sha256,
        verification_qualified: true,
        scalar_verification_kat_passed: true,
        unified_semantic_verification_kat_passed: true,
        mixed_batch_verification_kat_passed: true,
        primitive_sign_interface_size_checked: true,
        signing_tested: false,
        keygen_tested: false,
        protected_key_created: false,
        custody_accessed: false,
        database_created: false,
        service_started: false,
        whole_node_post_quantum_qualified: false,
    })
}

fn mldsa_identity(args: &KeyArgs, create: bool) -> Result<PublicIdentityV1, CtlError> {
    let library = args.library.as_ref().ok_or_else(|| {
        CtlError::InvalidArgument(
            "node ML-DSA-65 identity requires --library and --library-sha256; no Ed25519 fallback"
                .into(),
        )
    })?;
    let digest = args.library_sha256.as_deref().ok_or_else(|| {
        CtlError::InvalidArgument(
            "node ML-DSA-65 identity requires independently approved --library-sha256".into(),
        )
    })?;
    let expected = super::native_genesis::parse_digest(digest)?;
    let library = library
        .canonicalize()
        .map_err(|error| CtlError::FileReadFailed(format!("explicit AOEM library: {error}")))?;
    if super::native_genesis::file_digest(&library)? != expected {
        return Err(CtlError::InvalidArgument(
            "AOEM library SHA-256 mismatch before load".into(),
        ));
    }
    let runtime = AoemExecFacade::open(&library, AoemExecOpenOptions::default())
        .map_err(|error| CtlError::IntegrationFailed(format!("original AOEM loader: {error:#}")))?;
    let signer = runtime
        .mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)
        .map_err(|error| {
            CtlError::IntegrationFailed(format!("AOEM ML-DSA-65 qualification: {error}"))
        })?;
    let check = |public: &[u8], secret: &[u8], challenge: &[u8]| {
        signer
            .sign(public, secret, challenge, args.purpose.pair_context())
            .map(|_| ())
            .map_err(Into::into)
    };
    let identity = if create {
        purpose_keys::generate_mldsa(
            &args.key_file,
            args.purpose,
            args.chain_id,
            || {
                let key = signer.generate_keypair()?;
                purpose_keys::PurposeKeyMaterialV3::new(key.public_key, key.expanded_secret_key)
            },
            check,
        )
    } else {
        purpose_keys::inspect_mldsa(&args.key_file, args.purpose, args.chain_id, check)
    }
    .map_err(|error| {
        CtlError::IntegrationFailed(format!("protected ML-DSA-65 purpose identity: {error:#}"))
    })?;
    if super::native_genesis::file_digest(&library)? != expected {
        return Err(CtlError::IntegrationFailed(
            "AOEM library changed during purpose identity operation".into(),
        ));
    }
    Ok(identity)
}
