//! Opt-in slow-call diagnostics, not throughput or finality measurements.
//! Phases must be fixed literals, never request, signature or account data.
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static ENABLED: OnceLock<bool> = OnceLock::new();
static ORIGIN: OnceLock<Instant> = OnceLock::new();
const SLOW_CALL: Duration = Duration::from_millis(10);

/// Isolated handoff diagnosis only. Public object/validator identities, never
/// payloads or signatures; a hard process cap also bounds failure output.
pub(crate) fn handoff_trace(phase: &'static str, details: impl FnOnce() -> serde_json::Value) {
    static TRACE: OnceLock<bool> = OnceLock::new();
    static COUNTS: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
    if !*TRACE.get_or_init(|| {
        std::env::var("NOVOVM_NATIVE_FRESH_HANDOFF_TRACE").is_ok_and(|value| value == "1")
    }) {
        return;
    }
    // Stale archive rejects dominated the observed stream. They must not use
    // the body-completion/vote allowance before the next height starts.
    let counter = match phase {
        "lifecycle.receive" => 0,
        "body.queue" => 1,
        "parent.reject" => 2,
        "body.reject" => 3,
        "body.complete" => 4,
        "seal.reject" => 5,
        "seal.accept" => 6,
        "seal.local_vote" => 7,
        _ => return,
    };
    let Ok(index) = COUNTS[counter].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
        (count < 64).then_some(count + 1)
    }) else {
        return;
    };
    let now = Instant::now();
    let line = serde_json::json!({
        "pid": std::process::id(), "phase": phase, "sample": index + 1,
        "unix_us": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|elapsed| elapsed.as_micros()),
        "since_start_us": now.duration_since(*ORIGIN.get_or_init(|| now)).as_micros(),
        "details": details(),
    });
    let _ = writeln!(std::io::stdout().lock(), "native_fresh_handoff: {line}");
}

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
    fn fresh_handoff_trace_bounds_each_phase_and_skips_disabled_payloads() {
        let enabled =
            std::env::var("NOVOVM_NATIVE_FRESH_HANDOFF_TRACE").is_ok_and(|value| value == "1");
        for phase in [
            "lifecycle.receive",
            "body.queue",
            "parent.reject",
            "body.reject",
            "body.complete",
            "seal.reject",
            "seal.accept",
            "seal.local_vote",
        ] {
            let evaluated = Cell::new(0);
            for _ in 0..66 {
                handoff_trace(phase, || {
                    evaluated.set(evaluated.get() + 1);
                    serde_json::json!({"fixture":"public diagnostic only"})
                });
            }
            assert_eq!(evaluated.get(), if enabled { 64 } else { 0 }, "{phase}");
        }
    }

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
