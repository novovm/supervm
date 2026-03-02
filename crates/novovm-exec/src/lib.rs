use anyhow::Result;
use aoem_bindings::{AoemCreateOptionsV1, AoemDyn, AoemExecV2Result, AoemHandle, AoemOpV2};
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub struct AoemExecOpenOptions {
    pub ingress_workers: Option<u32>,
}

pub struct AoemExecFacade {
    dynlib: AoemDyn,
    options: AoemExecOpenOptions,
}

pub struct AoemExecSession<'a> {
    handle: AoemHandle<'a>,
}

#[derive(Clone, Debug, Default)]
pub struct AoemExecMetrics {
    pub elapsed_us: u64,
    pub submitted_ops: u32,
    pub processed_ops: u32,
    pub success_ops: u32,
    pub total_writes: u64,
    pub failed_index: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct AoemExecOutput {
    pub result: AoemExecV2Result,
    pub metrics: AoemExecMetrics,
}

impl AoemExecFacade {
    /// Loads AOEM FFI DLL and validates startup contract (ABI + manifest + capabilities).
    pub fn open(dll_path: impl AsRef<Path>, options: AoemExecOpenOptions) -> Result<Self> {
        let dynlib = unsafe { AoemDyn::load(dll_path.as_ref()) }?;
        Ok(Self { dynlib, options })
    }

    pub fn abi(&self) -> u32 {
        self.dynlib.abi()
    }

    pub fn version(&self) -> String {
        self.dynlib.version()
    }

    pub fn capabilities_json(&self) -> Result<serde_json::Value> {
        self.dynlib.capabilities()
    }

    /// Creates one execution session. Host can keep one session per worker thread.
    pub fn create_session(&self) -> Result<AoemExecSession<'_>> {
        let handle = self
            .dynlib
            .create_handle_with_ingress_workers(self.options.ingress_workers)?;
        Ok(AoemExecSession { handle })
    }
}

impl<'a> AoemExecSession<'a> {
    pub fn execute_ops_v2(&self, ops: &[AoemOpV2]) -> Result<AoemExecV2Result> {
        self.handle.execute_ops_v2(ops)
    }

    /// Host main-path stable entry: execute typed ops and return result+metrics in one object.
    pub fn submit_ops(&self, ops: &[AoemOpV2]) -> Result<AoemExecOutput> {
        let t0 = Instant::now();
        let result = self.execute_ops_v2(ops)?;
        let elapsed_us = t0.elapsed().as_micros() as u64;
        let metrics = AoemExecMetrics {
            elapsed_us,
            submitted_ops: ops.len() as u32,
            processed_ops: result.processed,
            success_ops: result.success,
            total_writes: result.total_writes,
            failed_index: if result.failed_index == u32::MAX {
                None
            } else {
                Some(result.failed_index)
            },
        };
        Ok(AoemExecOutput { result, metrics })
    }
}

/// Re-export AOEM V2 op/result types for host integration.
pub use aoem_bindings::AoemExecV2Result as ExecResultV2;
pub use aoem_bindings::AoemOpV2 as ExecOpV2;
pub use aoem_bindings::AoemHostAdaptiveDecision;
pub use aoem_bindings::AoemHostHint;
pub use aoem_bindings::acquire_global_lane;
pub use aoem_bindings::global_parallel_budget;
pub use aoem_bindings::recommend_threads_auto;
pub use aoem_bindings::recommend_threads_from_aoem;
pub use aoem_bindings::set_global_parallel_budget;

#[allow(dead_code)]
fn _assert_abi_struct_layout(_v: AoemCreateOptionsV1) {}
