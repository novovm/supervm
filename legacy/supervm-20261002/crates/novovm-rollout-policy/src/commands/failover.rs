use crate::cli::failover::FailoverCommand;
use crate::error::PolicyError;

pub fn dispatch(cmd: FailoverCommand) -> Result<i32, PolicyError> {
    match cmd {
        FailoverCommand::SeedEvaluate(args) => {
            novovm_rollout_policy::policy::failover::seed_evaluate::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        FailoverCommand::RegionEvaluate(args) => {
            novovm_rollout_policy::policy::failover::region_evaluate::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        FailoverCommand::PolicyMatrixBuild(args) => {
            novovm_rollout_policy::policy::failover::policy_matrix_build::run_with_args(&args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
    }
}
