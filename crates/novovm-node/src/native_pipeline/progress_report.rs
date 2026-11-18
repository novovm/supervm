//! One bounded writer for the original node's process-local reports.
use super::service::rpc::{observe_tick, TickStage};
use anyhow::{Context, Result};
use serde::Serialize;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

type Work = Box<dyn FnOnce() + Send>;

pub struct ProgressReporter {
    queue: mpsc::SyncSender<Work>,
    busy: Arc<AtomicBool>,
}

#[derive(Serialize)]
pub struct ProgressSnapshot<S, N> {
    #[serde(flatten)]
    status: S,
    #[serde(skip_serializing_if = "Option::is_none")]
    novonet: Option<N>,
    entry: &'static str,
    ticks: u64,
}

impl<S, N> ProgressSnapshot<S, N> {
    pub fn new(status: S, novonet: Option<N>, ticks: u64) -> Self {
        Self {
            status,
            novonet,
            entry: "original_native_execution_tick",
            ticks,
        }
    }
}

impl ProgressReporter {
    pub fn new() -> Result<Self> {
        let (queue, work) = mpsc::sync_channel::<Work>(1);
        std::thread::Builder::new()
            .name("novovm-progress-report".into())
            .spawn(move || {
                while let Ok(run) = work.recv() {
                    run();
                }
            })
            .context("start native progress writer")?;
        Ok(Self {
            queue,
            busy: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn available(&self) -> bool {
        !self.busy.load(Ordering::Acquire)
    }

    pub fn submit(
        &self,
        value: impl Serialize + Send + 'static,
        path: Option<PathBuf>,
        verbose: bool,
    ) {
        if self.busy.swap(true, Ordering::AcqRel) {
            return;
        }
        let busy = Arc::clone(&self.busy);
        let submitted = self.queue.try_send(Box::new(move || {
            let result = write_checked(value, path, verbose);
            busy.store(false, Ordering::Release);
            if let Err(error) = result {
                let _ = writeln!(
                    io::stderr().lock(),
                    "native progress report unavailable: {error:#}"
                );
            }
        }));
        if submitted.is_err() {
            self.busy.store(false, Ordering::Release);
        }
    }

    /// Only used after the original owners are drained. Never join a stuck writer.
    pub fn finish(
        self,
        value: impl Serialize + Send + 'static,
        path: Option<PathBuf>,
    ) -> Result<()> {
        let (done, result) = mpsc::sync_channel(1);
        self.queue
            .try_send(Box::new(move || {
                let _ = done.send(write_checked(value, path, true));
            }))
            .map_err(|_| anyhow::anyhow!("native summary writer unavailable"))?;
        result
            .recv_timeout(Duration::from_secs(2))
            .context("native summary writer deadline")?
    }
}

fn write_checked(value: impl Serialize, path: Option<PathBuf>, verbose: bool) -> Result<()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
        if let Some(path) = path {
            let encoded = observe_tick(TickStage::Encode, || serde_json::to_vec_pretty(&value))?;
            observe_tick(TickStage::Write, || -> Result<()> {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, encoded)?;
                Ok(())
            })?;
        }
        if verbose {
            let encoded = observe_tick(TickStage::Encode, || serde_json::to_vec(&value))?;
            observe_tick(TickStage::Output, || -> Result<()> {
                let mut stdout = io::stdout().lock();
                stdout.write_all(&encoded)?;
                stdout.write_all(b"\n")?;
                Ok(())
            })?;
        }
        Ok(())
    }))
    .map_err(|_| anyhow::anyhow!("native report formatter panicked"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::report::record;

    fn wait_available(reporter: &ProgressReporter) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !reporter.available() {
            assert!(
                std::time::Instant::now() < deadline,
                "writer did not complete"
            );
            std::thread::yield_now();
        }
    }

    struct Blocked {
        entered: mpsc::Sender<()>,
        gate: std::sync::Mutex<mpsc::Receiver<()>>,
    }

    impl Serialize for Blocked {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.entered.send(()).unwrap();
            self.gate
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            serializer.serialize_bool(false)
        }
    }

    struct Refused;
    impl Serialize for Refused {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("test report formatting failure"))
        }
    }

    #[test]
    fn native_progress_report_blocked_formatter_does_not_backpressure_or_grow() {
        let reporter = ProgressReporter::new().unwrap();
        let (entered, seen) = mpsc::channel();
        let (resume, gate) = mpsc::channel();
        let path =
            std::env::temp_dir().join(format!("novovm-progress-{}.json", rand::random::<u64>()));
        reporter.submit(
            Blocked {
                entered,
                gate: std::sync::Mutex::new(gate),
            },
            Some(path.clone()),
            false,
        );
        seen.recv_timeout(Duration::from_secs(5)).unwrap();
        for _ in 0..1024 {
            // No receiver may block the original tick or retain a backlog.
            assert!(!reporter.available());
            reporter.submit(true, Some(path.clone()), false);
        }
        resume.send(()).unwrap();
        wait_available(&reporter);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "false");

        let report = ProgressSnapshot::new(
            record! { archive_work: record! { reads_started: 17, verified_material_reused: 3 } },
            Some(record! { default: "native_novorudp" }),
            99,
        );
        reporter.submit(report, Some(path.clone()), false);
        wait_available(&reporter);
        let output: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            output,
            serde_json::json!({
                "archive_work":{"reads_started":17,"verified_material_reused":3},
                "novonet":{"default":"native_novorudp"},"entry":"original_native_execution_tick","ticks":99
            })
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn native_progress_report_errors_release_the_slot() {
        let reporter = ProgressReporter::new().unwrap();
        let path =
            std::env::temp_dir().join(format!("novovm-progress-{}.json", rand::random::<u64>()));
        std::fs::write(&path, b"old report").unwrap();
        reporter.submit(Refused, Some(path.clone()), false);
        wait_available(&reporter);
        assert_eq!(std::fs::read(&path).unwrap(), b"old report");
        reporter.submit(1, Some(path.join("invalid-child")), false);
        wait_available(&reporter);
        reporter.submit(2, Some(path.clone()), false);
        wait_available(&reporter);
        assert_eq!(std::fs::read(&path).unwrap(), b"2");
        std::fs::remove_file(path).unwrap();
    }
}
