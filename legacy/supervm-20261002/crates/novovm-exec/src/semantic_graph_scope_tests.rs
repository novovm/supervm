use super::*;
use std::ffi::OsString;

// Runtime tests deliberately mutate AOEM's process environment. They serialize
// with one another and restore even on panic; run the full explicit runtime
// acceptance group with --test-threads=1, as the older runtime tests do not take
// this lock.
static SCOPE_RUNTIME_TEST_LOCK: Mutex<()> = Mutex::new(());

struct ScopeRuntimeEnvironment(Vec<(OsString, OsString)>);

fn scope_runtime_environment_key(key: &std::ffi::OsStr) -> bool {
    let key = key.to_string_lossy().to_ascii_uppercase();
    key.starts_with("AOEM_") || key.starts_with("NOVOVM_AOEM_") || key == "NOVOVM_INGRESS_WORKERS"
}

impl ScopeRuntimeEnvironment {
    fn isolated() -> Self {
        let saved = std::env::vars_os()
            .filter(|(key, _)| scope_runtime_environment_key(key))
            .collect::<Vec<_>>();
        let restore = Self(saved);
        for (key, _) in &restore.0 {
            std::env::remove_var(key);
        }
        restore
    }
}

impl Drop for ScopeRuntimeEnvironment {
    fn drop(&mut self) {
        let modified = std::env::vars_os()
            .filter(|(key, _)| scope_runtime_environment_key(key))
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        for key in modified {
            std::env::remove_var(key);
        }
        for (key, value) in &self.0 {
            std::env::set_var(key, value);
        }
    }
}

