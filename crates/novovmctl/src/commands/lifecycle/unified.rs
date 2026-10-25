//! Native service management inside the existing lifecycle command. This owns
//! no protocol, ledger, daemon or credential provisioning. All effects are
//! explicit bounded calls to the platform's existing service manager.
use super::health::{self, HealthSpec};
use crate::{cli::LifecycleArgs, error::CtlError, output};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_MANIFEST: u64 = 4 * 1024 * 1024;
const MAX_UNIT: u64 = 128 * 1024;
const MAX_OUTPUT: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Deployment {
    schema_version: u32,
    host_id: String,
    package_root: PathBuf,
    expected_release_manifest_sha256: String,
    services: Vec<Service>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PinnedFile {
    path: PathBuf,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Manager {
    Systemd,
    WindowsScm,
    WindowsTask,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Node,
    Mailbox,
    Relay,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Service {
    role: Role,
    manager: Manager,
    name: String,
    unit_file: PathBuf,
    unit_sha256: String,
    binary: PathBuf,
    binary_sha256: String,
    config_files: Vec<PinnedFile>,
    protected_paths: Vec<PathBuf>,
    enabled: bool,
    #[serde(default)]
    existing: bool,
    health: Option<HealthSpec>,
}

#[derive(Deserialize)]
struct Release {
    scope: String,
    git_commit: String,
    target: String,
    mixed_version_rolling_upgrade_supported: bool,
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
struct Operation {
    program: String,
    args: Vec<String>,
}

#[derive(Debug)]
struct Captured {
    success: bool,
    stdout: String,
}

trait Runner {
    fn run(&mut self, operation: &Operation) -> Result<Captured, CtlError>;
}

struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&mut self, operation: &Operation) -> Result<Captured, CtlError> {
        // Never use a shell. Bounded readers prevent a child from blocking on a
        // full pipe; timeout kills and reaps only this command, not services.
        let mut child = Command::new(&operation.program)
            .args(&operation.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CtlError::ProcessLaunchFailed(format!("{}: {e}", operation.program)))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut source = stdout;
            let mut buffer = [0u8; 8192];
            loop {
                match source.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if bytes.len().saturating_add(count) > MAX_OUTPUT {
                            return Err("service manager output exceeds bounded limit".to_owned());
                        }
                        bytes.extend_from_slice(&buffer[..count]);
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
            Ok(bytes)
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
                other => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(CtlError::ProcessLaunchFailed(format!(
                        "{} did not finish within bound: {other:?}",
                        operation.program
                    )));
                }
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| bad("service manager output reader failed"))?
            .map_err(bad)?;
        Ok(Captured {
            success: status.success(),
            stdout: String::from_utf8(bytes).map_err(|_| bad("non-UTF8 service manager output"))?,
        })
    }
}

pub(super) fn run(args: &LifecycleArgs) -> Result<(), CtlError> {
    let result = run_with(args, &mut ProcessRunner);
    match result {
        Ok(record) => {
            if args.action == "health" {
                let envelope = serde_json::json!({"ok":false,"command":"lifecycle-unified-v1","health_accepted":false,"timestamp_unix_ms":output::now_unix_ms(),"host":output::host_name(),"data":record});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&envelope)
                        .map_err(|e| CtlError::FileWriteFailed(e.to_string()))?
                );
                return Err(CtlError::IntegrationFailed("full-stack readiness remains incomplete; see per-layer observations (not a process failure or asset authorization)".into()));
            }
            output::print_success_json("lifecycle-unified-v1", &record)?;
            Ok(())
        }
        Err(error) => {
            output::print_error_json("lifecycle-unified-v1", &error);
            Err(error)
        }
    }
}

fn run_with(args: &LifecycleArgs, runner: &mut impl Runner) -> Result<serde_json::Value, CtlError> {
    reject_legacy_options(args)?;
    let path = Path::new(args.deployment_manifest.as_ref().expect("dispatch checked"));
    let deployment: Deployment = read_json(path, MAX_MANIFEST)?;
    let release = validate_deployment(&deployment)?;
    let action = args.action.as_str();
    if !matches!(
        action,
        "plan"
            | "install"
            | "adopt"
            | "start"
            | "stop"
            | "status"
            | "logs"
            | "health"
            | "upgrade"
            | "rollback"
    ) {
        return Err(bad("unsupported unified action"));
    }
    if matches!(action, "upgrade" | "rollback") {
        return Err(bad("cross-release data/protocol compatibility is not qualified; upgrade/rollback refused without changing services, data or signing history"));
    }
    let selected: Vec<_> = deployment.services.iter().filter(|s| s.enabled).collect();
    if selected.is_empty() {
        return Err(bad(
            "no enabled reviewed services; example deployment is not executable",
        ));
    }
    if selected
        .iter()
        .any(|s| !matches!(s.manager, Manager::Systemd))
    {
        return Err(bad("unified Windows SCM/task install and ownership verification are not qualified; use existing reviewed platform adapters, not sc create for a non-SCM executable"));
    }
    let mut operations = Vec::new();
    for service in &selected {
        let operation = match action {
            "install" if service.existing => {
                return Err(bad(
                    "existing owner must be adopted, never reinstalled/provisioned",
                ))
            }
            "install" => Some(op("systemctl", &["link", path_str(&service.unit_file)?])),
            "start" | "stop" => Some(op("systemctl", &[action, &service.name])),
            "logs" => Some(op(
                "journalctl",
                &[
                    "--unit",
                    &service.name,
                    "--lines",
                    "100",
                    "--no-pager",
                    "--output",
                    "short-iso",
                ],
            )),
            _ => None,
        };
        if let Some(operation) = operation {
            operations.push(operation);
        }
    }
    if action == "install" {
        operations.push(op("systemctl", &["daemon-reload"]));
    }
    let observing = matches!(action, "status" | "logs" | "health" | "adopt");
    let mut observations = Vec::new();
    if args.apply || observing {
        if !cfg!(target_os = "linux") {
            return Err(bad("systemd apply/observations require Linux; cross-platform planning does not simulate installation"));
        }
        // Complete ALL target ownership checks before the first service effect.
        observations = preflight_all(&selected, action == "install", runner)?;
    }
    let mut applied = Vec::new();
    if args.apply || action == "logs" {
        // Pin everything again immediately before first effect. No automatic
        // rollback is attempted after a partial manager failure: it could erase
        // an independently established service or roll durable state backwards.
        validate_deployment(&deployment)?;
        for operation in &operations {
            let result = runner.run(operation)?;
            if !result.success {
                return Err(CtlError::IntegrationFailed(format!(
                    "service operation failed after {} successful operations; no data rollback or cleanup attempted: {} {:?}", applied.len(), operation.program, operation.args
                )));
            }
            applied.push(serde_json::json!({"operation":operation,"stdout":result.stdout}));
        }
    }
    let health_reports: Vec<_> = if action == "health" {
        selected.iter().map(|service| {
            serde_json::json!({"service":service.name,"health":service.health.as_ref().map(health::inspect)})
        }).collect()
    } else {
        Vec::new()
    };
    Ok(serde_json::json!({
        "schema_version":1,"host_id":deployment.host_id,"action":action,
        "source_commit":release.git_commit,"package_target":release.target,
        "release_manifest_sha256":deployment.expected_release_manifest_sha256,
        "applied":args.apply && !operations.is_empty(),"operations":operations,
        "results":applied,"service_observations":observations,"health":health_reports,
        "full_stack_accepted":false,"service_settlement":"not_connected",
        "manager_mutates_data_directly":false,"credentials_provisioned":false,
        "custody_verified":false,
        "note":"install only links reviewed units; no enable/start, key creation or genesis. Planning checks credential source metadata only; it does not read secrets or prove that the final runtime account can decrypt custody. Active process is not end-to-end health. Upgrade/rollback require a separately qualified compatibility transition."
    }))
}

