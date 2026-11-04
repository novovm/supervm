//! SCM owns process lifetime only. The original native tick owns its signer,
//! network, execution, database and orderly drain.
use anyhow::{ensure, Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;
#[cfg(any(windows, test))]
use std::sync::mpsc::{self, Receiver};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender,
    Arc,
};

pub const TICK_STACK_BYTES: usize = 16 * 1024 * 1024;
pub const SERVICE_CONTRACT: &str = "windows-scm-existing-v1";

/// Offline query of this executable's service entry. No config, environment,
/// runtime, custody, database or SCM operation is consulted.
pub fn contract_query(args: &[OsString]) -> Result<Option<serde_json::Value>> {
    if !args.iter().any(|arg| arg == "--service-contract") {
        return Ok(None);
    }
    ensure!(args.len() == 1, "usage: --service-contract");
    Ok(Some(serde_json::json!({
        "schema_version": 1,
        "contract": SERVICE_CONTRACT,
        "platform": std::env::consts::OS,
        "supported": cfg!(windows),
        "entry": "native_execution_tick",
        "start_mode": "existing",
        "service_started": false,
        "database_opened": false,
        "custody_accessed": false
    })))
}

#[derive(Clone, Debug)]
pub struct Request {
    pub config: PathBuf,
    pub name: String,
}

impl Request {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>> {
        let args: Vec<_> = args.into_iter().collect();
        if args.first().is_none_or(|arg| arg != "--service") {
            ensure!(
                !args
                    .iter()
                    .any(|arg| arg == "--service" || arg == "--service-name"),
                "service selection must be --service CONFIG --service-name NAME"
            );
            return Ok(None);
        }
        ensure!(
            args.len() == 4 && args[2] == "--service-name",
            "usage: --service CONFIG --service-name NAME"
        );
        let config = PathBuf::from(&args[1]);
        let name = args[3].to_str().context("service name must be ASCII")?;
        ensure!(
            config.is_absolute(),
            "absolute service configuration required"
        );
        ensure!(
            !name.is_empty()
                && name.len() <= 128
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')),
            "invalid native SCM service name"
        );
        Ok(Some(Self {
            config,
            name: name.into(),
        }))
    }

    /// Establish the existing native entry, never fresh-state initialization.
    /// Configuration validation happens before the tick opens its owners.
    pub fn prepare_existing_tick(&self) -> Result<()> {
        use crate::native_pipeline::service::{
            config::ORIGINAL_PROFILE, NodeKeyStorage, ResidentConfig,
        };
        let config = ResidentConfig::load(&self.config)?;
        ensure!(
            config.profile == ORIGINAL_PROFILE
                && config.rpc_addr.is_none()
                && config.key_storage == NodeKeyStorage::ProtectedV1
                && config.uca_initialization.is_some(),
            "managed native service requires original profile, protected asset initialization and no HTTP"
        );
        for (name, value) in [
            ("NOVOVM_NODE_MODE", OsString::from(ORIGINAL_PROFILE)),
            (
                "NOVOVM_NATIVE_EXECUTION_CONFIG",
                self.config.as_os_str().to_owned(),
            ),
            ("NOVOVM_NATIVE_EXECUTION_START", OsString::from("existing")),
        ] {
            ensure!(
                std::env::var_os(name).is_none_or(|current| current == value),
                "managed service conflicts with {name}"
            );
        }
        for name in [
            "NOVOVM_NATIVE_EXECUTION_RUN_MS",
            "NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS",
        ] {
            ensure!(
                std::env::var_os(name).is_none(),
                "managed service cannot use {name}"
            );
        }
        // Only these original bootstrap selectors are established. Backend,
        // policy, pins and runtime library still come from the validated config.
        std::env::set_var("NOVOVM_NODE_MODE", ORIGINAL_PROFILE);
        std::env::set_var("NOVOVM_NATIVE_EXECUTION_CONFIG", &self.config);
        std::env::set_var("NOVOVM_NATIVE_EXECUTION_START", "existing");
        Ok(())
    }
}

