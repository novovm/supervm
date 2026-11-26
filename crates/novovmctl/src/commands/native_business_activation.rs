//! Offline review and signatures for the original consensus activation transcript.
use super::native_genesis::{file_digest, parse_digest};
use crate::{error::CtlError, output};
use clap::{Args as ClapArgs, Subcommand};
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_node::native_pipeline::{
    consensus::{
        business_activation::{review_activation, MAX_ACTIVATION_HEADER_BYTES},
        statement::ParentPoint,
        wire::activation::ActivationApproval,
    },
    service::config::ResidentConfig,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const COMMAND: &str = "native-business-activation";

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The operator's original resident node configuration.
    #[arg(long)]
    node_config: PathBuf,
    #[arg(long)]
    library_sha256: String,
    /// Previously verified parent of the certified anchor, in original ParentPoint JSON.
    #[arg(long)]
    preceding_parent: PathBuf,
    #[arg(long)]
    preceding_parent_sha256: String,
    /// Original binary finalized header for the last block before the upgrade.
    #[arg(long)]
    finalized_anchor: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Verify finality and render the exact subject for operator review.
    Prepare,
    /// Approve the reviewed subject with this node's existing validator key.
    Approve {
        #[arg(long)]
        subject_sha256: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify distinct signatures and the original weighted quorum.
    Assemble {
        #[arg(long)]
        subject_sha256: String,
        #[arg(long = "approval", required = true, num_args = 1)]
        approvals: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
}

pub fn run(args: Args) -> Result<(), CtlError> {
    let result = execute(args).and_then(|value| output::print_success_json(COMMAND, &value));
    if let Err(error) = &result {
        output::print_error_json(COMMAND, error);
    }
    result
}

fn execute(args: Args) -> Result<Value, CtlError> {
    let preceding = bounded(&args.preceding_parent, 4096)?;
    let expected = parse_digest(&args.preceding_parent_sha256)?;
    if <[u8; 32]>::from(Sha256::digest(&preceding)) != expected {
        return Err(CtlError::InvalidArgument(
            "preceding checkpoint SHA-256 mismatch".into(),
        ));
    }
    let preceding: ParentPoint = serde_json::from_slice(&preceding).map_err(invalid)?;
    let anchor = bounded(&args.finalized_anchor, MAX_ACTIVATION_HEADER_BYTES)?;
    let config = ResidentConfig::load(&args.node_config).map_err(invalid)?;
    let library_sha256 = parse_digest(&args.library_sha256)?;
    check_library(&config.library, library_sha256)?;
    let runtime =
        AoemExecFacade::open(&config.library, AoemExecOpenOptions::default()).map_err(invalid)?;
    let reviewed = review_activation(&config, &runtime, &preceding, &anchor).map_err(invalid)?;
    let subject = reviewed.subject();
    let transcript = subject.signing_bytes().map_err(invalid)?;
    let hash = Sha256::digest(transcript);
    let subject_sha256 = format!("{hash:x}");
    let digest: [u8; 32] = hash.into();
    let report = match args.command {
        Command::Prepare => {
            json!({"action":"prepare", "subject_sha256":subject_sha256, "activation":subject})
        }
        Command::Approve {
            subject_sha256: expected,
            output,
        } => {
            check_subject(&expected, digest)?;
            let approval = reviewed.approve(&runtime).map_err(invalid)?;
            check_library(&config.library, library_sha256)?;
            write_new(&output, &approval)?;
            json!({"action":"approve", "subject_sha256":subject_sha256, "signer":approval.signer})
        }
        Command::Assemble {
            subject_sha256: expected,
            approvals,
            output,
        } => {
            check_subject(&expected, digest)?;
            if approvals.len() > config.genesis.validators.len() {
                return Err(CtlError::InvalidArgument(
                    "approval count exceeds the original validator set".into(),
                ));
            }
            let approvals = approvals
                .iter()
                .map(|path| {
                    serde_json::from_slice::<ActivationApproval>(&bounded(path, 32 * 1024)?)
                        .map_err(invalid)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let bundle = reviewed.assemble(approvals).map_err(invalid)?;
            check_library(&config.library, library_sha256)?;
            write_new(&output, &bundle)?;
            json!({"action":"assemble", "subject_sha256":subject_sha256, "approval_count":bundle.approvals.len()})
        }
    };
    Ok(report)
}

fn check_subject(expected: &str, digest: [u8; 32]) -> Result<(), CtlError> {
    if parse_digest(expected)? != digest {
        return Err(CtlError::InvalidArgument(
            "approved subject SHA-256 mismatch".into(),
        ));
    }
    Ok(())
}

fn check_library(path: &Path, digest: [u8; 32]) -> Result<(), CtlError> {
    if file_digest(path)? != digest {
        return Err(CtlError::InvalidArgument(
            "resident AOEM library SHA-256 mismatch".into(),
        ));
    }
    Ok(())
}

fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>, CtlError> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take((limit + 1) as u64).read_to_end(&mut bytes))
        .map_err(|error| CtlError::FileReadFailed(error.to_string()))?;
    if bytes.len() > limit {
        return Err(CtlError::InvalidArgument(
            "activation input exceeds byte limit".into(),
        ));
    }
    Ok(bytes)
}

fn write_new(path: &Path, value: &impl Serialize) -> Result<(), CtlError> {
    let bytes = serde_json::to_vec(value).map_err(invalid)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| CtlError::FileWriteFailed(error.to_string()))?;
    output
        .write_all(&bytes)
        .and_then(|()| output.sync_all())
        .map_err(|error| CtlError::FileWriteFailed(error.to_string()))
}

fn invalid(error: impl std::fmt::Display) -> CtlError {
    CtlError::InvalidArgument(error.to_string())
}
