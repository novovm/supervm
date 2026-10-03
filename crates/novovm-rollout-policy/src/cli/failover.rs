use super::PassthroughArgs;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct FailoverArgs {
    #[command(subcommand)]
    pub command: FailoverCommand,
}

#[derive(Debug, Subcommand)]
pub enum FailoverCommand {
    #[command(name = "seed-evaluate")]
    SeedEvaluate(PassthroughArgs),
    #[command(name = "region-evaluate")]
    RegionEvaluate(PassthroughArgs),
    #[command(name = "policy-matrix-build")]
    PolicyMatrixBuild(PassthroughArgs),
}
