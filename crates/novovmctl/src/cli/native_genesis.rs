use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Args)]
#[command(
    about = "Read-only identity and asset genesis preparation; never creates keys, a chain or a service"
)]
pub struct NativeGenesisArgs {
    #[command(subcommand)]
    pub command: NativeGenesisCommand,
}

#[derive(Debug, Subcommand)]
pub enum NativeGenesisCommand {
    /// Review the manifest and optionally select one original approval transcript.
    Prepare(PrepareArgs),
    /// Verify independent identity/asset approvals and derive review-only startup material; not launch approval.
    VerifyApprovals(VerifyArgs),
}

#[derive(Debug, Args)]
pub struct PrepareArgs {
    /// Public UcaGenesisManifestV2 JSON, with the old genesis commitment zeroed.
    #[arg(long)]
    pub manifest: PathBuf,
    /// Select one subject from this manifest/domain's canonically ordered approval list.
    /// Omit to review only: no signing task or per-subject message is selected.
    #[arg(long)]
    pub approval_index: Option<usize>,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    #[command(flatten)]
    pub input: PrepareArgs,
    /// Public UcaGenesisApprovalV2 signatures for identity, asset management and configured recovery subjects.
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
