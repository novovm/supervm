use super::{
    AoemRuntimeConfig, AoemSemanticGraphStoreV1, AoemStorageProviderConfigV1,
    SemanticGraphStoreInnerV1,
};
use anyhow::{bail, Context, Result};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::rc::{Rc, Weak};

thread_local! {
    // Never own a session in TLS: Windows TLS destruction may run under the
    // loader lock, where the provider's worker joins must not happen.
    static ACTIVE_SCOPE: RefCell<Weak<ScopeState>> = const { RefCell::new(Weak::new()) };
}

struct ScopeState {
    provider: RefCell<Option<(ProviderIdentity, Rc<SemanticGraphStoreInnerV1>)>>,
}

/// Explicit, non-nestable, same-thread lifetime for one generic storage provider.
///
/// Opening the same physical database/configuration reuses its session and shared
/// failure state. No database is opened until the first store request. Drop this
/// guard during ordinary thread execution, after its stores, not from a TLS
/// destructor. Without this guard stores keep their original open/drop behavior.
#[must_use = "the scope guard must remain alive for the intended provider lifetime"]
pub struct AoemSemanticGraphSessionScopeV1 {
    state: Rc<ScopeState>,
}

impl AoemSemanticGraphSessionScopeV1 {
    pub fn enter() -> Result<Self> {
        ACTIVE_SCOPE.with(|slot| {
            let mut slot = slot
                .try_borrow_mut()
                .context("AOEM graph scope registration is busy")?;
            if slot.upgrade().is_some() {
                bail!("AOEM semantic graph session scopes cannot be nested on one thread");
            }
            let state = Rc::new(ScopeState {
                provider: RefCell::new(None),
            });
            *slot = Rc::downgrade(&state);
            Ok(Self { state })
        })
    }
}

impl Drop for AoemSemanticGraphSessionScopeV1 {
    fn drop(&mut self) {
        // Clear discovery before releasing the strong owner. try_with also keeps
        // unwinding safe if a caller incorrectly drops the guard during TLS exit.
        let _ = ACTIVE_SCOPE.try_with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                if slot.ptr_eq(&Rc::downgrade(&self.state)) {
                    *slot = Weak::new();
                }
            }
        });
        // Outstanding stores/undrained submissions still retain their owners.
        // Rc releases the scope's provider outside TLS after this method returns.
    }
}

pub(super) fn open(
    runtime: &AoemRuntimeConfig,
    path: &Path,
    config: &AoemStorageProviderConfigV1,
) -> Result<Rc<SemanticGraphStoreInnerV1>> {
    let scope = ACTIVE_SCOPE.with(|slot| slot.borrow().upgrade());
    let Some(scope) = scope else {
        return AoemSemanticGraphStoreV1::open_uncached(runtime, path, config);
    };
    let identity = ProviderIdentity::new(runtime, path, config)?;
    let mut provider = scope
        .provider
        .try_borrow_mut()
        .context("AOEM semantic graph provider is already being opened")?;
    if let Some((existing, inner)) = provider.as_ref() {
        // A failed submission must not escape its shared poison by dropping the
        // public handle, changing an alias or asking to open another provider.
        inner.ensure_usable()?;
        if !existing.matches(&identity) {
            bail!("AOEM semantic graph session scope requires one unchanged database, runtime and storage configuration");
        }
        return Ok(inner.clone());
    }
    // Use the caller's original path, not the canonical identity. In particular,
    // Windows canonical paths carry a verbatim prefix unsupported by some builds.
    let inner = AoemSemanticGraphStoreV1::open_uncached(runtime, path, config)?;
    *provider = Some((identity, inner.clone()));
    Ok(inner)
}

struct ProviderIdentity {
    path: PathBuf,
    runtime: AoemRuntimeConfig,
    storage: AoemStorageProviderConfigV1,
    current_dir: PathBuf,
    environment: BTreeMap<OsString, OsString>,
}

impl ProviderIdentity {
    fn new(
        runtime: &AoemRuntimeConfig,
        path: &Path,
        storage: &AoemStorageProviderConfigV1,
    ) -> Result<Self> {
        Ok(Self {
            path: physical_path(path)?,
            runtime: runtime.clone(),
            storage: storage.clone(),
            current_dir: std::env::current_dir()
                .context("read AOEM graph scope current directory")?,
            environment: effective_environment(runtime),
        })
    }

