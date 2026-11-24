//! Native SCM adapter for the existing node, not a process supervisor.
use super::bad;
use crate::error::CtlError;
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use serde_json::json;
use serde_json::Value;
use std::ffi::OsString;
use std::path::PathBuf;

const RESTART_DELAYS_MS: [u64; 2] = [5_000, 15_000];
const FAILURE_RESET_SECONDS: u64 = 86_400;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StartType {
    #[default]
    Demand,
    Automatic,
}

pub(super) struct NodeServiceSpec {
    pub name: String,
    pub binary: PathBuf,
    pub config: PathBuf,
    pub account_name: String,
    pub start_type: StartType,
}

impl NodeServiceSpec {
    pub fn validate(&self) -> Result<(), CtlError> {
        if self.name.is_empty()
            || self.name.len() > 128
            || !self
                .name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
            || self.account_name.trim() != self.account_name
            || self.account_name.is_empty()
            || self.account_name.chars().any(char::is_control)
        {
            return Err(bad(
                "SCM service requires an exact name and explicit runtime account",
            ));
        }
        for path in [&self.binary, &self.config] {
            let value = path.to_str().ok_or_else(|| bad("SCM path must be UTF-8"))?;
            if !path.is_absolute()
                || value.chars().any(|c| c.is_control() || c == '"')
                || value.ends_with('/')
                || value.ends_with('\\')
            {
                return Err(bad("SCM binary/config must be literal absolute file paths"));
            }
        }
        Ok(())
    }

    pub fn launch_arguments(&self) -> Vec<OsString> {
        vec![
            "--service".into(),
            self.config.as_os_str().to_owned(),
            "--service-name".into(),
            self.name.clone().into(),
        ]
    }

    pub fn recovery_policy(&self) -> Option<Value> {
        (self.start_type == StartType::Automatic).then(|| {
            serde_json::json!({
                "restart_delays_ms": RESTART_DELAYS_MS,
                "reset_period_seconds": FAILURE_RESET_SECONDS,
                "restart_on_non_crash_failure": true
            })
        })
    }

    #[cfg(any(windows, test))]
    fn command_line(&self) -> Result<String, CtlError> {
        self.validate()?;
        // These literal file paths cannot contain quotes, controls or a trailing
        // slash. This is exactly the windows-service quoting for this argv set.
        let quote = |s: &str| {
            if s.contains(' ') {
                format!("\"{s}\"")
            } else {
                s.to_owned()
            }
        };
        Ok(format!(
            "{} --service {} --service-name {}",
            quote(self.binary.to_str().expect("validated UTF-8")),
            quote(self.config.to_str().expect("validated UTF-8")),
            self.name
        ))
    }

    #[cfg(any(windows, test))]
    fn password_free_install(&self) -> bool {
        let account = canonical_account(&self.account_name);
        account == "nt authority\\localservice"
            || account == "nt authority\\networkservice"
            || account == format!("nt service\\{}", self.name.to_ascii_lowercase())
    }
}

#[cfg(any(windows, test))]
fn canonical_account(account: &str) -> String {
    match account.to_ascii_lowercase().as_str() {
        "localservice" => "nt authority\\localservice".into(),
        "networkservice" => "nt authority\\networkservice".into(),
        value => value.to_owned(),
    }
}