fn with_scope_runtime(label: &str, test: impl FnOnce(&AoemRuntimeConfig, &Path)) {
    let _lock = SCOPE_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _restore = ScopeRuntimeEnvironment::isolated();
    // Keep the native spelling passed to the provider: canonicalize on Windows
    // would add a verbatim prefix which this bundled RocksDB does not accept.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf();
    std::env::set_var("NOVOVM_AOEM_ROOT", root.join("aoem"));
    std::env::set_var("NOVOVM_AOEM_VARIANT", "core");
    // The explicit semantic storage provider below is RocksDB. Do not open an
    // unrelated legacy persistence backend without its own fixture path.
    std::env::set_var("NOVOVM_AOEM_PERSIST_BACKEND", "none");
    let runtime = AoemRuntimeConfig::from_env().expect("bundled AOEM runtime");
    let directory = root
        .join("artifacts/audit/semantic-graph-scope")
        .join(format!(
            "{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    assert!(directory.starts_with(&root));
    fs::create_dir_all(&directory).expect("isolated in-repository fixture");
    test(&runtime, &directory.join("provider.rocksdb"));
}

fn scope_request(graph_id: u64, value: &[u8]) -> AoemAtomicGraphRequestV1 {
    AoemAtomicGraphRequestV1 {
        graph_id,
        steps: vec![AoemAtomicGraphStepV1 {
            task_kind: 1,
            task_payload: b"opaque".to_vec(),
            writes: vec![AoemAtomicGraphWriteV1::Put {
                key: b"value".to_vec(),
                value: value.to_vec(),
            }],
            event: Some(AoemAtomicGraphEventV1 {
                kind: 1,
                payload: value.to_vec(),
            }),
        }],
        completion_write: AoemAtomicGraphWriteV1::Put {
            key: b"head".to_vec(),
            value: value.to_vec(),
        },
    }
}

fn scope_expect_open_error(
    runtime: &AoemRuntimeConfig,
    path: &Path,
    config: &AoemStorageProviderConfigV1,
) -> String {
    match AoemSemanticGraphStoreV1::open(runtime, path, config) {
        Ok(_) => panic!("incompatible or poisoned scope unexpectedly opened"),
        Err(error) => format!("{error:#}"),
    }
}

#[test]
fn graph_scope_rejects_nested_entry_and_is_thread_local() {
    let outer = AoemSemanticGraphSessionScopeV1::enter().expect("outer scope");
    assert!(AoemSemanticGraphSessionScopeV1::enter().is_err());
    std::thread::spawn(|| {
        let _other_thread = AoemSemanticGraphSessionScopeV1::enter()
            .expect("independent host thread may own its own scope");
        assert!(AoemSemanticGraphSessionScopeV1::enter().is_err());
    })
    .join()
    .unwrap();
    assert!(AoemSemanticGraphSessionScopeV1::enter().is_err());
    drop(outer);
    let _next = AoemSemanticGraphSessionScopeV1::enter().expect("scope released");
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_scope_real_aoem_reuses_provider_alias_and_reopens_after_drop() {
    with_scope_runtime("reuse", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let scope = AoemSemanticGraphSessionScopeV1::enter().unwrap();
        let first = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        let report = first.commit(scope_request(1_001, b"first")).unwrap();
        assert_eq!(
            (
                report.processed,
                report.succeeded,
                report.failed,
                report.durable_event_count
            ),
            (1, 1, 0, 1)
        );
        let weak = Rc::downgrade(&first.inner);
        let session = Rc::downgrade(&first.inner.session);
        let alias = path.parent().unwrap().join(".").join("provider.rocksdb");
        for index in 0..8 {
            let next = AoemSemanticGraphStoreV1::open(runtime, &alias, &config).unwrap();
            // Pointer identity proves that this is one native session/provider,
            // not a second RocksDB open which happened to see the same bytes.
            assert!(Rc::ptr_eq(&first.inner, &next.inner));
            assert!(Rc::ptr_eq(&first.inner.session, &next.inner.session));
            assert_eq!(first.inner.database_id, next.inner.database_id);
            assert_ne!(next.inner.database_id, 0);
            if index == 0 {
                // Recovery may resubmit exactly the same durable graph. A
                // long-lived session must not retain a completed graph ID and
                // spuriously reject a retry which a cold session permitted.
                let replay = next.commit(scope_request(1_001, b"first")).unwrap();
                assert_eq!(
                    (
                        replay.processed,
                        replay.succeeded,
                        replay.failed,
                        replay.durable_event_count
                    ),
                    (1, 1, 0, 1)
                );
            }
            assert_eq!(next.get(b"head").unwrap(), Some(b"first".to_vec()));
            assert_eq!(next.get(b"value").unwrap(), Some(b"first".to_vec()));
            assert_eq!(
                next.get(format!("absent-{index}").as_bytes()).unwrap(),
                None
            );
        }
        drop(first);
        assert!(weak.upgrade().is_some(), "scope retains the native owner");
        let last = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert!(Rc::ptr_eq(&weak.upgrade().unwrap(), &last.inner));
        last.commit(scope_request(1_002, b"second")).unwrap();
        drop(last);
        drop(scope);
        assert!(weak.upgrade().is_none());
        assert!(session.upgrade().is_none());

        let reopened = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert_eq!(reopened.get(b"head").unwrap(), Some(b"second".to_vec()));
        assert_eq!(reopened.get(b"value").unwrap(), Some(b"second".to_vec()));
        let unscoped = Rc::downgrade(&reopened.inner);
        drop(reopened);
        assert!(
            unscoped.upgrade().is_none(),
            "unscoped ownership is not cached"
        );
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_scope_real_aoem_unwind_releases_scope_and_provider() {
    with_scope_runtime("unwind", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let mut provider = None;
        let mut session = None;
        let failed = catch_unwind(AssertUnwindSafe(|| {
            let _scope = AoemSemanticGraphSessionScopeV1::enter().unwrap();
            let store = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
            store
                .commit(scope_request(1_101, b"before-unwind"))
                .unwrap();
            provider = Some(Rc::downgrade(&store.inner));
            session = Some(Rc::downgrade(&store.inner.session));
            panic!("host unwinds after a fully drained commit");
        }));
        assert!(failed.is_err());
        assert!(provider.unwrap().upgrade().is_none());
        assert!(session.unwrap().upgrade().is_none());
        let _scope = AoemSemanticGraphSessionScopeV1::enter().expect("TLS reset on unwind");
        let recovered = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert_eq!(
            recovered.get(b"head").unwrap(),
            Some(b"before-unwind".to_vec())
        );
        recovered
            .commit(scope_request(1_102, b"after-unwind"))
            .unwrap();
        assert_eq!(
            recovered.get(b"value").unwrap(),
            Some(b"after-unwind".to_vec())
        );
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_scope_real_aoem_rejects_runtime_config_path_and_environment_drift() {
    with_scope_runtime("identity", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let _scope = AoemSemanticGraphSessionScopeV1::enter().unwrap();
        let store = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        store.commit(scope_request(1_201, b"unchanged")).unwrap();
        let mut different_runtime = runtime.clone();
        different_runtime.ingress_workers = Some(runtime.ingress_workers.unwrap_or(16) + 1);
        scope_expect_open_error(&different_runtime, path, &config);
        let mut different_config = config.clone();
        different_config.sync_every += 1;
        scope_expect_open_error(runtime, path, &different_config);
        let other_path = path.parent().unwrap().join("different.rocksdb");
        scope_expect_open_error(runtime, &other_path, &config);
        assert!(
            !other_path.exists(),
            "rejected provider must not be created"
        );
        std::env::set_var("AOEM_FFI_GLOBAL_BUDGET", "23");
        scope_expect_open_error(runtime, path, &config);
        std::env::remove_var("AOEM_FFI_GLOBAL_BUDGET");
        let alias = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert!(Rc::ptr_eq(&store.inner, &alias.inner));
        assert_eq!(alias.get(b"head").unwrap(), Some(b"unchanged".to_vec()));
        assert!(
            !store.inner.poisoned.get(),
            "identity rejection is not a failed commit"
        );
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_scope_real_aoem_shares_poison_across_handles_and_new_opens() {
    with_scope_runtime("poison", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let scope = AoemSemanticGraphSessionScopeV1::enter().unwrap();
        let first = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        first.commit(scope_request(1_301, b"durable")).unwrap();
        let second = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert!(Rc::ptr_eq(&first.inner, &second.inner));
        // Simulate the existing uncertain-commit barrier without leaving a real
        // in-flight owner or waiting for the 30-second failure deadline.
        first.inner.poisoned.set(true);
        for store in [&first, &second] {
            assert!(store
                .get(b"head")
                .unwrap_err()
                .to_string()
                .contains("restart"));
            assert!(store
                .commit(scope_request(1_302, b"must-not-write"))
                .unwrap_err()
                .to_string()
                .contains("restart"));
        }
        assert!(scope_expect_open_error(runtime, path, &config).contains("restart"));
        drop(first);
        drop(second);
        assert!(scope_expect_open_error(runtime, path, &config).contains("restart"));
        drop(scope);
        let recovered = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert_eq!(recovered.get(b"head").unwrap(), Some(b"durable".to_vec()));
        assert_eq!(recovered.get(b"value").unwrap(), Some(b"durable".to_vec()));
        recovered
            .commit(scope_request(1_303, b"recovered"))
            .unwrap();
        assert_eq!(recovered.get(b"head").unwrap(), Some(b"recovered".to_vec()));
    });
}

#[test]
#[ignore = "requires bundled AOEM and isolated RocksDB; run runtime tests serially"]
fn graph_scope_real_aoem_pre_admission_validation_does_not_poison_shared_owner() {
    with_scope_runtime("pre-admission", |runtime, path| {
        let config = AoemStorageProviderConfigV1::default();
        let _scope = AoemSemanticGraphSessionScopeV1::enter().unwrap();
        let first = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        let second = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        first.commit(scope_request(1_401, b"original")).unwrap();
        let mut invalid = scope_request(1_402, b"invalid");
        invalid.steps[0].writes[0] = AoemAtomicGraphWriteV1::Put {
            key: b"value".to_vec(),
            value: vec![0; MAX_ATOMIC_WRITE_VALUE_BYTES_V1 + 1],
        };
        assert!(first
            .commit(invalid)
            .unwrap_err()
            .to_string()
            .contains("exceeds"));
        assert!(!first.inner.poisoned.get());
        assert_eq!(second.get(b"head").unwrap(), Some(b"original".to_vec()));
        assert_eq!(second.get(b"value").unwrap(), Some(b"original".to_vec()));
        second.commit(scope_request(1_403, b"valid")).unwrap();
        let third = AoemSemanticGraphStoreV1::open(runtime, path, &config).unwrap();
        assert!(Rc::ptr_eq(&first.inner, &third.inner));
        assert_eq!(third.get(b"head").unwrap(), Some(b"valid".to_vec()));
    });
}
