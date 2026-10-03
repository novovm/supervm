use crate::cli::failover::FailoverCommand;
use crate::cli::overlay::OverlayCommand;
use crate::cli::risk::RiskCommand;
use crate::cli::rollout::RolloutCommand;
use crate::cli::PassthroughArgs;
use crate::error::PolicyError;

use super::{failover, overlay, risk, rollout};

pub fn is_flat_tool(tool: &str) -> bool {
    matches!(
        tool,
        "overlay-auto-profile"
            | "overlay-relay-discovery-merge"
            | "overlay-relay-health-refresh"
            | "rollout-decision-dashboard-export"
            | "rollout-decision-dashboard-consumer"
            | "rollout-decision-delivery"
            | "rollout-decision-route"
            | "risk-action-eval"
            | "risk-matrix-select"
            | "risk-blocked-select"
            | "risk-blocked-map-build"
            | "risk-level-set"
            | "risk-policy-profile-select"
            | "risk-action-matrix-build"
            | "failover-policy-matrix-build"
    )
}

pub fn dispatch_flat(tool: &str, args: Vec<String>) -> Result<i32, PolicyError> {
    let passthrough = PassthroughArgs { args };
    match tool {
        "overlay-auto-profile" => overlay::dispatch(OverlayCommand::AutoProfile(passthrough)),
        "overlay-relay-discovery-merge" => {
            overlay::dispatch(OverlayCommand::RelayDiscoveryMerge(passthrough))
        }
        "overlay-relay-health-refresh" => {
            overlay::dispatch(OverlayCommand::RelayHealthRefresh(passthrough))
        }
        "rollout-decision-dashboard-export" => {
            rollout::dispatch(RolloutCommand::DecisionDashboardExport(passthrough))
        }
        "rollout-decision-dashboard-consumer" => {
            rollout::dispatch(RolloutCommand::DecisionDashboardConsumer(passthrough))
        }
        "rollout-decision-delivery" => {
            rollout::dispatch(RolloutCommand::DecisionDelivery(passthrough))
        }
        "rollout-decision-route" => rollout::dispatch(RolloutCommand::DecisionRoute(passthrough)),
        "risk-action-eval" => risk::dispatch(RiskCommand::ActionEval(passthrough)),
        "risk-matrix-select" => risk::dispatch(RiskCommand::MatrixSelect(passthrough)),
        "risk-blocked-select" => risk::dispatch(RiskCommand::BlockedSelect(passthrough)),
        "risk-blocked-map-build" => risk::dispatch(RiskCommand::BlockedMapBuild(passthrough)),
        "risk-level-set" => risk::dispatch(RiskCommand::LevelSet(passthrough)),
        "risk-policy-profile-select" => {
            risk::dispatch(RiskCommand::PolicyProfileSelect(passthrough))
        }
        "risk-action-matrix-build" => risk::dispatch(RiskCommand::ActionMatrixBuild(passthrough)),
        "failover-policy-matrix-build" => {
            failover::dispatch(FailoverCommand::PolicyMatrixBuild(passthrough))
        }
        _ => Err(PolicyError::InvalidArgument(format!(
            "unknown rollout-policy tool: {tool}"
        ))),
    }
}
