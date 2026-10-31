//! On-demand process CPU accounting; no sampling thread or scheduler changes.
use std::io;

/// Cumulative CPU time across all threads of one process, not elapsed wall time.
/// Nanosecond units do not imply nanosecond accounting precision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessCpuTime {
    pub user_ns: u64,
    pub kernel_ns: u64,
}

impl ProcessCpuTime {
    /// Difference between snapshots of the same process instance. A decreasing
    /// component is an error to the caller, not a zero-duration measurement.
    pub fn checked_sub(self, earlier: Self) -> Option<Self> {
        Some(Self {
            user_ns: self.user_ns.checked_sub(earlier.user_ns)?,
            kernel_ns: self.kernel_ns.checked_sub(earlier.kernel_ns)?,
        })
    }

    pub fn checked_total_ns(self) -> Option<u64> {
        self.user_ns.checked_add(self.kernel_ns)
    }
}

/// Read a local process's cumulative user/kernel CPU accounting on demand.
///
/// The caller must keep track of the process instance: reopening a PID does not
/// protect two snapshots against PID reuse after the original process exits.
/// Windows accounting is quantized and is unsuitable for timing individual
/// micro-packets. Other platforms return [`io::ErrorKind::Unsupported`].
pub fn read_process_cpu_time(pid: u32) -> io::Result<ProcessCpuTime> {
    #[cfg(windows)]
    {
        windows::read(pid)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "process CPU accounting is only implemented on Windows",
        ))
    }
}

#[cfg(windows)]
mod windows {
    use super::ProcessCpuTime;
    use std::{ffi::c_void, io};

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[derive(Default)]
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        fn nanoseconds(self) -> io::Result<u64> {
            let ticks = (u64::from(self.high) << 32) | u64::from(self.low);
            ticks.checked_mul(100).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "process CPU time exceeds the nanosecond counter range",
                )
            })
        }
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit_handle: i32, pid: u32) -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    pub(super) fn read(pid: u32) -> io::Result<ProcessCpuTime> {
        // SAFETY: scalar arguments request a non-inheritable query-only handle.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut creation = FileTime::default();
        let mut exit = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        // SAFETY: OpenProcess returned a live handle owned only by this call;
        // all four out-pointers address distinct initialized FILETIME layouts.
        let read_ok =
            unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
        let read_error = (read_ok == 0).then(io::Error::last_os_error);
        // SAFETY: release the real owned handle exactly once, including when
        // GetProcessTimes fails. No fallible conversion occurs before release.
        let close_ok = unsafe { CloseHandle(handle) };
        if let Some(error) = read_error {
            return Err(error);
        }
        if close_ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ProcessCpuTime {
            user_ns: user.nanoseconds()?,
            kernel_ns: kernel.nanoseconds()?,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn filetime_conversion_preserves_high_word_and_checks_overflow() -> io::Result<()> {
            assert_eq!(FileTime { low: 1, high: 0 }.nanoseconds()?, 100);
            assert_eq!(
                FileTime { low: 7, high: 1 }.nanoseconds()?,
                ((1_u64 << 32) + 7) * 100
            );
            assert_eq!(
                FileTime {
                    low: u32::MAX,
                    high: u32::MAX,
                }
                .nanoseconds()
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidData
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_difference_rejects_either_decreasing_component() {
        let earlier = ProcessCpuTime {
            user_ns: 100,
            kernel_ns: 200,
        };
        assert_eq!(
            earlier.checked_sub(earlier),
            Some(ProcessCpuTime::default())
        );
        assert_eq!(
            ProcessCpuTime {
                user_ns: 130,
                kernel_ns: 240,
            }
            .checked_sub(earlier),
            Some(ProcessCpuTime {
                user_ns: 30,
                kernel_ns: 40,
            })
        );
        assert!(ProcessCpuTime {
            user_ns: 99,
            kernel_ns: 240,
        }
        .checked_sub(earlier)
        .is_none());
        assert!(ProcessCpuTime {
            user_ns: 130,
            kernel_ns: 199,
        }
        .checked_sub(earlier)
        .is_none());
    }

    #[test]
    fn checked_total_rejects_overflow() {
        assert_eq!(
            ProcessCpuTime {
                user_ns: 30,
                kernel_ns: 40,
            }
            .checked_total_ns(),
            Some(70)
        );
        assert_eq!(
            ProcessCpuTime {
                user_ns: u64::MAX,
                kernel_ns: 1,
            }
            .checked_total_ns(),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn current_process_cpu_time_is_nondecreasing() -> io::Result<()> {
        let mut previous = read_process_cpu_time(std::process::id())?;
        for round in 0..8_u64 {
            let mut work = round;
            for value in 0..65_536_u64 {
                work = std::hint::black_box(work.wrapping_add(value));
            }
            std::hint::black_box(work);
            let current = read_process_cpu_time(std::process::id())?;
            assert!(current.checked_sub(previous).is_some());
            previous = current;
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn invalid_process_id_returns_os_error() {
        // PID 0 is the idle process, which OpenProcess explicitly rejects.
        assert_eq!(
            read_process_cpu_time(0).unwrap_err().raw_os_error(),
            Some(87)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_does_not_fabricate_zero_cpu_time() {
        assert_eq!(
            read_process_cpu_time(std::process::id())
                .unwrap_err()
                .kind(),
            io::ErrorKind::Unsupported
        );
    }
}
