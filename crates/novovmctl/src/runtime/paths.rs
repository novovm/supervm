use std::path::{Path, PathBuf};

use crate::error::CtlError;

pub fn resolve_policy_binary(explicit: Option<&str>) -> Result<String, CtlError> {
    if let Some(path) = explicit {
        validate_exists(path, "novovm-rollout-policy")?;
        return Ok(path.to_string());
    }

    let candidates = default_policy_candidates();
    first_existing(&candidates).ok_or_else(|| {
        CtlError::BinaryNotFound(format!(
            "novovm-rollout-policy not found; tried: {}",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

pub fn resolve_node_binary(explicit: Option<&str>) -> Result<String, CtlError> {
    if let Some(path) = explicit {
        validate_exists(path, "novovm-node")?;
        return Ok(path.to_string());
    }

    let candidates = default_node_candidates();
    first_existing(&candidates).ok_or_else(|| {
        CtlError::BinaryNotFound(format!(
            "novovm-node not found; tried: {}",
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

fn default_policy_candidates() -> Vec<PathBuf> {
    let mut candidates = default_external_target_candidates("novovm-rollout-policy");
    candidates.extend([
        PathBuf::from("target/release/novovm-rollout-policy.exe"),
        PathBuf::from("target/release/novovm-rollout-policy"),
        PathBuf::from("target/debug/novovm-rollout-policy.exe"),
        PathBuf::from("target/debug/novovm-rollout-policy"),
        PathBuf::from("crates/novovm-rollout-policy/target/release/novovm-rollout-policy.exe"),
        PathBuf::from("crates/novovm-rollout-policy/target/release/novovm-rollout-policy"),
        PathBuf::from("crates/novovm-rollout-policy/target/debug/novovm-rollout-policy.exe"),
        PathBuf::from("crates/novovm-rollout-policy/target/debug/novovm-rollout-policy"),
    ]);
    candidates
}

fn default_node_candidates() -> Vec<PathBuf> {
    let mut candidates = default_external_target_candidates("novovm-node");
    candidates.extend([
        PathBuf::from("target/release/novovm-node.exe"),
        PathBuf::from("target/release/novovm-node"),
        PathBuf::from("target/debug/novovm-node.exe"),
        PathBuf::from("target/debug/novovm-node"),
        PathBuf::from("crates/novovm-node/target/release/novovm-node.exe"),
        PathBuf::from("crates/novovm-node/target/release/novovm-node"),
        PathBuf::from("crates/novovm-node/target/debug/novovm-node.exe"),
        PathBuf::from("crates/novovm-node/target/debug/novovm-node"),
    ]);
    candidates
}

fn default_external_target_candidates(binary_name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        candidates.extend([
            PathBuf::from(&target_dir)
                .join("release")
                .join(format!("{binary_name}.exe")),
            PathBuf::from(&target_dir).join("release").join(binary_name),
            PathBuf::from(&target_dir)
                .join("debug")
                .join(format!("{binary_name}.exe")),
            PathBuf::from(&target_dir).join("debug").join(binary_name),
        ]);
    }
    candidates.extend([
        PathBuf::from("D:/cargo-target-supervm/release").join(format!("{binary_name}.exe")),
        PathBuf::from("D:/cargo-target-supervm/release").join(binary_name),
        PathBuf::from("D:/cargo-target-supervm/debug").join(format!("{binary_name}.exe")),
        PathBuf::from("D:/cargo-target-supervm/debug").join(binary_name),
    ]);
    candidates
}

fn first_existing(candidates: &[PathBuf]) -> Option<String> {
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| p.display().to_string())
}

fn validate_exists(path: &str, name: &str) -> Result<(), CtlError> {
    if Path::new(path).exists() {
        Ok(())
    } else {
        Err(CtlError::BinaryNotFound(format!(
            "{name} explicit path not found: {path}"
        )))
    }
}
