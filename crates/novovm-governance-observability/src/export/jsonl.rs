#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::events::{GovernanceEvent, GovernanceEventEnvelope};

pub const GOVERNANCE_ARTIFACT_DIR_DEFAULT: &str = "artifacts/governance";
pub const GOVERNANCE_EVENT_FILE_PREFIX: &str = "governance-events-";
pub const GOVERNANCE_EVENT_FILE_SUFFIX: &str = ".jsonl";

#[must_use]
pub fn default_governance_events_dir() -> PathBuf {
    std::env::var("NOVOVM_GOVERNANCE_EVENTS_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(GOVERNANCE_ARTIFACT_DIR_DEFAULT))
}

#[must_use]
pub fn default_governance_events_path() -> PathBuf {
    if let Ok(path) = std::env::var("NOVOVM_GOVERNANCE_EVENTS_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let date = Utc::now().format("%Y-%m-%d");
    default_governance_events_dir().join(format!(
        "{}{}{}",
        GOVERNANCE_EVENT_FILE_PREFIX, date, GOVERNANCE_EVENT_FILE_SUFFIX
    ))
}

pub fn append_governance_event(
    path: &Path,
    source: &str,
    event: GovernanceEvent,
) -> Result<GovernanceEventEnvelope> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "create governance event parent dir failed: {}",
                parent.display()
            )
        })?;
    }

    let envelope = GovernanceEventEnvelope::new(source.to_string(), event);
    let encoded = serde_json::to_string(&envelope).context("serialize governance event failed")?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open governance event file failed: {}", path.display()))?;
    writeln!(file, "{encoded}")
        .with_context(|| format!("append governance event failed: {}", path.display()))?;

    Ok(envelope)
}

pub fn append_governance_event_auto(
    source: &str,
    event: GovernanceEvent,
) -> Result<GovernanceEventEnvelope> {
    append_governance_event(default_governance_events_path().as_path(), source, event)
}

pub fn discover_governance_event_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)
        .with_context(|| format!("read governance events dir failed: {}", dir.display()))?
    {
        let entry = entry.with_context(|| {
            format!("read governance events dir entry failed: {}", dir.display())
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if name.starts_with(GOVERNANCE_EVENT_FILE_PREFIX)
            && name.ends_with(GOVERNANCE_EVENT_FILE_SUFFIX)
        {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

pub fn load_governance_events_from_file(path: &Path) -> Result<Vec<GovernanceEventEnvelope>> {
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .with_context(|| format!("open governance event file failed: {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!(
                "read governance event line failed: path={} line={}",
                path.display(),
                line_index + 1
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed =
            serde_json::from_str::<GovernanceEventEnvelope>(trimmed).with_context(|| {
                format!(
                    "parse governance event failed: path={} line={}",
                    path.display(),
                    line_index + 1
                )
            })?;
        out.push(parsed);
    }
    Ok(out)
}

pub fn load_governance_events_from_paths(
    paths: &[PathBuf],
) -> Result<Vec<GovernanceEventEnvelope>> {
    let mut out = Vec::new();
    for path in paths {
        out.extend(load_governance_events_from_file(path.as_path())?);
    }
    out.sort_by_key(|item| item.at_unix_ms);
    Ok(out)
}
