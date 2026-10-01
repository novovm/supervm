#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use novovm_node::mainline_duty_report::{
    default_mainline_duty_report_markdown_path_v1, default_mainline_nightly_gate_report_path_v1,
    default_mainline_soak_24h_report_path_v1, default_mainline_soak_6h_report_path_v1,
    load_mainline_duty_input_from_paths_v1, render_mainline_duty_markdown_v1,
    write_mainline_duty_markdown_v1,
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

fn main() -> Result<()> {
    let nightly_path = string_env_nonempty("NOVOVM_MAINLINE_DUTY_NIGHTLY_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_mainline_nightly_gate_report_path_v1);
    let soak_6h_path = string_env_nonempty("NOVOVM_MAINLINE_DUTY_SOAK_6H_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_mainline_soak_6h_report_path_v1);
    let soak_24h_path = string_env_nonempty("NOVOVM_MAINLINE_DUTY_SOAK_24H_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_mainline_soak_24h_report_path_v1);
    let out_path = string_env_nonempty("NOVOVM_MAINLINE_DUTY_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_mainline_duty_report_markdown_path_v1);
    let owner = string_env_nonempty("NOVOVM_MAINLINE_DUTY_OWNER").unwrap_or_default();
    let workflow_run_url = string_env_nonempty("NOVOVM_MAINLINE_DUTY_WORKFLOW_RUN_URL");

    let input = load_mainline_duty_input_from_paths_v1(
        nightly_path.as_path(),
        soak_6h_path.as_path(),
        soak_24h_path.as_path(),
        owner,
        workflow_run_url,
    )
    .with_context(|| {
        format!(
            "load duty inputs failed (nightly={},6h={},24h={})",
            nightly_path.display(),
            soak_6h_path.display(),
            soak_24h_path.display()
        )
    })?;
    let output = render_mainline_duty_markdown_v1(&input);
    write_mainline_duty_markdown_v1(out_path.as_path(), output.markdown.as_str())?;
    println!(
        "mainline duty report generated: level={} primary_issue={} path={}",
        output.level.as_str(),
        output.primary_issue,
        out_path.display()
    );
    Ok(())
}
