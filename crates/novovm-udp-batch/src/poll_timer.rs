//! Process-scoped Windows timeout precision for the original native poll owner.
//! This changes neither its 1 ms watchdog nor park/unpark completion semantics.
use std::io;

#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(period: u32) -> u32;
    fn timeEndPeriod(period: u32) -> u32;
}

/// One successful request, paired on normal return and error unwinding. Windows
/// may still delay scheduling; this is not a real-time or exact-deadline promise.
pub struct NativePollTimer {
    period: u32,
}

impl NativePollTimer {
    pub fn new() -> io::Result<Self> {
        // SAFETY: WinMM's process-level API takes no pointer or borrowed state.
        let code = unsafe { timeBeginPeriod(1) };
        if code != 0 {
            return Err(io::Error::other(format!(
                "native 1 ms timer precision request: {code}"
            )));
        }
        Ok(Self { period: 1 })
    }
}

impl Drop for NativePollTimer {
    fn drop(&mut self) {
        // SAFETY: this owner exists only after a successful matching request.
        // It is not Clone; every owner releases exactly its own request.
        unsafe { timeEndPeriod(self.period) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn precision_scope_retains_unpark_token_with_nested_owners() -> io::Result<()> {
        let outer = NativePollTimer::new()?;
        let inner = NativePollTimer::new()?;
        drop(inner);
        thread::current().unpark();
        let start = Instant::now();
        thread::park_timeout(Duration::from_secs(5));
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "original unpark token lost"
        );
        drop(outer);
        Ok(())
    }
}
