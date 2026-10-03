use crate::cli::overlay::OverlayCommand;
use crate::error::PolicyError;

pub fn dispatch(cmd: OverlayCommand) -> Result<i32, PolicyError> {
    match cmd {
        OverlayCommand::AutoProfile(args) => {
            novovm_rollout_policy::policy::overlay::auto_profile::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        OverlayCommand::RelayDiscoveryMerge(args) => {
            novovm_rollout_policy::policy::overlay::relay_discovery_merge::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
        OverlayCommand::RelayHealthRefresh(args) => {
            novovm_rollout_policy::policy::overlay::relay_health_refresh::run_with_args(args.args)
                .map(|_| 0)
                .map_err(|err| PolicyError::LaunchToolFailed(err.to_string()))
        }
    }
}
