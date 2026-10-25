//! Offline, purpose-bound node identity preparation using the existing custody.
use crate::{error::CtlError, output};
use clap::{Args, Subcommand};
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
    let identity = if create {
        purpose_keys::generate(&args.key_file, args.purpose, args.chain_id)
    } else {
        purpose_keys::inspect(&args.key_file, args.purpose, args.chain_id)
    }
    .map_err(|error| {
        CtlError::IntegrationFailed(format!("protected purpose identity: {error:#}"))
    })?;
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
