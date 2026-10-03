//! Opt-in slow-call diagnostics, not throughput or finality measurements.
//! Phases must be fixed literals, never request, signature or account data.
use std::io::Write;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static ENABLED: OnceLock<bool> = OnceLock::new();
static ORIGIN: OnceLock<Instant> = OnceLock::new();
const SLOW_CALL: Duration = Duration::from_millis(10);

fn enabled() -> bool {
    *ENABLED
        .get_or_init(|| std::env::var("NOVOVM_NATIVE_FRESH_TIMING").is_ok_and(|value| value == "1"))
}

/// No clock read when disabled. The opt-in setting is fixed on first use.
/// Drop records slow calls even on an early error return or stack unwind.
pub struct Span {
    phase: &'static str,
    started: Option<Instant>,
}

impl Span {
    pub fn start(phase: &'static str) -> Self {
        Self::with_enabled(phase, enabled())
    }

    fn with_enabled(phase: &'static str, enabled: bool) -> Self {
        let started = enabled.then(|| {
            let now = Instant::now();
            ORIGIN.get_or_init(|| now);
            now
        });
        Self { phase, started }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some(started) = self.started else {
            return;
        };
        let now = Instant::now();
        let elapsed = now.duration_since(started);
        if elapsed < SLOW_CALL {
            return;
        }
        let line = serde_json::json!({
            "pid": std::process::id(),
            "phase": self.phase,
            "elapsed_us": elapsed.as_micros(),
            "since_start_us": now.duration_since(*ORIGIN.get().unwrap_or(&started)).as_micros(),
        });
        // Diagnostic output failure must not replace a business result/error.
        let _ = writeln!(std::io::stdout().lock(), "native_fresh_timing: {line}");
    }
}

pub fn measure<T>(phase: &'static str, action: impl FnOnce() -> T) -> T {
    measure_enabled(phase, enabled(), action)
}

fn measure_enabled<T>(phase: &'static str, enabled: bool, action: impl FnOnce() -> T) -> T {
    let _span = Span::with_enabled(phase, enabled);
    action()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn fresh_timing_disabled_runs_once_and_preserves_value() {
        let calls = Cell::new(0);
        let original = vec![1, 2, 3];
        let pointer = original.as_ptr();
        let result = measure_enabled("test.value", false, || {
            calls.set(calls.get() + 1);
            original
        });
        assert_eq!(calls.get(), 1);
        assert_eq!(result, [1, 2, 3]);
        assert_eq!(result.as_ptr(), pointer);
        assert!(Span::with_enabled("test.disabled", false).started.is_none());
    }

    #[test]
    fn fresh_timing_disabled_runs_once_and_preserves_error() {
        let calls = Cell::new(0);
        let original = String::from("original error");
        let pointer = original.as_ptr();
        let result = measure_enabled("test.error", false, || {
            calls.set(calls.get() + 1);
            Err::<(), _>(original)
        });
        let error = result.unwrap_err();
        assert_eq!(calls.get(), 1);
        assert_eq!(error, "original error");
        assert_eq!(error.as_ptr(), pointer);
    }
}
