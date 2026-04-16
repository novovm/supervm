#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use novovm_network::load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1;
use novovm_node::mainline_soak::{
    apply_mainline_soak_threshold_env_overrides_v1, default_mainline_soak_duration_seconds_v1,
    default_mainline_soak_report_path_v1, default_mainline_soak_snapshot_path_v1,
    default_mainline_soak_thresholds_v1, run_mainline_soak_v1, write_mainline_soak_report_v1,
    MainlineSoakConfigV1,
};
use std::path::PathBuf;

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

fn resolve_chain_id_v1(snapshot_path: &std::path::Path) -> Result<u64> {
    if let Some(chain_id) = parse_u64_env("NOVOVM_MAINLINE_SOAK_CHAIN_ID")? {
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

fn main() -> Result<()> {
    let profile = string_env_nonempty("NOVOVM_MAINLINE_SOAK_PROFILE")
        .unwrap_or_else(|| "6h".to_string())
        .trim()
        .to_ascii_lowercase();
    let snapshot_path = string_env_nonempty("NOVOVM_MAINLINE_SOAK_SNAPSHOT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_mainline_soak_snapshot_path_v1);
    let duration_seconds = parse_u64_env("NOVOVM_MAINLINE_SOAK_DURATION_SECONDS")?
        .unwrap_or_else(|| default_mainline_soak_duration_seconds_v1(profile.as_str()));
    let sample_interval_seconds =
        parse_u64_env("NOVOVM_MAINLINE_SOAK_INTERVAL_SECONDS")?.unwrap_or(60);
    let chain_id = resolve_chain_id_v1(snapshot_path.as_path())?;
    let report_path = string_env_nonempty("NOVOVM_MAINLINE_SOAK_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_mainline_soak_report_path_v1(profile.as_str()));

    let mut thresholds = default_mainline_soak_thresholds_v1(profile.as_str());
    apply_mainline_soak_threshold_env_overrides_v1("NOVOVM_MAINLINE_SOAK_", &mut thresholds)?;

    let config = MainlineSoakConfigV1 {
        profile: profile.clone(),
        chain_id,
        duration_seconds,
        sample_interval_seconds,
        snapshot_path: snapshot_path.clone(),
        report_path: report_path.clone(),
        thresholds,
    };
    println!(
        "mainline soak start: profile={} chain_id={} duration={}s interval={}s snapshot={} report={}",
        profile,
        chain_id,
        duration_seconds,
        sample_interval_seconds,
        snapshot_path.display(),
        report_path.display()
    );
    let report = run_mainline_soak_v1(&config)?;
    write_mainline_soak_report_v1(report_path.as_path(), &report)?;
    println!(
        "mainline soak done: profile={} pass={} sample_count={} elapsed={}s report={}",
        report.profile,
        report.evaluation.pass,
        report.sample_count,
        report.observed_elapsed_seconds,
        report_path.display()
    );
    Ok(())
}