enum Event {
    Ready,
    #[cfg(windows)]
    Exited(Result<()>),
}

#[derive(Clone)]
pub struct Control {
    stop: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    events: SyncSender<Event>,
}

impl Control {
    #[cfg(any(windows, test))]
    fn channel() -> (Self, Receiver<Event>) {
        let (events, receiver) = mpsc::sync_channel(2);
        (
            Self {
                stop: Arc::new(AtomicBool::new(false)),
                ready: Arc::new(AtomicBool::new(false)),
                events,
            },
            receiver,
        )
    }

    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    pub fn ready(&self) -> Result<()> {
        if !self.stop_requested() && !self.ready.swap(true, Ordering::AcqRel) {
            self.events
                .send(Event::Ready)
                .context("SCM lifecycle receiver closed")?;
        }
        Ok(())
    }
}

pub type Tick = fn(Request, Control) -> Result<()>;

pub fn dispatch(request: Request, tick: Tick) -> Result<()> {
    #[cfg(windows)]
    {
        windows::dispatch(request, tick)
    }
    #[cfg(not(windows))]
    {
        let _ = (request, tick);
        anyhow::bail!("native SCM service requires Windows")
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::sync::OnceLock;
    use std::time::Duration;
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    static DEFINITION: OnceLock<(Request, Tick)> = OnceLock::new();
    static OUTCOME: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    define_windows_service!(service_entry, service_main);

    pub(super) fn dispatch(request: Request, tick: Tick) -> Result<()> {
        let name = request.name.clone();
        DEFINITION
            .set((request, tick))
            .map_err(|_| anyhow::anyhow!("SCM dispatch already selected"))?;
        service_dispatcher::start(name, service_entry).context("connect original node to SCM")?;
        match OUTCOME.get().context("SCM callback did not complete")? {
            Ok(()) => Ok(()),
            Err(error) => anyhow::bail!("{error}"),
        }
    }

    fn service_main(_: Vec<OsString>) {
        // Preserve both SCM's service-specific failure and the executable's
        // non-success exit after startup/poll/drain failure.
        let _ = OUTCOME.set(run().map_err(|error| format!("{error:#}")));
    }

    fn run() -> Result<()> {
        let (request, tick) = DEFINITION.get().context("SCM definition missing")?;
        let (control, events) = Control::channel();
        let signal = control.clone();
        let handle =
            service_control_handler::register(&request.name, move |command| match command {
                ServiceControl::Stop | ServiceControl::Shutdown => {
                    signal.stop.store(true, Ordering::Release);
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
        let request = request.clone();
        let tick = *tick;
        let worker_control = control.clone();
        let worker = std::thread::Builder::new()
            .name("novovm-native-tick".into())
            .stack_size(TICK_STACK_BYTES)
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    tick(request, worker_control.clone())
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("original native tick panicked")));
                let _ = worker_control.events.send(Event::Exited(result));
            });
        let result = match worker {
            Err(error) => Err(error.into()),
            Ok(worker) => {
                let result = loop {
                    match events.recv_timeout(Duration::from_secs(2)) {
                        Ok(Event::Ready) if !control.stop_requested() => {
                            status.current_state = ServiceState::Running;
                            status.controls_accepted =
                                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN;
                            status.checkpoint = 0;
                            status.wait_hint = Duration::ZERO;
                            // Process readiness is NOT chain finality, receipts
                            // or whole-stack/asset authorization.
                            if let Err(error) = handle.set_service_status(status.clone()) {
                                control.stop.store(true, Ordering::Release);
                                break Err(error.into());
                            }
                        }
                        Ok(Event::Ready) => {}
                        Ok(Event::Exited(result)) => {
                            break result.and_then(|()| {
                                ensure!(
                                    control.stop_requested(),
                                    "native tick exited without an SCM stop"
                                );
                                Ok(())
                            });
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            break Err(anyhow::anyhow!("native tick lost its lifecycle sender"))
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if control.stop_requested() {
                                status.current_state = ServiceState::StopPending;
                                status.controls_accepted = ServiceControlAccept::empty();
                            }
                            if matches!(
                                status.current_state,
                                ServiceState::StartPending | ServiceState::StopPending
                            ) {
                                status.checkpoint = status.checkpoint.saturating_add(1);
                                status.wait_hint = Duration::from_secs(10);
                                if let Err(error) = handle.set_service_status(status.clone()) {
                                    control.stop.store(true, Ordering::Release);
                                    break Err(error.into());
                                }
                            }
                        }
                    }
                };
                let joined = worker
                    .join()
                    .map_err(|_| anyhow::anyhow!("original native tick panicked"));
                result.and(joined)
            }
        };
        status.current_state = ServiceState::Stopped;
        status.controls_accepted = ServiceControlAccept::empty();
        status.checkpoint = 0;
        status.wait_hint = Duration::ZERO;
        status.exit_code = if result.is_ok() {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::ServiceSpecific(1)
        };
        handle.set_service_status(status)?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_query_is_explicit_offline_and_platform_qualified() {
        let args = [OsString::from("--service-contract")];
        let report = contract_query(&args).unwrap().unwrap();
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["contract"], SERVICE_CONTRACT);
        assert_eq!(report["platform"], std::env::consts::OS);
        assert_eq!(report["supported"], cfg!(windows));
        assert_eq!(report["entry"], "native_execution_tick");
        assert_eq!(report["start_mode"], "existing");
        for key in ["service_started", "database_opened", "custody_accessed"] {
            assert_eq!(report[key], false);
        }
        assert!(contract_query(&[]).unwrap().is_none());
        assert!(contract_query(&[args[0].clone(), OsString::from("extra")]).is_err());
        assert!(contract_query(&[OsString::from("--service"), args[0].clone()]).is_err());
    }

    #[test]
    fn service_request_is_explicit_and_has_no_implicit_name_or_relative_config() {
        let config = std::env::temp_dir().join("native config.json");
        let args = [
            OsString::from("--service"),
            config.into_os_string(),
            OsString::from("--service-name"),
            OsString::from("NOVOVM-Node-A"),
        ];
        let parsed = Request::parse(args.clone()).unwrap().unwrap();
        assert_eq!(parsed.name, "NOVOVM-Node-A");
        assert!(Request::parse(args[..3].iter().cloned()).is_err());
        let mut changed = args.clone();
        changed[1] = "relative.json".into();
        assert!(Request::parse(changed).is_err());
        let mut changed = args;
        changed[3] = "name / unsafe".into();
        assert!(Request::parse(changed).is_err());
    }

    #[test]
    fn nonservice_entry_is_unchanged_and_misplaced_service_flags_reject() {
        assert!(Request::parse([OsString::from("--mainline-evm-host")])
            .unwrap()
            .is_none());
        assert!(
            Request::parse([OsString::from("--unknown"), OsString::from("--service")]).is_err()
        );
        let request = Request::parse([
            OsString::from("--service"),
            std::env::temp_dir().join("node.json").into_os_string(),
            OsString::from("--service-name"),
            OsString::from("NOVOVM.Node-A"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(request.name, "NOVOVM.Node-A");
    }

    #[test]
    fn readiness_is_single_and_stop_does_not_revive_a_pending_owner() {
        let (control, events) = Control::channel();
        control.ready().unwrap();
        control.ready().unwrap();
        assert!(matches!(events.try_recv(), Ok(Event::Ready)));
        assert!(events.try_recv().is_err());
        control.stop.store(true, Ordering::Release);
        assert!(control.clone().stop_requested());
        control.ready().unwrap();
        assert!(events.try_recv().is_err());

        let (pending, events) = Control::channel();
        pending.stop.store(true, Ordering::Release);
        pending.ready().unwrap();
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn lost_scm_receiver_is_not_reported_as_readiness() {
        let (control, events) = Control::channel();
        drop(events);
        assert!(control.ready().is_err());
    }
}
