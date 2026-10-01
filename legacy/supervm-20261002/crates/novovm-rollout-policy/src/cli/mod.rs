pub mod failover;
pub mod overlay;
pub mod risk;
pub mod rollout;

use clap::{Args, Parser, Subcommand};
pub use failover::FailoverArgs;
pub use overlay::OverlayArgs;
pub use risk::RiskArgs;
pub use rollout::RolloutArgs;

#[derive(Debug, Clone, Args)]
pub struct PassthroughArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[derive(Debug, Parser)]
#[command(name = "novovm-rollout-policy")]
#[command(about = "NOVOVM rollout policy unified entrypoint")]
pub struct Cli {
    #[command(subcommand)]
    pub command: DomainCommand,
}

#[derive(Debug, Subcommand)]
pub enum DomainCommand {
    Overlay(OverlayArgs),
    Rollout(RolloutArgs),
    Risk(RiskArgs),
    Failover(FailoverArgs),
}
