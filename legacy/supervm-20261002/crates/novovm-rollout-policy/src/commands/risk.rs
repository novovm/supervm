use crate::cli::risk::RiskCommand;
use crate::error::PolicyError;

pub fn dispatch(cmd: RiskCommand) -> Result<i32, PolicyError> {
    match cmd {
        RiskCommand::SloEvaluate(args) => {
            novovm_rollout_policy::policy::risk::slo_evaluate::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        RiskCommand::CircuitBreakerEvaluate(args) => {
            novovm_rollout_policy::policy::risk::circuit_breaker_evaluate::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        RiskCommand::ActionEval(args) => {
            novovm_rollout_policy::policy::risk::action_eval::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        RiskCommand::ActionMatrixBuild(args) => {
            novovm_rollout_policy::policy::risk::action_matrix_build::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        RiskCommand::MatrixSelect(args) => {
            novovm_rollout_policy::policy::risk::matrix_select::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        RiskCommand::BlockedSelect(args) => {
            novovm_rollout_policy::policy::risk::blocked_select::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        RiskCommand::BlockedMapBuild(args) => {
            novovm_rollout_policy::policy::risk::blocked_map_build::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        RiskCommand::LevelSet(args) => {
            novovm_rollout_policy::policy::risk::level_set::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        RiskCommand::PolicyProfileSelect(args) => {
            novovm_rollout_policy::policy::risk::policy_profile_select::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
    }
}
