//! Windows SCM host for the existing NOVOVM rendezvous runtime. Foreground mode
//! uses the same owner for bounded, independent-process acceptance tests.
// Direct Windows task/service execution must not open an interactive console.
// Redirected CLI handles continue to carry process-test diagnostics.
#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod rendezvous_owner;
#[cfg(windows)]
mod rendezvous_self_owned_owner;
#[cfg(windows)]
mod windows_custody;
#[cfg(windows)]
mod windows_owner_files;

#[cfg(windows)]
mod windows {
    use super::rendezvous_owner::{Config, Owner};
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
    pub const NAME: &str = "NOVOVMRendezvous";
    static CONFIG: OnceLock<PathBuf> = OnceLock::new();
    define_windows_service!(service_entry, service_main);

    fn execute(config: &std::path::Path, stop: Arc<AtomicBool>, epochs: Option<u64>) -> Result<()> {
        let version = serde_json::from_slice::<serde_json::Value>(
            &super::windows_owner_files::read_bounded(config, 65536)?,
        )?["version"]
            .as_u64();
        if version == Some(2) {
            ensure!(
                epochs.is_none(),
                "self-owned service uses local stop, not endpoint epochs"
            );
            return execute_owned(config, stop, false, None);
        }
        let config = Config::load(config)?;
        let result = (|| {
            let owner = Owner::open(config.clone())?;
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(owner.run(stop, epochs))
        })();
        if result.is_err() {
            config.record_start_failure();
        }
        result
    }
    fn execute_owned(
        config: &std::path::Path,
        stop: Arc<AtomicBool>,
        provision: bool,
        legacy: Option<super::rendezvous_owner::Config>,
    ) -> Result<()> {
        let config = super::rendezvous_self_owned_owner::Config::load(config)?;
        let owner = super::rendezvous_self_owned_owner::Owner::open(config, provision, legacy)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(owner.run(stop))
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
        // SCM Running means the owner process is alive, not that its external
        // authorization has been granted. status.json reports that distinction.
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
            "usage: --check CONFIG | --provision CONFIG | --service CONFIG | --foreground CONFIG EPOCHS"
        );
        let path = PathBuf::from(&args[1]);
        ensure!(path.is_absolute(), "absolute configuration path required");
        if args[0] == "--set-self-owned-native" {
            use super::rendezvous_self_owned_owner::{Config, NativeListenerConfig, Owner};
            ensure!(
                args.len() == 3,
                "usage: --set-self-owned-native CONFIG LISTENER_JSON|off"
            );
            let listener = if args[2] == "off" {
                None
            } else {
                let settings = PathBuf::from(&args[2]);
                ensure!(
                    settings.is_absolute(),
                    "absolute listener settings path required"
                );
                Some(NativeListenerConfig::load(&settings)?)
            };
            return Owner::open(Config::load(&path)?, false, None)?.set_native_listener(listener);
        }
        if args[0] == "--set-self-owned-tls" {
            use super::rendezvous_self_owned_owner::{Config, Owner, TlsListenerConfig};
            ensure!(
                args.len() == 3,
                "usage: --set-self-owned-tls CONFIG LISTENER_JSON|off"
            );
            let listener = if args[2] == "off" {
                None
            } else {
                let settings = PathBuf::from(&args[2]);
                ensure!(
                    settings.is_absolute(),
                    "absolute listener settings path required"
                );
                Some(TlsListenerConfig::load(&settings)?)
            };
            return Owner::open(Config::load(&path)?, false, None)?.set_tls_listener(listener);
        }
        if args[0] == "--set-self-owned-carrier" {
            use super::rendezvous_self_owned_owner::{CarrierMode, Config, Owner};
            ensure!(
                args.len() == 3,
                "usage: --set-self-owned-carrier CONFIG automatic|relay-only"
            );
            let mode = match args[2].to_str() {
                Some("automatic") => CarrierMode::Automatic,
                Some("relay-only") => CarrierMode::RelayOnly,
                _ => anyhow::bail!("unknown carrier mode"),
            };
            return Owner::open(Config::load(&path)?, false, None)?.set_carrier_mode(mode);
        }
        if args[0] == "--migrate-self-owned" {
            ensure!(
                args.len() == 3,
                "usage: --migrate-self-owned OLD_CONFIG NEW_CONFIG"
            );
            let old = Config::load(&path)?;
            return execute_owned(
                &PathBuf::from(&args[2]),
                Arc::new(AtomicBool::new(false)),
                true,
                Some(old),
            );
        }
        if args[0] == "--provision-self-owned" {
            ensure!(args.len() == 2, "usage: --provision-self-owned CONFIG");
            return execute_owned(&path, Arc::new(AtomicBool::new(false)), true, None);
        }
        if args[0] == "--foreground" && args.len() == 2 {
            return execute_owned(&path, Arc::new(AtomicBool::new(false)), false, None);
        }
        if args[0] == "--check" {
            ensure!(args.len() == 2, "check arguments");
            let version = serde_json::from_slice::<serde_json::Value>(
                &super::windows_owner_files::read_bounded(&path, 65536)?,
            )?["version"]
                .as_u64();
            if version == Some(2) {
                super::rendezvous_self_owned_owner::Config::load(&path)?;
            } else {
                Config::load(&path)?;
            }
        } else if args[0] == "--provision" {
            ensure!(args.len() == 2, "provision arguments");
            let config = Config::load(&path)?;
            // Creates/restores custody and exports only the public identity.
            // No carrier is opened and no service authority is granted here.
            let _owner = Owner::open(config.clone())?;
            config.status("provisioned", 0)?;
        } else if args[0] == "--service" {
            ensure!(args.len() == 2, "service arguments");
            CONFIG
                .set(path)
                .map_err(|_| anyhow::anyhow!("service already initialized"))?;
            service_dispatcher::start(NAME, service_entry)?;
        } else {
            ensure!(
                args[0] == "--foreground" && args.len() == 3,
                "explicit bounded foreground required"
            );
            let epochs: u64 = args[2].to_str().context("epoch count")?.parse()?;
            ensure!((1..=100).contains(&epochs), "foreground epoch limit");
            execute(&path, Arc::new(AtomicBool::new(false)), Some(epochs))?;
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
