//! Independently provisioned publisher entry. Uses the existing manifest format,
//! protected Windows keys and durable issuance journal; never a node self-grant.
#[cfg(windows)]
mod bootstrap_publisher_owner;
#[cfg(windows)]
mod windows_custody;
#[cfg(windows)]
mod windows_owner_files;

#[cfg(windows)]
mod windows {
    use super::bootstrap_publisher_owner::{Config, Owner};
    use anyhow::{ensure, Context, Result};
    use std::{
        ffi::OsString,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, OnceLock,
        },
        time::Duration,
    };
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };
    pub const NAME: &str = "NOVOVMBootstrapPublisher";
    static CONFIG: OnceLock<PathBuf> = OnceLock::new();
    define_windows_service!(service_entry, service_main);

    fn execute(
        config: &std::path::Path,
        stop: Arc<AtomicBool>,
        seconds: Option<u64>,
    ) -> Result<()> {
        Owner::run(
            Config::load(config)?,
            stop,
            seconds.map(Duration::from_secs),
        )
    }
    fn service_main(_: Vec<OsString>) {
        let _ = run_service();
    }
    fn run_service() -> Result<()> {
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let handle = service_control_handler::register(NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                signal.store(true, Ordering::Release);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
        let mut status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 1,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        };
        handle.set_service_status(status.clone())?;
        status.current_state = ServiceState::Running;
        status.controls_accepted = ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN;
        status.checkpoint = 0;
        status.wait_hint = Duration::ZERO;
        handle.set_service_status(status.clone())?;
        // SCM Running is process health, not node/network/Privacy acceptance.
        let result = execute(CONFIG.get().context("service configuration")?, stop, None);
        status.current_state = ServiceState::Stopped;
        status.controls_accepted = ServiceControlAccept::empty();
        status.exit_code = if result.is_ok() {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::ServiceSpecific(1)
        };
        handle.set_service_status(status.clone())?;
        // No key, address, client identifier, error chain or ciphertext logging.
        result
    }
    pub fn main() -> Result<()> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.len() == 1 && args[0] == "--configure-recovery" {
            use windows_service::service::{
                ServiceAccess, ServiceAction, ServiceActionType, ServiceFailureActions,
                ServiceFailureResetPeriod,
            };
            use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
            let manager =
                ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
            let service =
                manager.open_service(NAME, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)?;
            service.update_failure_actions(ServiceFailureActions {
                reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86400)),
                reboot_msg: None,
                command: None,
                actions: Some(vec![
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(15),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(60),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::None,
                        delay: Duration::ZERO,
                    },
                ]),
            })?;
            service.set_failure_actions_on_non_crash_failures(false)?;
            return Ok(());
        }
        ensure!(
            args.len() == 2 || args.len() == 3,
            "usage: --initialize STATE | --enroll-policy CONFIG | --revoke CONFIG | --check CONFIG | --service CONFIG | --foreground CONFIG SECONDS"
        );
        let path = PathBuf::from(&args[1]);
        ensure!(path.is_absolute(), "absolute configuration path required");
        if args[0] == "--initialize" {
            ensure!(args.len() == 2, "initialize arguments");
            Owner::initialize(&path)?;
        } else if args[0] == "--enroll-policy" {
            ensure!(args.len() == 2, "enroll arguments");
            Owner::enroll(Config::load(&path)?)?;
        } else if args[0] == "--revoke" {
            ensure!(args.len() == 2, "revoke arguments");
            Owner::request_revoke(&Config::load(&path)?)?;
            println!("Revocation requested; verify publisher status and node shutdown.");
        } else if args[0] == "--check" {
            ensure!(args.len() == 2, "check arguments");
            Config::load(&path)?;
        } else if args[0] == "--service" {
            ensure!(args.len() == 2, "service arguments");
            CONFIG
                .set(path)
                .map_err(|_| anyhow::anyhow!("service already initialized"))?;
            service_dispatcher::start(NAME, service_entry)?;
        } else {
            ensure!(
                args[0] == "--foreground" && args.len() == 3,
                "explicit foreground duration required"
            );
            let seconds: u64 = args[2].to_str().context("duration")?.parse()?;
            ensure!((1..=86400).contains(&seconds), "foreground duration bound");
            execute(&path, Arc::new(AtomicBool::new(false)), Some(seconds))?;
        }
        Ok(())
    }
}
fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::main()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows platform custody required")
    }
}
