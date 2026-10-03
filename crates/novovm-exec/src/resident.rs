//! Product facade for the integrated resident AOEM owners.
//!
//! The migrated node pipeline imports this module, never a runtime crate or a
//! second FFI implementation. Generic owned CPU tasks, bounded binary storage
//! and portable AORCP002 receipts retain their reviewed owner/ACK contracts.
//! A library selection also opens the existing wire facade, including typed
//! integer/GPU execution and proof workloads. This does not turn an opaque CPU
//! callback into a GPU plan or make fixed-profile proofs prove NOV semantics.
//!
//! The embedding node configures AOEM before starting threads. Selection here
//! does not mutate environment, discover another binary or fall back. Compute,
//! I/O and proof owners are distinct lifetimes, not duplicate authority stores;
//! opening a wire session does not attach it to an existing storage provider.
//! Receipt libraries may be separately pinned when the bundled execution SDK
//! does not export the required AORCP002 backend.

pub use crate::{
    AoemCapabilityContract, AoemExecFacade, AoemExecOpenOptions, AoemExecSession,
    AoemInteger1024V1, AoemIntegerOutcomeBatchV1, AoemIntegerOutcomeKindV1,
    AoemIntegerOutcomeRequestV1, AoemIntegerOutcomeRowV1, AoemRuntimeConfig,
};
use anyhow::{ensure, Context, Result};
pub use aoem_bindings::resident::{
    ComputeReport, ComputeSession, ComputeTask, ReceiptBackendUnavailable, ReceiptLimits,
    ReceiptSession, StorageConfig, StorageLimits, StorageSession, StorageWrite,
};
use std::path::{Path, PathBuf};

/// Explicit binary and worker options shared by existing wire and resident
/// boundaries. Canonicalization proves file identity, not binary authenticity.
#[derive(Clone, Debug)]
pub struct AoemLibrarySelection {
    library: PathBuf,
    options: AoemExecOpenOptions,
}

impl AoemLibrarySelection {
    pub fn new(library: impl AsRef<Path>, options: AoemExecOpenOptions) -> Result<Self> {
        let library = library
            .as_ref()
            .canonicalize()
            .context("resolve explicitly selected AOEM library")?;
        ensure!(library.is_file(), "selected AOEM library is not a file");
        Ok(Self { library, options })
    }

    /// Uses the original resolved configuration without apply_process_env().
    pub fn from_runtime(config: &AoemRuntimeConfig) -> Result<Self> {
        Self::new(&config.dll_path, config.open_options())
    }

    pub fn library(&self) -> &Path {
        &self.library
    }

    pub fn ingress_workers(&self) -> Option<u32> {
        self.options.ingress_workers
    }

    pub fn open_compute(&self) -> Result<ComputeSession> {
        ComputeSession::open(&self.library, self.options.ingress_workers.unwrap_or(0))
    }

    pub fn open_storage(&self, database: &Path, config: StorageConfig) -> Result<StorageSession> {
        StorageSession::open(&self.library, database, config)
    }

    pub fn open_receipt(&self, limits: ReceiptLimits) -> Result<ReceiptSession> {
        ReceiptSession::open(&self.library, limits)
    }

    /// Reuses the original unified wire/GPU surface, not a parallel interpreter.
    /// Ambient persistence is forbidden just as for the resident compute owner.
    /// Native initialization and plugin discovery still follow caller-prepared
    /// process configuration; missing capabilities fail at their actual call.
    pub fn open_wire_facade(&self) -> Result<AoemExecFacade> {
        ensure!(
            std::env::var_os("AOEM_PERSISTENCE_PATH").is_none_or(|path| path.is_empty()),
            "resident wire execution requires unset or empty AOEM_PERSISTENCE_PATH"
        );
        AoemExecFacade::open(&self.library, self.options.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_selection_preserves_original_wire_worker_options_without_loading() {
        // Selection does not load native code or silently substitute a DLL.
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let selection = AoemLibrarySelection::new(
            &file,
            AoemExecOpenOptions {
                ingress_workers: Some(7),
            },
        )
        .unwrap();
        assert_eq!(selection.library(), file.canonicalize().unwrap());
        assert_eq!(selection.ingress_workers(), Some(7));
    }

    #[test]
    fn explicit_selection_never_falls_back_for_missing_or_directory_paths() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(AoemLibrarySelection::new(root, AoemExecOpenOptions::default()).is_err());
        assert!(AoemLibrarySelection::new(
            root.join("no-resident-test-library-here"),
            AoemExecOpenOptions::default(),
        )
        .is_err());
    }

    #[test]
    fn resident_tasks_and_storage_writes_are_the_original_bindings_types() {
        let task: ComputeTask = Box::new(|| Ok(vec![1]));
        let task: aoem_bindings::resident::ComputeTask = task;
        assert_eq!(task().unwrap(), vec![1]);
        let write = StorageWrite::Delete { key: vec![2] };
        let original: aoem_bindings::resident::StorageWrite = write;
        assert_eq!(original, StorageWrite::Delete { key: vec![2] });
    }
}
