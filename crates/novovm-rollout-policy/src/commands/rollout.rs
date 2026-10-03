use crate::cli::rollout::RolloutCommand;
use crate::error::PolicyError;
use novovm_rollout_policy::policy::rollout;

pub fn dispatch(cmd: RolloutCommand) -> Result<i32, PolicyError> {
    match cmd {
        RolloutCommand::ControllerDispatchEvaluate(args) => {
            rollout::controller_dispatch_evaluate::run_with_args(&args.args)
                .map_err(|e| PolicyError::LaunchToolFailed(e.to_string()))?;
            Ok(0)
        }
        RolloutCommand::DecisionDashboardExport(args) => {
            rollout::decision_dashboard_export::run_with_args(&args.args)
                .map_err(|e| PolicyError::LaunchToolFailed(e.to_string()))?;
            Ok(0)
        }
        RolloutCommand::DecisionDashboardConsumer(args) => {
            rollout::decision_dashboard_consumer::run_with_args(&args.args)
                .map_err(|e| PolicyError::LaunchToolFailed(e.to_string()))?;
            Ok(0)
        }
        RolloutCommand::DecisionDelivery(args) => {
            rollout::decision_delivery::run_with_args(&args.args)
                .map_err(|e| PolicyError::LaunchToolFailed(e.to_string()))?;
            Ok(0)
        }
        RolloutCommand::DecisionRoute(args) => {
            rollout::decision_route::run_with_args(&args.args)
                .map_err(|e| PolicyError::LaunchToolFailed(e.to_string()))?;
            Ok(0)
        }
    }
}