    fn matches(&self, other: &Self) -> bool {
        // Destructure exhaustively so adding a runtime option cannot silently
        // leave it out of the session identity.
        let AoemRuntimeConfig {
            variant,
            aoem_root,
            dll_path,
            manifest_path,
            runtime_profile_path,
            plugin_dir,
            persist_backend,
            wasm_runtime,
            zkvm_mode,
            mldsa_mode,
            ingress_workers,
        } = &self.runtime;
        self.path == other.path
            && self.storage == other.storage
            && self.current_dir == other.current_dir
            && self.environment == other.environment
            && *variant == other.runtime.variant
            && *aoem_root == other.runtime.aoem_root
            && *dll_path == other.runtime.dll_path
            && *manifest_path == other.runtime.manifest_path
            && *runtime_profile_path == other.runtime.runtime_profile_path
            && *plugin_dir == other.runtime.plugin_dir
            && *persist_backend == other.runtime.persist_backend
            && *wasm_runtime == other.runtime.wasm_runtime
            && *zkvm_mode == other.runtime.zkvm_mode
            && *mldsa_mode == other.runtime.mldsa_mode
            && *ingress_workers == other.runtime.ingress_workers
    }
}

fn physical_path(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path).context("resolve AOEM storage provider path")?;
    let mut ancestor = absolute.as_path();
    let mut suffix = Vec::new();
    loop {
        match std::fs::canonicalize(ancestor) {
            Ok(mut resolved) => {
                for component in suffix.iter().rev() {
                    match component {
                        Component::ParentDir => {
                            resolved.pop();
                        }
                        Component::Normal(name) => resolved.push(name),
                        _ => {}
                    }
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let component = ancestor
                    .components()
                    .next_back()
                    .context("AOEM storage provider path has no existing ancestor")?;
                suffix.push(component);
                ancestor = ancestor
                    .parent()
                    .context("AOEM storage provider path has no existing parent")?;
            }
            Err(error) => return Err(error).context("resolve physical AOEM storage provider path"),
        }
    }
}

fn effective_environment(runtime: &AoemRuntimeConfig) -> BTreeMap<OsString, OsString> {
    let mut environment: BTreeMap<_, _> = std::env::vars_os()
        .filter_map(|(name, value)| {
            #[cfg(windows)]
            let key = name.to_string_lossy().to_ascii_uppercase();
            #[cfg(not(windows))]
            let key = name.to_string_lossy().into_owned();
            if key.starts_with("AOEM_")
                || key.starts_with("NOVOVM_AOEM_")
                || matches!(
                    key.as_str(),
                    "NOVOVM_INGRESS_WORKERS"
                        | "TEMP"
                        | "TMP"
                        | "TMPDIR"
                        | "PATH"
                        | "LD_LIBRARY_PATH"
                        | "DYLD_LIBRARY_PATH"
                        | "RAYON_NUM_THREADS"
                )
            {
                Some((OsString::from(key), value))
            } else {
                None
            }
        })
        .collect();
    // Match AoemRuntimeConfig::apply_process_env's effective values before the
    // first open, rather than mistaking its own environment writes for drift.
    for (key, value) in [
        ("AOEM_DLL", runtime.dll_path.as_os_str()),
        ("AOEM_DLL_MANIFEST", runtime.manifest_path.as_os_str()),
        (
            "AOEM_RUNTIME_PROFILE",
            runtime.runtime_profile_path.as_os_str(),
        ),
        (
            "AOEM_FFI_PERSIST_BACKEND",
            std::ffi::OsStr::new(&runtime.persist_backend),
        ),
        (
            "AOEM_FFI_WASM_RUNTIME",
            std::ffi::OsStr::new(&runtime.wasm_runtime),
        ),
        (
            "AOEM_FFI_ZKVM_MODE",
            std::ffi::OsStr::new(&runtime.zkvm_mode),
        ),
        (
            "AOEM_FFI_MLDSA_MODE",
            std::ffi::OsStr::new(&runtime.mldsa_mode),
        ),
    ] {
        environment.insert(OsString::from(key), value.to_owned());
    }
    if let Some(dir) = &runtime.plugin_dir {
        for key in [
            "AOEM_FFI_PLUGIN_DIR",
            "AOEM_FFI_PERSIST_PLUGIN_DIR",
            "AOEM_FFI_WASM_PLUGIN_DIR",
            "AOEM_FFI_ZKVM_PLUGIN_DIR",
            "AOEM_FFI_MLDSA_PLUGIN_DIR",
        ] {
            environment.insert(OsString::from(key), dir.as_os_str().to_owned());
        }
    }
    environment
}