fn validate_deployment(deployment: &Deployment) -> Result<Release, CtlError> {
    if deployment.schema_version != 1 || !safe_name(&deployment.host_id) {
        return Err(bad("invalid deployment schema or host identity"));
    }
    plain_path(&deployment.package_root, true)?;
    let manifest = deployment.package_root.join("release-manifest.json");
    verify_file(&manifest, &deployment.expected_release_manifest_sha256)?;
    let release: Release = read_json(&manifest, MAX_MANIFEST)?;
    if release.scope != "novovm_unified_linux_release_v3"
        || release.git_commit.len() != 40
        || !release.git_commit.bytes().all(|b| b.is_ascii_hexdigit())
        || release.mixed_version_rolling_upgrade_supported
        || release.artifacts.is_empty()
        || release.artifacts.len() > 4096
    {
        return Err(bad("package is not the qualified unified v3 manifest shape; no mixed-version upgrade inference"));
    }
    let mut packaged = BTreeMap::new();
    for artifact in &release.artifacts {
        let relative = Path::new(&artifact.path);
        if relative.is_absolute()
            || artifact.path.contains('\\')
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || artifact.path.is_empty()
        {
            return Err(bad("unsafe package artifact path"));
        }
        let path = deployment.package_root.join(relative);
        if packaged
            .insert(path.clone(), artifact.sha256.clone())
            .is_some()
        {
            return Err(bad("duplicate package artifact"));
        }
        verify_file(&path, &artifact.sha256)?;
        if fs::metadata(&path).map_err(read_error)?.len() != artifact.bytes {
            return Err(bad("package artifact length mismatch"));
        }
    }
    if deployment.services.is_empty() || deployment.services.len() > 3 {
        return Err(bad(
            "deployment must describe one to three existing service roles",
        ));
    }
    let mut names = BTreeSet::new();
    let mut roles = BTreeSet::new();
    let mut data_owners: Vec<(&str, &Path)> = Vec::new();
    for service in deployment.services.iter().filter(|s| s.enabled) {
        if !safe_unit_name(&service.name)
            || !names.insert(&service.name)
            || !roles.insert(format!("{:?}", service.role))
        {
            return Err(bad("invalid or duplicate service identity/role"));
        }
        verify_file(&service.binary, &service.binary_sha256)?;
        verify_file(&service.unit_file, &service.unit_sha256)?;
        if !service.existing && packaged.get(&service.binary) != Some(&service.binary_sha256) {
            return Err(bad(
                "new service binary must match a pinned package artifact",
            ));
        }
        let expected_binary = match service.role {
            Role::Node => "novovm-node",
            Role::Mailbox => "novovm-rendezvous-service",
            Role::Relay => "supervm-relay",
        };
        if service.binary.file_name().and_then(|s| s.to_str()) != Some(expected_binary) {
            return Err(bad("service role does not match original product binary"));
        }
        if service.config_files.is_empty()
            || service.config_files.len() > 32
            || service.protected_paths.is_empty()
            || service.protected_paths.len() > 32
        {
            return Err(bad(
                "service requires bounded explicit config pins and protected data/identity paths",
            ));
        }
        let mut inputs = BTreeSet::new();
        for file in &service.config_files {
            if !inputs.insert(file.path.clone()) {
                return Err(bad("duplicate configuration pin"));
            }
            verify_file(&file.path, &file.sha256)?;
        }
        for path in &service.protected_paths {
            plain_path(path, true)?;
            if path.starts_with(&deployment.package_root)
                || deployment.package_root.starts_with(path)
            {
                return Err(bad(
                    "data and identity paths must be external to immutable release",
                ));
            }
            for (owner, prior) in &data_owners {
                if *owner != service.name && (path.starts_with(prior) || prior.starts_with(path)) {
                    return Err(bad(
                        "different service roles cannot share a protected owner directory",
                    ));
                }
            }
            data_owners.push((&service.name, path));
        }
        if service.unit_file.file_name().and_then(|s| s.to_str()) != Some(service.name.as_str()) {
            return Err(bad("unit filename must match managed service name"));
        }
        validate_unit(service, &packaged, &deployment.package_root)?;
        if let Some(spec) = &service.health {
            if (spec.native_client.is_some() && !matches!(service.role, Role::Node))
                || (spec.rendezvous.is_some() && !matches!(service.role, Role::Mailbox))
            {
                return Err(bad("health probe must belong to the managed service role"));
            }
            for path in spec.executable_paths() {
                let expected = packaged
                    .get(path)
                    .or_else(|| (path == service.binary).then_some(&service.binary_sha256));
                verify_file(
                    path,
                    expected.ok_or_else(|| {
                        bad("health executable is not pinned by this package/service")
                    })?,
                )?;
            }
            for path in spec.input_paths() {
                if !inputs.contains(path) {
                    return Err(bad("health input is not pinned in service configuration"));
                }
            }
        }
    }
    Ok(release)
}

