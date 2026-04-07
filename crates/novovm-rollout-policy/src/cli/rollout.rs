use super::PassthroughArgs;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct RolloutArgs {
    #[command(subcommand)]
    pub command: RolloutCommand,
}

#[derive(Debug, Subcommand)]
pub enum RolloutCommand {
    #[command(name = "controller-dispatch-evaluate")]
    ControllerDispatchEvaluate(PassthroughArgs),
    #[command(name = "decision-dashboard-export")]
    DecisionDashboardExport(PassthroughArgs),
    #[command(name = "decision-dashboard-consumer")]
    DecisionDashboardConsumer(PassthroughArgs),
    #[command(name = "decision-delivery")]
    DecisionDelivery(PassthroughArgs),
    #[command(name = "decision-route")]
    DecisionRoute(PassthroughArgs),
}
