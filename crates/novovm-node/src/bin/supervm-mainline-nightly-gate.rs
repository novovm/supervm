#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use chrono::Utc;
use novovm_network::load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1;
use novovm_node::mainline_soak::{
    apply_mainline_soak_threshold_env_overrides_v1, default_mainline_soak_duration_seconds_v1,
    default_mainline_soak_report_path_v1, default_mainline_soak_snapshot_path_v1,
    default_mainline_soak_thresholds_v1, run_mainline_soak_v1, write_mainline_soak_report_v1,
    MainlineNightlySoakGateReportV1, MainlineNightlySoakProfileResultV1, MainlineSoakConfigV1,
    MAINLINE_NIGHTLY_SOAK_GATE_REPORT_SCHEMA_V1,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn string_env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_u64_env(name: &str) -> Result<Option<u64>> {
    let Some(raw) = string_env_nonempty(name) else {
        return Ok(None);
    };
    let parsed = raw
        .parse::<u64>()
        .with_context(|| format!("invalid {name}: '{raw}'"))?;
    Ok(Some(parsed))
}

fn parse_bool_env(name: &str, default: bool) -> bool {
    let Some(raw) = std::env::var(name).ok() else {
        return default;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn resolve_chain_id_v1(snapshot_path: &Path) -> Result<u64> {
    if let Some(chain_id) = parse_u64_env("NOVOVM_MAINLINE_NIGHTLY_SOAK_CHAIN_ID")? {
        return Ok(chain_id);
    }
    if let Some(chain_id) = parse_u64_env("NOVOVM_MAINLINE_QUERY_CHAIN_ID")? {
        return Ok(chain_id);
    }
    if let Ok(snapshot) =
        load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1(snapshot_path)
    {
        return Ok(snapshot.chain_id);
    }
    Ok(1)
}

fn run_step(name: &str, program: &str, args: &[&str]) -> Result<()> {
    println!("==> {name}");
    let mut cmd = Command::new(program);
    cmd.args(args);
    if program.eq_ignore_ascii_case("cargo") {
        cmd.env("CARGO_TARGET_DIR", resolve_gate_target_dir_v1());
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to run step: {name}"))?;
    if !status.success() {
        match status.code() {
            Some(code) => bail!("Step failed: {name} (exit={code})"),
            None => bail!("Step failed: {name} (terminated by signal)"),
        }
    }
    Ok(())
}

fn resolve_gate_target_dir_v1() -> String {
    if let Ok(raw) = std::env::var("CARGO_TARGET_DIR") {
        let trimmed = raw.trim();
        // On non-Windows runners, a Windows-style value like `D:\...`
        // propagates `:` into LD_LIBRARY_PATH join and breaks cargo.
        let linux_safe = cfg!(windows) || !trimmed.contains(':');
        if !trimmed.is_empty() && linux_safe {
            return trimmed.to_string();
        }
    }
    if cfg!(windows) {
        "D:\\cargo-target-supervm-gate".to_string()
    } else {
        "target/cargo-target-supervm-gate".to_string()
    }
}

fn profile_list_v1() -> Vec<String> {
    let raw = string_env_nonempty("NOVOVM_MAINLINE_NIGHTLY_SOAK_PROFILES")
        .unwrap_or_else(|| "6h,24h".to_string());
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

fn profile_duration_key_v1(profile: &str) -> String {
    format!(
        "NOVOVM_MAINLINE_NIGHTLY_SOAK_{}_DURATION_SECONDS",
        profile.to_ascii_uppercase()
    )
}

fn profile_interval_key_v1(profile: &str) -> String {
    format!(
        "NOVOVM_MAINLINE_NIGHTLY_SOAK_{}_INTERVAL_SECONDS",
        profile.to_ascii_uppercase()
    )
}

fn profile_report_key_v1(profile: &str) -> String {
    format!(
        "NOVOVM_MAINLINE_NIGHTLY_SOAK_{}_REPORT_PATH",
        profile.to_ascii_uppercase()
    )
}

fn profile_threshold_prefix_v1(profile: &str) -> String {
    format!(
        "NOVOVM_MAINLINE_NIGHTLY_SOAK_{}_",
        profile.to_ascii_uppercase()
    )
}

fn resolve_profile_config_v1(
    profile: &str,
    chain_id: u64,
    snapshot_path: &Path,
) -> Result<MainlineSoakConfigV1> {
    let duration_seconds = parse_u64_env(profile_duration_key_v1(profile).as_str())?
        .unwrap_or_else(|| default_mainline_soak_duration_seconds_v1(profile));
    let sample_interval_seconds =
        parse_u64_env(profile_interval_key_v1(profile).as_str())?.unwrap_or(60);
    let report_path = string_env_nonempty(profile_report_key_v1(profile).as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if profile == "6h" || profile == "24h" {
                PathBuf::from(format!("artifacts/mainline/mainline-soak-{profile}.json"))
            } else {
                default_mainline_soak_report_path_v1(profile)
            }
        });

    let mut thresholds = default_mainline_soak_thresholds_v1(profile);
    apply_mainline_soak_threshold_env_overrides_v1("NOVOVM_MAINLINE_SOAK_", &mut thresholds)?;
    apply_mainline_soak_threshold_env_overrides_v1(
        profile_threshold_prefix_v1(profile).as_str(),
        &mut thresholds,
    )?;

    Ok(MainlineSoakConfigV1 {
        profile: profile.to_string(),
        chain_id,
        duration_seconds,
        sample_interval_seconds,
        snapshot_path: snapshot_path.to_path_buf(),
        report_path,
        thresholds,
    })
}

fn main() -> Result<()> {
    let run_mainline_gate = parse_bool_env("NOVOVM_MAINLINE_NIGHTLY_RUN_MAINLINE_GATE", false);
    if run_mainline_gate {
        run_step(
            "run supervm-mainline-gate",
            "cargo",
            &["run", "-p", "novovm-node", "--bin", "supervm-mainline-gate"],
        )?;
    }

    let snapshot_path = string_env_nonempty("NOVOVM_MAINLINE_NIGHTLY_SOAK_SNAPSHOT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_mainline_soak_snapshot_path_v1);
    let chain_id = resolve_chain_id_v1(snapshot_path.as_path())?;
    let profiles = profile_list_v1();
    if profiles.is_empty() {
        bail!("NOVOVM_MAINLINE_NIGHTLY_SOAK_PROFILES resolved to an empty profile list");
    }

    let mut profile_results = Vec::new();
    let mut overall_pass = true;
    for profile in profiles {
        let config =
            resolve_profile_config_v1(profile.as_str(), chain_id, snapshot_path.as_path())?;
        println!(
            "nightly soak profile start: profile={} duration={}s interval={}s snapshot={} report={}",
            config.profile,
            config.duration_seconds,
            config.sample_interval_seconds,
            config.snapshot_path.display(),
            config.report_path.display()
        );
        let report = run_mainline_soak_v1(&config)?;
        write_mainline_soak_report_v1(config.report_path.as_path(), &report)?;
        println!(
            "nightly soak profile done: profile={} pass={} sample_count={} elapsed={}s violations={} report={}",
            report.profile,
            report.evaluation.pass,
            report.sample_count,
            report.observed_elapsed_seconds,
            report.evaluation.violation_count,
            config.report_path.display()
        );
        overall_pass &= report.evaluation.pass;
        profile_results.push(MainlineNightlySoakProfileResultV1 {
            profile: report.profile.clone(),
            report_path: config.report_path.display().to_string(),
            requested_duration_seconds: report.requested_duration_seconds,
            observed_elapsed_seconds: report.observed_elapsed_seconds,
            sample_interval_seconds: report.sample_interval_seconds,
            sample_count: report.sample_count,
            pass: report.evaluation.pass,
            violation_count: report.evaluation.violation_count,
        });
    }

    let nightly_report_path = string_env_nonempty("NOVOVM_MAINLINE_NIGHTLY_SOAK_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("artifacts/mainline/mainline-nightly-soak-gate-report.json")
        });
    if let Some(parent) = nightly_report_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create nightly soak report directory: {}", parent.display())
            })?;
        }
    }

    let nightly_report = MainlineNightlySoakGateReportV1 {
        schema: MAINLINE_NIGHTLY_SOAK_GATE_REPORT_SCHEMA_V1,
        generated_utc: Utc::now().to_rfc3339(),
        chain_id,
        snapshot_path: snapshot_path.display().to_string(),
        run_mainline_gate,
        profile_results,
        overall_pass,
    };
    let encoded =
        serde_json::to_string_pretty(&nightly_report).context("encode nightly soak report")?;
    fs::write(&nightly_report_path, format!("{encoded}\n")).with_context(|| {
        format!(
            "write nightly soak report: {}",
            nightly_report_path.display()
        )
    })?;
    println!(
        "nightly soak gate done: pass={} report={}",
        nightly_report.overall_pass,
        nightly_report_path.display()
    );

    if !nightly_report.overall_pass {
        bail!(
            "nightly soak gate failed: see {}",
            nightly_report_path.display()
        );
    }
    Ok(())
}
