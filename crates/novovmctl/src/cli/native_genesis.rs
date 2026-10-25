use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Args)]
#[command(
    about = "Read-only original UCA genesis preparation; never creates keys, a chain or a service"
)]
pub struct NativeGenesisArgs {
    #[command(subcommand)]
    pub command: NativeGenesisCommand,
}

#[derive(Debug, Subcommand)]
pub enum NativeGenesisCommand {
    /// Emit original canonical signing bytes and observed domain, without approving them.
    Prepare(PrepareArgs),
    /// Verify management signatures and derive review-only startup material; not launch approval.
    VerifyApprovals(VerifyArgs),
}

#[derive(Debug, Args)]
pub struct PrepareArgs {
    /// Public UcaGenesisManifestV1 JSON, with the old genesis commitment zeroed.
    #[arg(long)]
    pub manifest: PathBuf,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    #[command(flatten)]
    pub input: PrepareArgs,
    /// Public JSON array of original UcaGenesisApprovalV1 management signatures.
    #[arg(long)]
    pub approvals: PathBuf,
    /// Independently reviewed domain, exactly 64 lowercase hex characters.
    #[arg(long)]
    pub expected_domain: String,
    /// Explicit trusted AOEM library; original ABI/manifest checks remain enabled.
    #[arg(long)]
    pub library: PathBuf,
    /// Independently verified package library SHA-256, not a self-trusted auto pin.
    #[arg(long)]
    pub library_sha256: String,
}