fn validate_unit(
    service: &Service,
    packaged: &BTreeMap<PathBuf, String>,
    package_root: &Path,
) -> Result<(), CtlError> {
    if !matches!(service.manager, Manager::Systemd) {
        return Ok(());
    }
    let unit = read_text(&service.unit_file, MAX_UNIT)?;
    let entries = unit_values(&unit)?;
    let starts = entries
        .get("ExecStart")
        .ok_or_else(|| bad("unit lacks ExecStart"))?;
    if starts.len() != 1 || starts[0].split_whitespace().next() != Some(path_str(&service.binary)?)
    {
        return Err(bad(
            "unit ExecStart does not bind exactly to pinned service binary",
        ));
    }
    if entries.keys().any(|key| {
        matches!(
            key.as_str(),
            "ExecStartPre"
                | "ExecStartPost"
                | "ExecStop"
                | "ExecStopPost"
                | "ExecReload"
                | "RootDirectory"
                | "RootImage"
                | "Environment"
                | "PassEnvironment"
                | "UnsetEnvironment"
        )
    }) {
        return Err(bad(
            "unit contains unsupported hooks/root remapping; no hidden provisioning or migration",
        ));
    }
    let pinned: BTreeSet<_> = service
        .config_files
        .iter()
        .map(|f| path_str(&f.path))
        .collect::<Result<_, _>>()?;
    if let Some(envfiles) = entries.get("EnvironmentFile") {
        for file in envfiles {
            if !pinned.contains(file.as_str()) {
                return Err(bad("unit EnvironmentFile is not explicitly pinned"));
            }
        }
    }
    match service.role {
        Role::Node => {
            validate_node_credential(service, &entries)?;
            let envfiles = entries
                .get("EnvironmentFile")
                .ok_or_else(|| bad("node requires one pinned actual EnvironmentFile"))?;
            if envfiles.len() != 1 {
                return Err(bad(
                    "node requires exactly one EnvironmentFile, without overrides",
                ));
            }
            let env = parse_node_environment(&read_text(Path::new(&envfiles[0]), MAX_UNIT)?)?;
            validate_aoem_layout(&env, package_root)?;
            let config = env
                .get("NOVOVM_NATIVE_EXECUTION_CONFIG")
                .ok_or_else(|| bad("missing native execution config"))?;
            if !pinned.contains(config.as_str()) {
                return Err(bad("native execution configuration must be pinned"));
            }
            for key in [
                "NOVOVM_AOEM_DLL",
                "NOVOVM_AOEM_MANIFEST",
                "NOVOVM_AOEM_RUNTIME_PROFILE",
            ] {
                let path = Path::new(
                    env.get(key)
                        .ok_or_else(|| bad("node requires package AOEM path pins"))?,
                );
                let expected = packaged.get(path).ok_or_else(|| {
                    bad("AOEM environment path is outside pinned package artifacts")
                })?;
                verify_file(path, expected)?;
            }
            let env_library = Path::new(env.get("NOVOVM_AOEM_DLL").expect("checked above"));
            validate_native_config(service, Path::new(config), packaged, env_library)?;
        }
        Role::Mailbox => {
            if !starts[0].contains(" --foreground ")
                || !starts[0].split_whitespace().any(|arg| pinned.contains(arg))
            {
                return Err(bad(
                    "mailbox requires original foreground owner with pinned config",
                ));
            }
            if let Some(probe) = service.health.as_ref().and_then(|h| h.rendezvous.as_ref()) {
                if probe.binary != service.binary
                    || starts[0].split_whitespace().last() != Some(path_str(&probe.config)?)
                {
                    return Err(bad(
                        "mailbox health must inspect the actual managed owner/config",
                    ));
                }
            }
        }
        Role::Relay => {
            if !service.existing && !starts[0].contains(" --runtime duplex-v2 ") {
                return Err(bad("fresh relay cannot silently select legacy JSON mode"));
            }
            if !starts[0].split_whitespace().any(|arg| pinned.contains(arg)) {
                return Err(bad("relay config is not pinned in ExecStart"));
            }
        }
    }
    Ok(())
}

/// Only inspect the reviewed source path and its metadata. The original
/// systemd/custody owner, under the final service account, must decrypt it.
/// Neither a valid plan nor a pinned unit constitutes custody qualification.
fn validate_node_credential(
    service: &Service,
    entries: &BTreeMap<String, Vec<String>>,
) -> Result<(), CtlError> {
    if [
        "LoadCredential",
        "SetCredential",
        "SetCredentialEncrypted",
        "ImportCredential",
    ]
    .iter()
    .any(|key| entries.contains_key(*key))
    {
        return Err(bad(
            "node permits only one encrypted owner-key source, without plaintext, inline or imported credential overrides",
        ));
    }
    let sources = entries
        .get("LoadCredentialEncrypted")
        .ok_or_else(|| bad("node requires LoadCredentialEncrypted=owner-key:absolute-source"))?;
    if sources.len() != 1 {
        return Err(bad("node requires exactly one encrypted owner-key source"));
    }
    let source = sources[0]
        .strip_prefix("owner-key:")
        .filter(|source| {
            !source.is_empty()
                && !source.chars().any(|c| {
                    c.is_whitespace()
                        || c.is_control()
                        || matches!(c, '%' | '\'' | '"')
                        || (cfg!(unix) && c == '\\')
                })
        })
        .ok_or_else(|| {
            bad("encrypted node credential must be an explicit owner-key path without systemd expansion or quoting")
        })?;
    let source = Path::new(source);
    plain_path(source, false)?;
    if !service
        .protected_paths
        .iter()
        .any(|protected| source.starts_with(protected))
    {
        return Err(bad(
            "encrypted node credential source is outside the node's protected owner directories",
        ));
    }
    Ok(())
}

