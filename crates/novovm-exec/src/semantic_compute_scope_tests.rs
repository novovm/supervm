use super::*;
use std::process::Command;
use std::time::Duration;

#[test]
fn compute_scope_is_lazy_non_nested_and_thread_local() {
    let scope = AoemComputeSessionScopeV1::enter().unwrap();
    assert!(scope.state.session.borrow().is_none());
    assert!(AoemComputeSessionScopeV1::enter().is_err());
    std::thread::spawn(|| {
        let scope = AoemComputeSessionScopeV1::enter().unwrap();
        assert!(scope.state.session.borrow().is_none());
        assert!(AoemComputeSessionScopeV1::enter().is_err());
    })
    .join()
    .unwrap();
    assert!(AoemComputeSessionScopeV1::enter().is_err());
    drop(scope);
    let _next = AoemComputeSessionScopeV1::enter().unwrap();
}

#[test]
fn compute_scope_identity_checks_runtime_environment_and_directory() {
    let runtime = AoemRuntimeConfig {
        variant: crate::AoemRuntimeVariant::Core,
        aoem_root: "sdk".into(),
        dll_path: "sdk/runtime".into(),
        manifest_path: "sdk/manifest.json".into(),
        runtime_profile_path: "sdk/profile.json".into(),
        plugin_dir: Some("sdk/plugins".into()),
        persist_backend: "none".into(),
        wasm_runtime: "none".into(),
        zkvm_mode: "none".into(),
        mldsa_mode: "none".into(),
        ingress_workers: Some(2),
    };
    let make = |runtime| RuntimeIdentity {
        runtime,
        current_dir: "workspace".into(),
        environment: BTreeMap::from([(OsString::from("AOEM_TEST"), OsString::from("one"))]),
    };
    let expected = make(runtime.clone());
    macro_rules! drift {
        ($field:ident, $value:expr) => {{
            let mut changed = runtime.clone();
            changed.$field = $value;
            assert!(expected != make(changed), stringify!($field));
        }};
    }
    drift!(variant, crate::AoemRuntimeVariant::Persist);
    drift!(aoem_root, "other".into());
    drift!(dll_path, "other".into());
    drift!(manifest_path, "other".into());
    drift!(runtime_profile_path, "other".into());
    drift!(plugin_dir, None);
    drift!(persist_backend, "rocksdb".into());
    drift!(wasm_runtime, "wasmtime".into());
    drift!(zkvm_mode, "risc0".into());
    drift!(mldsa_mode, "auto".into());
    drift!(ingress_workers, Some(3));
    let mut changed = make(runtime.clone());
    changed.current_dir = "other".into();
    assert!(expected != changed);
    let mut changed = make(runtime);
    changed.environment.insert("AOEM_TEST".into(), "two".into());
    assert!(expected != changed);
}

/// Only the child changes AOEM's process environment. Other libtests can run
/// concurrently without seeing these configuration-drift/poison experiments.
fn with_isolated_runtime(name: &str, test: impl FnOnce(&AoemRuntimeConfig)) {
    const CHILD: &str = "NOVOVM_COMPUTE_SCOPE_TEST_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        test(&AoemRuntimeConfig::from_env().expect("bundled compute runtime"));
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap()
        .to_path_buf();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        &format!("semantic_compute::session_scope::tests::{name}"),
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]);
    for (key, _) in std::env::vars_os() {
        let normalized = key.to_string_lossy().to_ascii_uppercase();
        if normalized.starts_with("AOEM_")
            || normalized.starts_with("NOVOVM_AOEM_")
            || normalized == "NOVOVM_INGRESS_WORKERS"
        {
            command.env_remove(key);
        }
    }
    let result = command
        .env(CHILD, name)
        .env("NOVOVM_AOEM_ROOT", root.join("aoem"))
        .env("NOVOVM_AOEM_VARIANT", "core")
        .env("NOVOVM_AOEM_PERSIST_BACKEND", "none")
        .current_dir(root)
        .output()
        .expect("start isolated compute scope test");
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("1 passed; 0 failed"), "{stdout}\n{stderr}");
}

fn expect_open_error(runtime: &AoemRuntimeConfig, fragment: &str) {
    let error = AoemComputeSessionV1::open_scoped(runtime)
        .err()
        .expect("incompatible or poisoned scoped session must not reopen");
    assert!(format!("{error:#}").contains(fragment), "{error:#}");
}

