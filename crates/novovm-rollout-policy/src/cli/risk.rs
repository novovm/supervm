use super::PassthroughArgs;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct RiskArgs {
    #[command(subcommand)]
    pub command: RiskCommand,
}

#[derive(Debug, Subcommand)]
pub enum RiskCommand {
    #[command(name = "slo-evaluate")]
    SloEvaluate(PassthroughArgs),
    #[command(name = "circuit-breaker-evaluate")]
    CircuitBreakerEvaluate(PassthroughArgs),
    #[command(name = "action-eval")]
    ActionEval(PassthroughArgs),
    #[command(name = "action-matrix-build")]
    ActionMatrixBuild(PassthroughArgs),
    #[command(name = "matrix-select")]
    MatrixSelect(PassthroughArgs),
    #[command(name = "blocked-select")]
    BlockedSelect(PassthroughArgs),
    #[command(name = "blocked-map-build")]
    BlockedMapBuild(PassthroughArgs),
    #[command(name = "level-set")]
    LevelSet(PassthroughArgs),
    #[command(name = "policy-profile-select")]
    PolicyProfileSelect(PassthroughArgs),
}