fn unit_values(unit: &str) -> Result<BTreeMap<String, Vec<String>>, CtlError> {
    let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut service = false;
    for raw in unit.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            service = line == "[Service]";
            continue;
        }
        if !service || line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.ends_with('\\') {
            return Err(bad(
                "continued unit directives require explicit adapter review",
            ));
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| bad("invalid unit directive"))?;
        values
            .entry(key.trim().to_owned())
            .or_default()
            .push(value.trim().to_owned());
    }
    Ok(values)
}

fn preflight_service(
    service: &Service,
    install: bool,
    runner: &mut impl Runner,
) -> Result<serde_json::Value, CtlError> {
    let operation = op(
        "systemctl",
        &[
            "show",
            "--no-pager",
            "--property=LoadState,ActiveState,SubState,FragmentPath,DropInPaths,ExecStart,EnvironmentFiles,NeedDaemonReload,Environment,PassEnvironment,UnsetEnvironment,ExecStartPre,ExecStartPost,ExecStop,ExecStopPost,ExecReload,RootDirectory,RootImage",
            &service.name,
        ],
    );
    let observed = runner.run(&operation)?;
    let values: BTreeMap<_, _> = observed
        .stdout
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    if install {
        if values.get("LoadState") != Some(&"not-found") {
            return Err(bad(
                "install target already exists or cannot be verified absent; use explicit adoption",
            ));
        }
    } else {
        if !observed.success
            || values.get("LoadState") != Some(&"loaded")
            || values.get("FragmentPath") != Some(&path_str(&service.unit_file)?)
            || values.get("DropInPaths").is_none_or(|v| !v.is_empty())
            || values.get("NeedDaemonReload") != Some(&"no")
            || [
                "Environment",
                "PassEnvironment",
                "UnsetEnvironment",
                "ExecStartPre",
                "ExecStartPost",
                "ExecStop",
                "ExecStopPost",
                "ExecReload",
                "RootDirectory",
                "RootImage",
            ]
            .iter()
            .any(|key| values.get(key).is_none_or(|value| !value.is_empty()))
        {
            return Err(bad(
                "installed service ownership differs from pinned unit or has unreviewed drop-ins",
            ));
        }
        let exec = values
            .get("ExecStart")
            .ok_or_else(|| bad("manager omitted ExecStart"))?;
        let unit = unit_values(&read_text(&service.unit_file, MAX_UNIT)?)?;
        let expected = &unit["ExecStart"][0];
        if !exec.contains(&format!("path={} ;", path_str(&service.binary)?))
            || !exec.contains(&format!("argv[]={expected} ;"))
        {
            return Err(bad(
                "loaded service executable/arguments differ from reviewed unit",
            ));
        }
        let expected = unit
            .get("EnvironmentFile")
            .map(|files| {
                files
                    .iter()
                    .map(|file| format!("{file} (ignore_errors=no)"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        if values.get("EnvironmentFiles") != Some(&expected.as_str()) {
            return Err(bad(
                "loaded service environment-file binding differs from reviewed unit",
            ));
        }
    }
    Ok(
        serde_json::json!({"service":service.name,"manager":"systemd","load_state":values.get("LoadState"),"active_state":values.get("ActiveState"),"sub_state":values.get("SubState"),"ownership_checked":!install,"health_verified":false}),
    )
}

fn validate_native_config(
    service: &Service,
    path: &Path,
    packaged: &BTreeMap<PathBuf, String>,
    env_library: &Path,
) -> Result<(), CtlError> {
    // Reuse the real parser and genesis/quorum checks. This opens no database,
    // loads no signer/AOEM context and performs no initialization.
    let parsed = novovm_node::native_pipeline::service::ResidentConfig::load(path)
        .map_err(|e| bad(format!("original native configuration rejected: {e}")))?;
    same_library_binding(&parsed.library, env_library)?;
    if parsed.key_storage != novovm_node::native_pipeline::service::NodeKeyStorage::ProtectedV1 {
        return Err(bad("managed node requires protected_v1 purpose custody; raw_development is test tooling only"));
    }
    if parsed.uca_initialization.is_none() {
        return Err(bad("economic deployment requires approved UCA initialization; the Ed25519 reference config is not asset activation"));
    }
    let native: serde_json::Value = read_json(path, MAX_MANIFEST)?;
    if native["profile"] != "native_execution_tick" || !native["rpc_addr"].is_null() {
        return Err(bad(
            "managed node must use original native tick with no HTTP release listener",
        ));
    }
    let base = path
        .parent()
        .ok_or_else(|| bad("native config has no parent"))?;
    for key in ["database", "signing_key_file", "transport_key_file"] {
        let configured = native[key]
            .as_str()
            .ok_or_else(|| bad("native config lacks protected data/key path"))?;
        let configured = Path::new(configured);
        if configured
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(bad(
                "native protected paths must be normalized without traversal",
            ));
        }
        let absolute = if configured.is_absolute() {
            configured.to_path_buf()
        } else {
            base.join(configured)
        };
        plain_path(&absolute, key == "database")?;
        if !service
            .protected_paths
            .iter()
            .any(|protected| absolute.starts_with(protected))
        {
            return Err(bad(
                "native database/signing journal owner is outside declared protected directories",
            ));
        }
    }
    let library = native["library"]
        .as_str()
        .ok_or_else(|| bad("missing native AOEM library"))?;
    let library = Path::new(library);
    let library = if library.is_absolute() {
        library.to_path_buf()
    } else {
        base.join(library)
    };
    let expected = packaged
        .get(&library)
        .ok_or_else(|| bad("native AOEM library must be the pinned package artifact"))?;
    verify_file(&library, expected)?;
    if let Some(probe) = service
        .health
        .as_ref()
        .and_then(|h| h.native_client.as_ref())
    {
        let client: serde_json::Value = read_json(&probe.config, MAX_MANIFEST)?;
        for (client_key, genesis_key) in [
            ("chain_id", "chain_id"),
            ("validator_epoch", "validator_epoch"),
            ("protocol", "protocol_commitment"),
            ("validators", "validators"),
        ] {
            if client[client_key] != native["genesis"][genesis_key] {
                return Err(bad(
                    "health client chain/validator domain differs from managed node",
                ));
            }
        }
        // A wildcard listener has no unique remote identity. Require a local
        // loopback probe with the same UDP port; never certify another peer.
        let listener: std::net::SocketAddr = native["udp_addr"]
            .as_str()
            .ok_or_else(|| bad("missing native UDP listener"))?
            .parse()
            .map_err(|_| bad("invalid native UDP listener"))?;
        let destination: std::net::SocketAddr = client["node"]["address"]
            .as_str()
            .ok_or_else(|| bad("missing health endpoint"))?
            .parse()
            .map_err(|_| bad("invalid health endpoint"))?;
        if listener.port() != destination.port() || !destination.ip().is_loopback() {
            return Err(bad(
                "health client must query managed local loopback UDP port",
            ));
        }
        let expected_genesis = if native["uca_initialization"].is_null() {
            &native["genesis"]["genesis_config_commitment"]
        } else {
            &native["uca_initialization"]["expected_final_commitment"]
        };
        if &client["genesis"] != expected_genesis {
            return Err(bad(
                "health client genesis differs from managed node final pin",
            ));
        }
        let local_key = &client["node"]["public_key"];
        if native["peers"]
            .as_array()
            .is_none_or(|peers| peers.iter().any(|peer| &peer["public_key"] == local_key))
            || native["genesis"]["validators"]
                .as_array()
                .is_none_or(|validators| !validators.iter().any(|v| &v["public_key"] == local_key))
        {
            return Err(bad(
                "health client does not identify the local configured validator",
            ));
        }
    }
    Ok(())
}

fn same_library_binding(configured: &Path, env_library: &Path) -> Result<(), CtlError> {
    if configured != env_library {
        return Err(bad(
            "native configuration and loaded EnvironmentFile select different AOEM artifacts",
        ));
    }
    Ok(())
}

/// Bind this package's explicit core layout. This closes a deployment-contract
/// ambiguity; it does not assert that native tick used an external plugin dir.
fn validate_aoem_layout(
    env: &BTreeMap<String, String>,
    package_root: &Path,
) -> Result<(), CtlError> {
    let root = package_root.join("aoem");
    let plugins = root.join("linux/core/plugins");
    if env.get("NOVOVM_AOEM_VARIANT").map(String::as_str) != Some("core")
        || env.get("NOVOVM_AOEM_ROOT").map(Path::new) != Some(root.as_path())
        || env.get("NOVOVM_AOEM_PLUGIN_DIR").map(Path::new) != Some(plugins.as_path())
    {
        return Err(bad(
            "AOEM root/plugin/variant must match the pinned unified Linux core package layout",
        ));
    }
    plain_path(&root, true)?;
    plain_path(&plugins, true)?;
    Ok(())
}

fn parse_node_environment(text: &str) -> Result<BTreeMap<String, String>, CtlError> {
    let mut values = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| bad("invalid native environment line"))?;
        if key.trim() != key
            || value.trim() != value
            || value.contains(['\'', '"', '\\'])
            || values.insert(key.to_string(), value.to_string()).is_some()
        {
            return Err(bad(
                "duplicate/quoted/escaped environment overrides require explicit review",
            ));
        }
        if !matches!(
            key,
            "NOVOVM_NODE_MODE"
                | "NOVOVM_NATIVE_EXECUTION_CONFIG"
                | "NOVOVM_NATIVE_EXECUTION_START"
                | "NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS"
                | "NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS"
                | "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH"
                | "NOVOVM_NATIVE_EXECUTION_PIPELINE_SUMMARY_REPORT_PATH"
                | "NOVOVM_AOEM_VARIANT"
                | "NOVOVM_AOEM_ROOT"
                | "NOVOVM_AOEM_DLL"
                | "NOVOVM_AOEM_MANIFEST"
                | "NOVOVM_AOEM_RUNTIME_PROFILE"
                | "NOVOVM_AOEM_PLUGIN_DIR"
                | "AOEM_SEMANTIC_GRAPH_V2_WORKERS"
        ) {
            return Err(bad(
                "legacy/HTTP/loader override prohibited in native service",
            ));
        }
    }
    if values.get("NOVOVM_NODE_MODE").map(String::as_str) != Some("native_execution_tick")
        || values
            .get("NOVOVM_NATIVE_EXECUTION_START")
            .map(String::as_str)
            != Some("existing")
    {
        return Err(bad("service must use native tick and existing database; create/genesis activation is not installation"));
    }
    Ok(values)
}

fn preflight_all(
    services: &[&Service],
    install: bool,
    runner: &mut impl Runner,
) -> Result<Vec<serde_json::Value>, CtlError> {
    services
        .iter()
        .map(|service| preflight_service(service, install, runner))
        .collect()
}

fn reject_legacy_options(args: &LifecycleArgs) -> Result<(), CtlError> {
    if args.force
        || args.version.is_some()
        || args.target_version.is_some()
        || args.rollback_version.is_some()
        || args.node_binary_from.is_some()
        || args.gateway_binary_from.is_some()
        || args.runtime_template_file.is_some()
        || args.profile.is_some()
        || args.runtime_state_file.is_some()
        || args.set_current
        || args.restart_after_set_runtime
    {
        return Err(bad(
            "legacy lifecycle overrides/force cannot accompany a native deployment manifest",
        ));
    }
    Ok(())
}
fn op(program: &str, args: &[&str]) -> Operation {
    Operation {
        program: program.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
    }
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 96
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !name.starts_with('-')
}
fn safe_unit_name(name: &str) -> bool {
    safe_name(name) && name.ends_with(".service")
}
fn path_str(path: &Path) -> Result<&str, CtlError> {
    path.to_str()
        .ok_or_else(|| bad("non-UTF8 path not supported by service contract"))
}
fn bad(message: impl Into<String>) -> CtlError {
    CtlError::InvalidArgument(message.into())
}
fn read_error(error: std::io::Error) -> CtlError {
    CtlError::FileReadFailed(error.to_string())
}
fn read_text(path: &Path, limit: u64) -> Result<String, CtlError> {
    let metadata = fs::metadata(path).map_err(read_error)?;
    if metadata.len() > limit {
        return Err(bad("configuration exceeds bounded size"));
    }
    let mut content = String::new();
    File::open(path)
        .map_err(read_error)?
        .take(limit + 1)
        .read_to_string(&mut content)
        .map_err(read_error)?;
    if content.len() as u64 > limit {
        return Err(bad("configuration grew beyond bounded size"));
    }
    Ok(content.trim_start_matches('\u{feff}').to_string())
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T, CtlError> {
    serde_json::from_str(&read_text(path, limit)?)
        .map_err(|e| bad(format!("invalid {}: {e}", path.display())))
}
fn verify_file(path: &Path, expected: &str) -> Result<(), CtlError> {
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("missing or invalid SHA-256 pin"));
    }
    plain_path(path, false)?;
    let mut file = File::open(path).map_err(read_error)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(read_error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != expected.to_ascii_lowercase() {
        return Err(bad(format!("pinned file changed: {}", path.display())));
    }
    Ok(())
}
fn plain_path(path: &Path, directory: bool) -> Result<(), CtlError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(bad("paths must be absolute and normalized"));
    }
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(read_error)?;
        if metadata.file_type().is_symlink() {
            return Err(bad(
                "symlink path requires separate reviewed adoption; refusing implicit redirection",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(bad("reparse point in managed path"));
            }
        }
    }
    let metadata = fs::metadata(path).map_err(read_error)?;
    if directory && !metadata.is_dir() || !directory && !metadata.is_file() {
        return Err(bad("managed path has incorrect file type"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, TopCommand};
    use clap::Parser;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
        deployment: Deployment,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "novovm-unified-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("release/bin")).unwrap();
            fs::create_dir_all(root.join("config")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let binary = root.join("release/bin/supervm-relay");
            fs::write(&binary, b"fixture only").unwrap();
            let config = root.join("config/relay.json");
            fs::write(&config, b"{}").unwrap();
            let unit = root.join("config/test-relay.service");
            fs::write(
                &unit,
                format!(
                    "[Service]\nExecStart={} --runtime duplex-v2 {}\n",
                    binary.display(),
                    config.display()
                ),
            )
            .unwrap();
            let release = serde_json::json!({"scope":"novovm_unified_linux_release_v3","git_commit":"a".repeat(40),"target":"x86_64-unknown-linux-gnu","mixed_version_rolling_upgrade_supported":false,"artifacts":[{"path":"bin/supervm-relay","sha256":hash(&binary),"bytes":12}]});
            let manifest = root.join("release/release-manifest.json");
            fs::write(&manifest, serde_json::to_vec(&release).unwrap()).unwrap();
            let deployment = Deployment {
                schema_version: 1,
                host_id: "test-host".into(),
                package_root: root.join("release"),
                expected_release_manifest_sha256: hash(&manifest),
                services: vec![Service {
                    role: Role::Relay,
                    manager: Manager::Systemd,
                    name: "test-relay.service".into(),
                    unit_file: unit.clone(),
                    unit_sha256: hash(&unit),
                    binary: binary.clone(),
                    binary_sha256: hash(&binary),
                    config_files: vec![PinnedFile {
                        path: config.clone(),
                        sha256: hash(&config),
                    }],
                    protected_paths: vec![root.join("data")],
                    enabled: true,
                    existing: false,
                    health: None,
                }],
            };
            Self { root, deployment }
        }
        fn service(&self) -> &Service {
            &self.deployment.services[0]
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("novovm-unified-"));
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn hash(path: &Path) -> String {
        format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
    }
    #[derive(Default)]
    struct FakeRunner {
        outputs: VecDeque<Captured>,
        calls: Vec<String>,
    }
    impl Runner for FakeRunner {
        fn run(&mut self, operation: &Operation) -> Result<Captured, CtlError> {
            self.calls
                .push(format!("{} {:?}", operation.program, operation.args));
            Ok(self
                .outputs
                .pop_front()
                .expect("unexpected service-manager call"))
        }
    }
    fn observed(service: &Service, extra: &str) -> Captured {
        let unit = unit_values(&read_text(&service.unit_file, MAX_UNIT).unwrap()).unwrap();
        Captured{success:true,stdout:format!("LoadState=loaded\nActiveState=active\nSubState=running\nFragmentPath={}\nDropInPaths=\nEnvironmentFiles=\nNeedDaemonReload=no\nEnvironment=\nPassEnvironment=\nUnsetEnvironment=\nExecStartPre=\nExecStartPost=\nExecStop=\nExecStopPost=\nExecReload=\nRootDirectory=\nRootImage=\nExecStart={{ path={} ; argv[]={} ; ignore_errors=no ; }}\n{extra}",service.unit_file.display(),service.binary.display(),unit["ExecStart"][0])}
    }

    #[test]
    fn validates_original_package_and_external_owner() {
        let f = Fixture::new();
        assert!(validate_deployment(&f.deployment).is_ok());
    }
    #[test]
    fn rejects_changed_artifact_even_with_same_filename() {
        let f = Fixture::new();
        fs::write(&f.service().binary, b"changed").unwrap();
        assert!(validate_deployment(&f.deployment).is_err());
    }
    #[test]
    fn rejects_config_changed_after_review() {
        let f = Fixture::new();
        fs::write(&f.service().config_files[0].path, b"changed").unwrap();
        assert!(validate_deployment(&f.deployment).is_err());
    }
    #[test]
    fn rejects_data_inside_release() {
        let mut f = Fixture::new();
        f.deployment.services[0].protected_paths = vec![f.deployment.package_root.clone()];
        assert!(validate_deployment(&f.deployment).is_err());
    }
    #[test]
    fn rejects_old_package_scope() {
        let mut f = Fixture::new();
        let p = f.deployment.package_root.join("release-manifest.json");
        let text = fs::read_to_string(&p).unwrap().replace(
            "novovm_unified_linux_release_v3",
            "novovm_native_linux_release_v2",
        );
        fs::write(&p, text).unwrap();
        f.deployment.expected_release_manifest_sha256 = hash(&p);
        assert!(validate_deployment(&f.deployment).is_err());
    }
    #[test]
    fn rejects_traversal_even_with_pinned_manifest() {
        let mut f = Fixture::new();
        let p = f.deployment.package_root.join("release-manifest.json");
        let text = fs::read_to_string(&p)
            .unwrap()
            .replace("bin/supervm-relay", "../config/relay.json");
        fs::write(&p, text).unwrap();
        f.deployment.expected_release_manifest_sha256 = hash(&p);
        assert!(validate_deployment(&f.deployment).is_err());
    }
    #[test]
    fn rejects_fresh_legacy_relay_but_preserves_explicit_existing_owner() {
        let mut f = Fixture::new();
        let p = f.service().unit_file.clone();
        let text = fs::read_to_string(&p)
            .unwrap()
            .replace(" --runtime duplex-v2", "");
        fs::write(&p, text).unwrap();
        f.deployment.services[0].unit_sha256 = hash(&p);
        assert!(validate_deployment(&f.deployment).is_err());
        f.deployment.services[0].existing = true;
        assert!(validate_deployment(&f.deployment).is_ok());
    }
    #[test]
    fn rejects_hidden_provision_hooks() {
        let mut f = Fixture::new();
        let p = f.service().unit_file.clone();
        let text = format!(
            "{}ExecStartPre=/bin/provision\n",
            fs::read_to_string(&p).unwrap()
        );
        fs::write(&p, text).unwrap();
        f.deployment.services[0].unit_sha256 = hash(&p);
        assert!(validate_deployment(&f.deployment).is_err());
    }
    fn credential_entries(source: &Path, extra: &str) -> BTreeMap<String, Vec<String>> {
        unit_values(&format!(
            "[Service]\nLoadCredentialEncrypted=owner-key:{}\n{extra}",
            source.display()
        ))
        .unwrap()
    }
    #[test]
    fn node_credential_plan_checks_metadata_not_decryption() {
        let mut f = Fixture::new();
        f.deployment.services[0].role = Role::Node;
        let source = f.root.join("data/owner-key.credential");
        // Deliberately NOT a valid encrypted credential. Metadata preflight
        // must neither decrypt it nor falsely qualify final-account custody.
        fs::write(&source, []).unwrap();
        assert!(validate_node_credential(f.service(), &credential_entries(&source, "")).is_ok());
        assert_eq!(fs::metadata(&source).unwrap().len(), 0);
        assert_eq!(fs::read_dir(f.root.join("data")).unwrap().count(), 1);
    }
    #[test]
    fn node_credential_rejects_missing_duplicate_and_override_sources() {
        let mut f = Fixture::new();
        f.deployment.services[0].role = Role::Node;
        let source = f.root.join("data/owner-key.credential");
        fs::write(&source, b"not a real credential").unwrap();
        assert!(validate_node_credential(f.service(), &BTreeMap::new()).is_err());
        for extra in [
            format!("LoadCredentialEncrypted=owner-key:{}\n", source.display()),
            "LoadCredentialEncrypted=\n".into(),
            "LoadCredentialEncrypted=another-key:/unreviewed\n".into(),
            "LoadCredential=owner-key:/unreviewed\n".into(),
            "LoadCredential = owner-key:/unreviewed\n".into(),
            "LoadCredential=\n".into(),
            "SetCredential=owner-key:unreviewed\n".into(),
            "SetCredentialEncrypted=owner-key:unreviewed\n".into(),
            "ImportCredential=owner-*\n".into(),
        ] {
            assert!(
                validate_node_credential(f.service(), &credential_entries(&source, &extra))
                    .is_err(),
                "accepted ambiguous credential directive: {extra}"
            );
        }
    }
    #[test]
    fn node_credential_rejects_unowned_or_ambiguous_paths() {
        let mut f = Fixture::new();
        f.deployment.services[0].role = Role::Node;
        let source = f.root.join("data/owner-key.credential");
        fs::write(&source, b"not a real credential").unwrap();
        for path in [
            PathBuf::from("relative.credential"),
            f.root.join("data/missing.credential"),
            f.root.join("data"),
            f.root.join("data/../data/owner-key.credential"),
            f.service().config_files[0].path.clone(),
        ] {
            assert!(
                validate_node_credential(f.service(), &credential_entries(&path, "")).is_err(),
                "accepted unowned or invalid source: {}",
                path.display()
            );
        }
        for value in [
            format!("another-key:{}", source.display()),
            format!("owner-key:\"{}\"", source.display()),
            format!("owner-key:{}%i", source.display()),
            "owner-key".into(),
            "owner-key:".into(),
        ] {
            let entries = BTreeMap::from([("LoadCredentialEncrypted".into(), vec![value])]);
            assert!(validate_node_credential(f.service(), &entries).is_err());
        }
    }
    #[test]
    fn credential_directive_changes_still_require_unit_hash_review() {
        let f = Fixture::new();
        let p = &f.service().unit_file;
        fs::write(
            p,
            format!(
                "{}LoadCredentialEncrypted=owner-key:/unreviewed\n",
                fs::read_to_string(p).unwrap()
            ),
        )
        .unwrap();
        assert!(validate_deployment(&f.deployment).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn node_credential_rejects_symlink_source_and_ancestor() {
        use std::os::unix::fs::symlink;
        let mut f = Fixture::new();
        f.deployment.services[0].role = Role::Node;
        let source = f.root.join("data/owner-key.credential");
        fs::write(&source, b"not a real credential").unwrap();
        let alias = f.root.join("data/alias.credential");
        symlink(&source, &alias).unwrap();
        assert!(validate_node_credential(f.service(), &credential_entries(&alias, "")).is_err());
        let directory = f.root.join("data/alias-directory");
        symlink(f.root.join("data"), &directory).unwrap();
        assert!(validate_node_credential(
            f.service(),
            &credential_entries(&directory.join("owner-key.credential"), "")
        )
        .is_err());
    }
    #[test]
    fn checks_installed_fragment_and_loaded_arguments() {
        let f = Fixture::new();
        let mut runner = FakeRunner::default();
        runner.outputs.push_back(observed(f.service(), ""));
        assert!(preflight_service(f.service(), false, &mut runner).is_ok());
        let mut changed = observed(f.service(), "");
        changed.stdout = changed
            .stdout
            .replace("--runtime duplex-v2", "--runtime legacy-json");
        runner.outputs.push_back(changed);
        assert!(preflight_service(f.service(), false, &mut runner).is_err());
    }
    #[test]
    fn refuses_unreviewed_dropins() {
        let f = Fixture::new();
        let mut runner = FakeRunner::default();
        runner.outputs.push_back(observed(
            f.service(),
            "DropInPaths=/etc/systemd/system/test.d/override.conf\n",
        ));
        assert!(preflight_service(f.service(), false, &mut runner).is_err());
    }
    #[test]
    fn refuses_stale_unit_and_loaded_environment_overrides() {
        let f = Fixture::new();
        for extra in [
            "NeedDaemonReload=yes\n",
            "Environment=NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1\n",
            "EnvironmentFiles=/etc/unreviewed.env (ignore_errors=no)\n",
            "ExecStartPre={ path=/bin/provision ; }\n",
        ] {
            let mut runner = FakeRunner::default();
            runner.outputs.push_back(observed(f.service(), extra));
            assert!(preflight_service(f.service(), false, &mut runner).is_err());
        }
    }
    #[test]
    fn install_refuses_existing_service() {
        let f = Fixture::new();
        let mut runner = FakeRunner::default();
        runner.outputs.push_back(observed(f.service(), ""));
        assert!(preflight_service(f.service(), true, &mut runner).is_err());
        assert_eq!(runner.calls.len(), 1);
        assert!(runner.calls[0].contains("show"));
    }
    #[test]
    fn all_targets_checked_before_any_start() {
        let f = Fixture::new();
        let mut runner = FakeRunner::default();
        runner.outputs.push_back(observed(f.service(), ""));
        runner.outputs.push_back(Captured {
            success: true,
            stdout: "LoadState=not-found\n".into(),
        });
        assert!(preflight_all(&[f.service(), f.service()], false, &mut runner).is_err());
        assert_eq!(runner.calls.len(), 2);
        assert!(runner.calls.iter().all(|call| call.contains("show")));
    }
    #[test]
    fn apply_is_explicit_and_conflicts_with_dry_run() {
        let cli = Cli::try_parse_from([
            "novovmctl",
            "lifecycle",
            "--deployment-manifest",
            "host.json",
            "--action",
            "install",
        ])
        .unwrap();
        let TopCommand::Lifecycle(args) = cli.command else {
            panic!()
        };
        assert!(!args.apply);
        assert!(Cli::try_parse_from([
            "novovmctl",
            "lifecycle",
            "--deployment-manifest",
            "host.json",
            "--apply",
            "--dry-run"
        ])
        .is_err());
    }
    #[test]
    fn legacy_force_does_not_override_native_contract() {
        let cli = Cli::try_parse_from([
            "novovmctl",
            "lifecycle",
            "--deployment-manifest",
            "host.json",
            "--force",
        ])
        .unwrap();
        let TopCommand::Lifecycle(args) = cli.command else {
            panic!()
        };
        assert!(reject_legacy_options(&args).is_err());
    }
    #[test]
    fn no_shell_or_option_unit_injection() {
        assert!(!safe_unit_name("--help.service"));
        assert!(!safe_unit_name("x;shutdown.service"));
        assert!(!safe_unit_name("../../x.service"));
        assert!(safe_unit_name("novonet-mailbox.service"));
    }
    #[test]
    fn requires_existing_native_state_and_no_duplicate_overrides() {
        let good =
            "NOVOVM_NODE_MODE=native_execution_tick\nNOVOVM_NATIVE_EXECUTION_START=existing\n";
        assert!(parse_node_environment(good).is_ok());
        assert!(parse_node_environment(&good.replace("existing", "create")).is_err());
        assert!(
            parse_node_environment(&format!("{good}NOVOVM_NATIVE_EXECUTION_START=create\n"))
                .is_err()
        );
        assert!(
            parse_node_environment(&format!("{good}NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1\n"))
                .is_err()
        );
    }
    #[test]
    fn native_and_environment_library_must_match_even_if_both_are_pinned() {
        let f = Fixture::new();
        assert!(same_library_binding(&f.service().binary, &f.service().binary).is_ok());
        assert!(
            same_library_binding(&f.service().binary, &f.root.join("other/libaoem_ffi.so"))
                .is_err()
        );
    }
    #[test]
    fn pins_aoem_root_plugin_and_variant_to_this_package() {
        let f = Fixture::new();
        let root = f.deployment.package_root.join("aoem");
        let plugins = root.join("linux/core/plugins");
        fs::create_dir_all(&plugins).unwrap();
        let env = BTreeMap::from([
            ("NOVOVM_AOEM_ROOT".into(), root.to_str().unwrap().into()),
            (
                "NOVOVM_AOEM_PLUGIN_DIR".into(),
                plugins.to_str().unwrap().into(),
            ),
            ("NOVOVM_AOEM_VARIANT".into(), "core".into()),
        ]);
        assert!(validate_aoem_layout(&env, &f.deployment.package_root).is_ok());
        for (key, value) in [
            ("NOVOVM_AOEM_ROOT", f.root.to_str().unwrap()),
            ("NOVOVM_AOEM_PLUGIN_DIR", f.root.to_str().unwrap()),
            ("NOVOVM_AOEM_VARIANT", "persist"),
        ] {
            let mut changed = env.clone();
            changed.insert(key.into(), value.into());
            assert!(validate_aoem_layout(&changed, &f.deployment.package_root).is_err());
        }
    }
    #[test]
    fn install_name_cannot_differ_from_linked_unit() {
        let mut f = Fixture::new();
        f.deployment.services[0].name = "different.service".into();
        assert!(validate_deployment(&f.deployment).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_artifact_ancestor() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let alias = f.root.join("alias");
        symlink(&f.deployment.package_root, &alias).unwrap();
        assert!(plain_path(&alias.join("bin/supervm-relay"), false).is_err());
    }
}
