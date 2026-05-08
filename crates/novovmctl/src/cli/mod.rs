pub mod daemon;
pub mod governance_stats;
pub mod lifecycle;
pub mod rollout;
pub mod rollout_control;
pub mod up;

use clap::{Parser, Subcommand};

pub use daemon::DaemonArgs;
pub use governance_stats::GovernanceStatsArgs;
pub use lifecycle::LifecycleArgs;
pub use rollout::RolloutArgs;
pub use rollout_control::RolloutControlArgs;
pub use up::UpArgs;

#[derive(Debug, Parser)]
#[command(name = "novovmctl")]
#[command(about = "NOVOVM cross-platform operational CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: TopCommand,
}

#[derive(Debug, Subcommand)]
pub enum TopCommand {
    Up(UpArgs),
    GovernanceStats(GovernanceStatsArgs),
    RolloutControl(RolloutControlArgs),
    Rollout(RolloutArgs),
    Lifecycle(LifecycleArgs),
    Daemon(DaemonArgs),
}
