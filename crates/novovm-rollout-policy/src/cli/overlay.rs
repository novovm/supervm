use super::PassthroughArgs;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct OverlayArgs {
    #[command(subcommand)]
    pub command: OverlayCommand,
}

#[derive(Debug, Subcommand)]
pub enum OverlayCommand {
    #[command(name = "auto-profile", alias = "auto-profile-select")]
    AutoProfile(PassthroughArgs),
    #[command(name = "relay-discovery-merge")]
    RelayDiscoveryMerge(PassthroughArgs),
    #[command(name = "relay-health-refresh", alias = "relay-score-refresh")]
    RelayHealthRefresh(PassthroughArgs),
}
