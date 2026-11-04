//! Native SCM adapter for the existing node, not a process supervisor.
use super::bad;
use crate::error::CtlError;
#[cfg(windows)]
use serde_json::json;
use serde_json::Value;
use std::ffi::OsString;
use std::path::PathBuf;

pub(super) struct NodeServiceSpec {
    pub name: String,
    pub binary: PathBuf,
    pub config: PathBuf,
    pub account_name: String,
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
    demand_start: bool,
    has_dependencies: bool,
) -> Result<(), CtlError> {
    if command_line != spec.command_line()?
        || account_name
            .is_none_or(|a| canonical_account(a) != canonical_account(&spec.account_name))
        || !own_process
        || !demand_start
        || has_dependencies
    {
        return Err(bad(
            "SCM ownership differs from pinned node binary, argv, account or demand-start contract",
        ));
    }
    Ok(())
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::time::{Duration, Instant};
    use windows_service::service::{
        Service, ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    fn failure(error: windows_service::Error) -> CtlError {
        CtlError::IntegrationFailed(format!("SCM operation failed: {error}"))
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
            config.start_type == ServiceStartType::OnDemand,
            !config.dependencies.is_empty(),
        )?;
        let status = service.query_status().map_err(failure)?;
        Ok(
            json!({"service":spec.name,"manager":"windows_scm","state":format!("{:?}",status.current_state),"ownership_checked":true,"health_verified":false,"custody_verified":false}),
        )
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
            manager
                .create_service(
                    &ServiceInfo {
                        name: spec.name.clone().into(),
                        display_name: spec.name.clone().into(),
                        service_type: ServiceType::OWN_PROCESS,
                        start_type: ServiceStartType::OnDemand,
                        error_control: ServiceErrorControl::Normal,
                        executable_path: spec.binary.clone(),
                        launch_arguments: spec.launch_arguments(),
                        dependencies: Vec::new(),
                        account_name: Some(canonical_account(&spec.account_name).into()),
                        account_password: None,
                    },
                    ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS,
                )
                .map_err(failure)?;
            return Ok(
                json!({"service":spec.name,"action":"install","created":true,"started":false,"auto_start":false,"custody_verified":false}),
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
        let s = spec();
        let cmd = s.command_line().unwrap();
        assert!(verify_owned_config(&s, &cmd, Some("LocalService"), true, true, false).is_ok());
        for (command, account, own, demand, deps) in [
            (
                format!("{cmd} --create"),
                Some("LocalService"),
                true,
                true,
                false,
            ),
            (cmd.clone(), Some("NetworkService"), true, true, false),
            (cmd.clone(), None, true, true, false),
            (cmd.clone(), Some("LocalService"), false, true, false),
            (cmd.clone(), Some("LocalService"), true, false, false),
            (cmd.clone(), Some("LocalService"), true, true, true),
        ] {
            assert!(verify_owned_config(&s, &command, account, own, demand, deps).is_err());
        }
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
