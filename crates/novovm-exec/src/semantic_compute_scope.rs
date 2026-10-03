//! Explicit lifetime reuse for generic AOEM compute sessions. This is not an
//! asynchronous pipeline, storage owner, or a second task scheduler.

use super::{AoemComputeSessionV1, AoemRuntimeConfig, ComputeSessionInner};
use crate::semantic_graph_v3::effective_runtime_environment;
use anyhow::{bail, Context, Result};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

thread_local! {
    // Discovery only. A TLS destructor must never join AOEM workers under the
    // Windows loader lock; the stack guard below owns the strong reference.
    static ACTIVE_SCOPE: RefCell<Weak<ScopeState>> = const { RefCell::new(Weak::new()) };
}

struct ScopeState {
    session: RefCell<Option<(RuntimeIdentity, Rc<ComputeSessionInner>)>>,
}

#[derive(PartialEq, Eq)]
struct RuntimeIdentity {
    runtime: AoemRuntimeConfig,
    current_dir: PathBuf,
    environment: BTreeMap<OsString, OsString>,
}

impl RuntimeIdentity {
    fn new(runtime: &AoemRuntimeConfig) -> Result<Self> {
        Ok(Self {
            runtime: runtime.clone(),
            current_dir: std::env::current_dir()
                .context("read AOEM compute scope current directory")?,
            // Share the existing storage-scope normalization, including the
            // environment writes made by AoemRuntimeConfig::apply_process_env.
            environment: effective_runtime_environment(runtime),
        })
    }
}

/// Explicit, non-nestable, same-thread lifetime for one lazy compute session.
///
/// Hold this guard on the actual node owner thread. `open_scoped` returns
/// handles to the same native session only while runtime/configuration and
/// effective environment remain unchanged. An admitted failure poisons every
/// handle, including later opens; no automatic recovery or backend fallback
/// occurs. Drop the guard during ordinary execution, not from a TLS destructor.
/// This scope does not make synchronous graph execution nonblocking.
#[must_use = "the scope guard must remain alive for the intended compute lifetime"]
pub struct AoemComputeSessionScopeV1 {
    state: Rc<ScopeState>,
}

impl AoemComputeSessionScopeV1 {
    pub fn enter() -> Result<Self> {
        ACTIVE_SCOPE.with(|slot| {
            let mut slot = slot
                .try_borrow_mut()
                .context("AOEM compute scope registration is busy")?;
            if slot.upgrade().is_some() {
                bail!("AOEM compute session scopes cannot be nested on one thread");
            }
            let state = Rc::new(ScopeState {
                session: RefCell::new(None),
            });
            *slot = Rc::downgrade(&state);
            Ok(Self { state })
        })
    }
}

impl Drop for AoemComputeSessionScopeV1 {
    fn drop(&mut self) {
        // Stop discovery first. Outstanding handles and retained undrained
        // ComputeFlight owners keep their native session/DLL alive themselves.
        let _ = ACTIVE_SCOPE.try_with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                if slot.ptr_eq(&Rc::downgrade(&self.state)) {
                    *slot = Weak::new();
                }
            }
        });
    }
}

pub(super) fn open(runtime: &AoemRuntimeConfig) -> Result<AoemComputeSessionV1> {
    let scope = ACTIVE_SCOPE.with(|slot| slot.borrow().upgrade());
    let Some(scope) = scope else {
        return AoemComputeSessionV1::open(runtime);
    };
    let identity = RuntimeIdentity::new(runtime)?;
    let mut session = scope
        .session
        .try_borrow_mut()
        .context("AOEM compute scoped session is already being opened")?;
    if let Some((existing, inner)) = session.as_ref() {
        // Do not let a dropped public handle or changed runtime escape a live
        // session's shared poison. Scope retirement is an explicit boundary.
        if inner.poisoned.get() {
            bail!("AOEM compute session scope is poisoned; retire this owner before recovery");
        }
        if *existing != identity {
            bail!(
                "AOEM compute session scope requires unchanged runtime and effective configuration"
            );
        }
        return Ok(AoemComputeSessionV1 {
            inner: Rc::clone(inner),
        });
    }
    let opened = AoemComputeSessionV1::open(runtime)?;
    *session = Some((identity, Rc::clone(&opened.inner)));
    Ok(opened)
}

#[cfg(test)]
#[path = "semantic_compute_scope_tests.rs"]
mod tests;