#[cfg(any(windows, test))]
fn verify_owned_config(
    spec: &NodeServiceSpec,
    command_line: &str,
    account_name: Option<&str>,
    own_process: bool,
    start_type: Option<StartType>,
    has_dependencies: bool,
) -> Result<(), CtlError> {
    if command_line != spec.command_line()?
        || account_name
            .is_none_or(|a| canonical_account(a) != canonical_account(&spec.account_name))
        || !own_process
        || start_type != Some(spec.start_type)
        || has_dependencies
    {
        return Err(bad(
            "SCM ownership differs from pinned node binary, argv, account or startup contract",
        ));
    }
    Ok(())
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::time::{Duration, Instant};
    use windows_service::service::{
        Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
        ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
        ServiceState, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    fn failure(error: windows_service::Error) -> CtlError {
        CtlError::IntegrationFailed(format!("SCM operation failed: {error}"))
    }

    pub(super) fn restart_actions() -> ServiceFailureActions {
        let mut actions: Vec<_> = RESTART_DELAYS_MS
            .map(|delay| ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_millis(delay),
            })
            .into();
        // SCM repeats the last action for subsequent failures.
        actions.push(ServiceAction {
            action_type: ServiceActionType::None,
            delay: Duration::ZERO,
        });
        ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(
                FAILURE_RESET_SECONDS,
            )),
            reboot_msg: None,
            command: None,
            actions: Some(actions),
        }
    }

    pub(super) fn verify_recovery(
        actual: &ServiceFailureActions,
        includes_reported_failures: bool,
    ) -> Result<(), CtlError> {
        let expected = restart_actions();
        if actual.reset_period != expected.reset_period
            || actual.actions != expected.actions
            || !includes_reported_failures
        {
            return Err(bad(
                "SCM failure recovery differs from pinned startup policy",
            ));
        }
        Ok(())
    }

    fn inspect(service: &Service, spec: &NodeServiceSpec) -> Result<Value, CtlError> {
        let config = service.query_config().map_err(failure)?;
        verify_owned_config(
            spec,
            config
                .executable_path
                .to_str()
                .ok_or_else(|| bad("SCM returned non-UTF8 command line"))?,
            config.account_name.as_deref().and_then(|a| a.to_str()),
            config.service_type == ServiceType::OWN_PROCESS,
            match config.start_type {
                ServiceStartType::OnDemand => Some(StartType::Demand),
                ServiceStartType::AutoStart => Some(StartType::Automatic),
                _ => None,
            },
            !config.dependencies.is_empty(),
        )?;
        if spec.start_type == StartType::Automatic {
            verify_recovery(
                &service.get_failure_actions().map_err(failure)?,
                service
                    .get_failure_actions_on_non_crash_failures()
                    .map_err(failure)?,
            )?;
        }
        let status = service.query_status().map_err(failure)?;
        Ok(
            json!({"service":spec.name,"manager":"windows_scm","state":format!("{:?}",status.current_state),"start_type":spec.start_type,"recovery_policy":spec.recovery_policy(),"ownership_checked":true,"health_verified":false,"custody_verified":false}),
        )
    }

    fn configure_automatic(
        service: &Service,
        spec: &NodeServiceSpec,
        mut info: ServiceInfo,
    ) -> Result<(), CtlError> {
        service
            .update_failure_actions(restart_actions())
            .map_err(failure)?;
        service
            .set_failure_actions_on_non_crash_failures(true)
            .map_err(failure)?;
        verify_recovery(
            &service.get_failure_actions().map_err(failure)?,
            service
                .get_failure_actions_on_non_crash_failures()
                .map_err(failure)?,
        )?;
        info.start_type = ServiceStartType::AutoStart;
        service.change_config(&info).map_err(failure)?;
        inspect(service, spec)?;
        Ok(())
    }

    pub(super) fn preflight(spec: &NodeServiceSpec, installing: bool) -> Result<Value, CtlError> {
        spec.validate()?;
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .map_err(failure)?;
        let service = manager.open_service(
            &spec.name,
            ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS,
        );
        if installing {
            if !spec.password_free_install() {
                return Err(bad("SCM creation requires explicit LocalService, NetworkService or matching virtual account; adopt an independently provisioned named account instead"));
            }
            match service {
                Err(windows_service::Error::Winapi(error))
                    if error.raw_os_error() == Some(1060) =>
                {
                    Ok(
                        json!({"service":spec.name,"manager":"windows_scm","state":"absent","ownership_checked":false,"health_verified":false,"custody_verified":false}),
                    )
                }
                Ok(_) => Err(bad(
                    "SCM install target exists; explicit adoption is required",
                )),
                Err(error) => Err(failure(error)),
            }
        } else {
            inspect(&service.map_err(failure)?, spec)
        }
    }

    pub(super) fn apply(spec: &NodeServiceSpec, action: &str) -> Result<Value, CtlError> {
        if !matches!(action, "install" | "start" | "stop") {
            return Err(bad("unsupported SCM mutation"));
        }
        // Root first preflights every selected owner. Recheck this owner before
        // its effect; never replace existing configuration or collect passwords.
        preflight(spec, action == "install")?;
        let access = if action == "install" {
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE
        } else {
            ServiceManagerAccess::CONNECT
        };
        let manager = ServiceManager::local_computer(None::<&str>, access).map_err(failure)?;
        if action == "install" {
            let account = canonical_account(&spec.account_name);
            let info = ServiceInfo {
                name: spec.name.clone().into(),
                display_name: spec.name.clone().into(),
                service_type: ServiceType::OWN_PROCESS,
                start_type: ServiceStartType::OnDemand,
                error_control: ServiceErrorControl::Normal,
                executable_path: spec.binary.clone(),
                launch_arguments: spec.launch_arguments(),
                dependencies: Vec::new(),
                account_name: Some(account.into()),
                account_password: None,
            };
            let automatic = spec.start_type == StartType::Automatic;
            let mut access = ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS;
            if automatic {
                access |=
                    ServiceAccess::CHANGE_CONFIG | ServiceAccess::START | ServiceAccess::DELETE;
            }
            let service = manager.create_service(&info, access).map_err(failure)?;
            if automatic {
                configure_automatic(&service, spec, info).map_err(|error| {
                    match service.delete() {
                        Ok(()) => error,
                        Err(cleanup) => bad(format!(
                            "{error}; failed to remove incomplete new SCM service {}: {cleanup}",
                            spec.name
                        )),
                    }
                })?;
            }
            return Ok(
                json!({"service":spec.name,"action":"install","created":true,"started":false,"auto_start":automatic,"start_type":spec.start_type,"recovery_policy":spec.recovery_policy(),"custody_verified":false}),
            );
        }
        let request_access = ServiceAccess::QUERY_CONFIG
            | ServiceAccess::QUERY_STATUS
            | if action == "start" {
                ServiceAccess::START
            } else {
                ServiceAccess::STOP
            };
        let service = manager
            .open_service(&spec.name, request_access)
            .map_err(failure)?;
        inspect(&service, spec)?;
        let target = if action == "start" {
            ServiceState::Running
        } else {
            ServiceState::Stopped
        };
        if service.query_status().map_err(failure)?.current_state != target {
            if action == "start" {
                service.start::<&str>(&[]).map_err(failure)?;
            } else {
                service.stop().map_err(failure)?;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let status = service.query_status().map_err(failure)?;
            if status.current_state == target {
                return inspect(&service, spec);
            }
            if Instant::now() >= deadline
                || (action == "start" && status.current_state == ServiceState::Stopped)
            {
                return Err(bad(format!("SCM {action} not completed: state={:?}, exit={:?}; no process kill or data rollback attempted",status.current_state,status.exit_code)));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

pub(super) fn preflight(spec: &NodeServiceSpec, installing: bool) -> Result<Value, CtlError> {
    #[cfg(windows)]
    {
        platform::preflight(spec, installing)
    }
    #[cfg(not(windows))]
    {
        let _ = (spec, installing);
        Err(bad("SCM observations require Windows"))
    }
}

pub(super) fn apply(spec: &NodeServiceSpec, action: &str) -> Result<Value, CtlError> {
    #[cfg(windows)]
    {
        platform::apply(spec, action)
    }
    #[cfg(not(windows))]
    {
        let _ = (spec, action);
        Err(bad("SCM mutation requires Windows"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec() -> NodeServiceSpec {
        NodeServiceSpec {
            name: "NOVOVM-node-isolated".into(),
            binary: std::env::temp_dir().join("release with spaces/novovm-node.exe"),
            config: std::env::temp_dir().join("node state/native.json"),
            account_name: "NT AUTHORITY\\LocalService".into(),
            start_type: StartType::Demand,
        }
    }
    #[test]
    fn literal_paths_produce_the_original_service_argv() {
        let s = spec();
        s.validate().unwrap();
        assert_eq!(
            s.launch_arguments(),
            vec![
                OsString::from("--service"),
                s.config.clone().into_os_string(),
                OsString::from("--service-name"),
                OsString::from(&s.name)
            ]
        );
        assert!(s.command_line().unwrap().starts_with('"'));
        assert!(s.command_line().unwrap().contains("\" --service \""));
    }
    #[test]
    fn adoption_checks_actual_command_account_and_service_type() {
        let mut s = spec();
        let cmd = s.command_line().unwrap();
        for start_type in [StartType::Demand, StartType::Automatic] {
            s.start_type = start_type;
            assert!(verify_owned_config(
                &s,
                &cmd,
                Some("LocalService"),
                true,
                Some(start_type),
                false
            )
            .is_ok());
            assert!(
                verify_owned_config(&s, &cmd, Some("LocalService"), true, None, false).is_err()
            );
        }
        let automatic = Some(StartType::Automatic);
        for (command, account, own, start, deps) in [
            (
                format!("{cmd} --create"),
                Some("LocalService"),
                true,
                automatic,
                false,
            ),
            (cmd.clone(), Some("NetworkService"), true, automatic, false),
            (cmd.clone(), None, true, automatic, false),
            (cmd.clone(), Some("LocalService"), false, automatic, false),
            (
                cmd.clone(),
                Some("LocalService"),
                true,
                Some(StartType::Demand),
                false,
            ),
            (cmd.clone(), Some("LocalService"), true, automatic, true),
        ] {
            assert!(verify_owned_config(&s, &command, account, own, start, deps).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn recovery_restarts_twice_and_rejects_policy_drift() {
        use std::time::Duration;
        use windows_service::service::{ServiceActionType, ServiceFailureResetPeriod};

        let mut policy = platform::restart_actions();
        let actions = policy.actions.as_ref().unwrap();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].action_type, ServiceActionType::Restart);
        assert_eq!(actions[0].delay, Duration::from_secs(5));
        assert_eq!(actions[1].action_type, ServiceActionType::Restart);
        assert_eq!(actions[1].delay, Duration::from_secs(15));
        assert_eq!(actions[2].action_type, ServiceActionType::None);
        assert_eq!(
            policy.reset_period,
            ServiceFailureResetPeriod::After(Duration::from_secs(86_400))
        );
        assert!(platform::verify_recovery(&policy, true).is_ok());
        assert!(platform::verify_recovery(&policy, false).is_err());
        policy.actions.as_mut().unwrap().pop();
        assert!(platform::verify_recovery(&policy, true).is_err());
    }
    #[test]
    fn no_implicit_or_password_account_is_created() {
        let mut s = spec();
        assert!(s.password_free_install());
        s.account_name = format!("NT SERVICE\\{}", s.name);
        assert!(s.password_free_install());
        s.account_name = "NT SERVICE\\another-owner".into();
        assert!(!s.password_free_install());
        s.account_name = "DOMAIN\\existing-user".into();
        assert!(!s.password_free_install());
        s.account_name.clear();
        assert!(s.validate().is_err());
    }
    #[test]
    fn service_names_and_file_argv_reject_ambiguous_forms() {
        let mut s = spec();
        s.name = "node --other".into();
        assert!(s.validate().is_err());
        s = spec();
        s.config = PathBuf::from("relative.json");
        assert!(s.validate().is_err());
        s = spec();
        s.config = std::env::temp_dir().join("quoted\".json");
        assert!(s.validate().is_err());
    }
}
