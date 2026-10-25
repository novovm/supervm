//! Offline, purpose-bound node identity preparation using the existing custody.
use crate::{error::CtlError, output};
use clap::{Args, Subcommand};
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade, AoemExecOpenOptions};
use novovm_relay::purpose_keys::{self, PublicIdentityV1, PurposeV1};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Args)]
#[command(about = "Prepare protected test-node purpose keys; never starts a chain or service")]
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
}

#[derive(Debug, Args)]
pub struct KeyArgs {
    /// Absolute key path in an already prepared private directory under the eventual service account.
    #[arg(long)]
    pub key_file: PathBuf,
    /// Independent node role: validator or transport. Not a UCA asset key.
    #[arg(long, value_parser = parse_purpose)]
    pub purpose: PurposeV1,
    /// Explicit independent test-chain domain; this is not genesis activation.
    #[arg(long)]
    pub chain_id: u64,
    /// Explicit trusted AOEM library. Required for ML-DSA-65 validator generation/inspection.
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
        _ => Err("purpose must be validator or transport".into()),
    }
}

#[derive(Serialize)]
struct Report {
    action: &'static str,
    identity: PublicIdentityV1,
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
    };
    let identity = if args.purpose == PurposeV1::Validator {
        validator_identity(&args, create)?
    } else {
        if args.library.is_some() || args.library_sha256.is_some() {
            return Err(CtlError::InvalidArgument("AOEM arguments are for validator ML-DSA-65 only; transport remains a separate purpose".into()));
        }
        if create {
            purpose_keys::generate(&args.key_file, args.purpose, args.chain_id)
        } else {
            purpose_keys::inspect(&args.key_file, args.purpose, args.chain_id)
        }
        .map_err(|error| {
            CtlError::IntegrationFailed(format!("protected purpose identity: {error:#}"))
        })?
    };
    Ok(Report {
        action,
        identity,
        protected_key_created: create,
        private_material_exported: false,
        database_created: false,
        validator_started: false,
        service_started: false,
        genesis_activated: false,
        consensus_signed: false,
        asset_authority_granted: false,
        whole_node_post_quantum_qualified: false,
    })
}

fn validator_identity(args: &KeyArgs, create: bool) -> Result<PublicIdentityV1, CtlError> {
    let library = args.library.as_ref().ok_or_else(|| {
        CtlError::InvalidArgument(
            "validator ML-DSA-65 requires --library and --library-sha256; no Ed25519 fallback"
                .into(),
        )
    })?;
    let digest = args.library_sha256.as_deref().ok_or_else(|| {
        CtlError::InvalidArgument(
            "validator ML-DSA-65 requires independently approved --library-sha256".into(),
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
            .sign(
                public,
                secret,
                challenge,
                purpose_keys::VALIDATOR_PAIR_CONTEXT,
            )
            .map(|_| ())
            .map_err(Into::into)
    };
    let identity = if create {
        purpose_keys::generate_validator(
            &args.key_file,
            args.chain_id,
            || {
                let key = signer.generate_keypair()?;
                purpose_keys::ValidatorKeyMaterialV2::new(key.public_key, key.expanded_secret_key)
            },
            check,
        )
    } else {
        purpose_keys::inspect_validator(&args.key_file, args.chain_id, check)
    }
    .map_err(|error| {
        CtlError::IntegrationFailed(format!("protected validator identity: {error:#}"))
    })?;
    if super::native_genesis::file_digest(&library)? != expected {
        return Err(CtlError::IntegrationFailed(
            "AOEM library changed during purpose identity operation".into(),
        ));
    }
    Ok(identity)
}