#[test]
#[ignore = "requires bundled AOEM; isolated child verifies actual cross-batch session reuse"]
fn compute_scope_real_aoem_reuses_two_batches_and_releases_normally() {
    with_isolated_runtime(
        "compute_scope_real_aoem_reuses_two_batches_and_releases_normally",
        |runtime| {
            let scope = AoemComputeSessionScopeV1::enter().unwrap();
            let mut first = AoemComputeSessionV1::open_scoped(runtime).unwrap();
            let weak = Rc::downgrade(&first.inner);
            let first_result = first
                .execute(
                    vec![Box::new(|| Ok(vec![1])), Box::new(|| Ok(vec![2]))],
                    Duration::from_secs(5),
                )
                .unwrap();
            assert_eq!(first_result.outputs, vec![vec![1], vec![2]]);
            assert_eq!((first_result.processed, first_result.failed), (2, 0));
            drop(first);
            assert!(
                weak.upgrade().is_some(),
                "scope must retain the same native owner"
            );
            let mut second = AoemComputeSessionV1::open_scoped(runtime).unwrap();
            assert!(Rc::ptr_eq(&weak.upgrade().unwrap(), &second.inner));
            // The first native open writes its effective AOEM configuration.
            // Reaching this second call proves those writes are normalized.
            let second_result = second
                .execute(vec![Box::new(|| Ok(vec![3]))], Duration::from_secs(5))
                .unwrap();
            assert_eq!(second_result.outputs, vec![vec![3]]);
            assert_eq!((second_result.processed, second_result.failed), (1, 0));
            drop(scope);
            assert!(
                weak.upgrade().is_some(),
                "an outstanding handle retains its owner"
            );
            drop(second);
            assert!(
                weak.upgrade().is_none(),
                "drained owner must release outside TLS"
            );

            // Outside a lifecycle scope the existing one-shot ownership remains.
            let cold = AoemComputeSessionV1::open_scoped(runtime).unwrap();
            let cold_weak = Rc::downgrade(&cold.inner);
            drop(cold);
            assert!(cold_weak.upgrade().is_none());
        },
    );
}

#[test]
#[ignore = "requires bundled AOEM; isolated child verifies shared poison and configuration drift"]
fn compute_scope_real_aoem_rejects_drift_and_reopen_after_failure() {
    with_isolated_runtime(
        "compute_scope_real_aoem_rejects_drift_and_reopen_after_failure",
        |runtime| {
            let scope = AoemComputeSessionScopeV1::enter().unwrap();
            let mut session = AoemComputeSessionV1::open_scoped(runtime).unwrap();
            let mut changed = runtime.clone();
            changed.ingress_workers = Some(runtime.ingress_workers.unwrap_or(1) + 1);
            expect_open_error(&changed, "unchanged runtime");
            std::env::set_var("AOEM_COMPUTE_SCOPE_TEST_DRIFT", "changed");
            expect_open_error(runtime, "unchanged runtime");
            std::env::remove_var("AOEM_COMPUTE_SCOPE_TEST_DRIFT");
            assert!(Rc::ptr_eq(
                &session.inner,
                &AoemComputeSessionV1::open_scoped(runtime).unwrap().inner
            ));
            assert!(session.execute(vec![], Duration::from_secs(1)).is_err());
            assert!(
                !session.inner.poisoned.get(),
                "pre-admission rejection is not poison"
            );
            assert_eq!(
                session
                    .execute(vec![Box::new(|| Ok(vec![7]))], Duration::from_secs(5))
                    .unwrap()
                    .outputs,
                vec![vec![7]]
            );
            assert!(session
                .execute(
                    vec![Box::new(|| bail!("injected compute failure"))],
                    Duration::from_secs(5)
                )
                .is_err());
            let weak = Rc::downgrade(&session.inner);
            drop(session);
            expect_open_error(runtime, "scope is poisoned");
            expect_open_error(&changed, "scope is poisoned");
            assert!(weak.upgrade().unwrap().poisoned.get());
            drop(scope);
            assert!(
                weak.upgrade().is_none(),
                "the failed graph drained before retirement"
            );
            // Explicit owner retirement, not dropping a public handle, permits
            // a fresh session after this known-drained test failure.
            let _next = AoemComputeSessionScopeV1::enter().unwrap();
            let mut recovered = AoemComputeSessionV1::open_scoped(runtime).unwrap();
            assert_eq!(
                recovered
                    .execute(vec![Box::new(|| Ok(vec![8]))], Duration::from_secs(5))
                    .unwrap()
                    .outputs,
                vec![vec![8]]
            );
        },
    );
}
