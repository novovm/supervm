#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_node::governance_surface::{
    default_mainline_governance_store_path, is_mainline_governance_query_method,
};
use novovm_node::mainline_query::{
    default_mainline_query_store_path, default_mainline_runtime_snapshot_path,
    is_mainline_native_execution_query_method, is_mainline_runtime_query_method,
    mainline_query_method_from_env, mainline_query_params_from_env, run_mainline_query_from_path,
};
use novovm_node::tx_ingress::nov_native_execution_store_path_v1;
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
    let method = mainline_query_method_from_env()
        .ok_or_else(|| anyhow::anyhow!("NOVOVM_MAINLINE_QUERY_METHOD is required"))?;
    let params =
        mainline_query_params_from_env().context("parse NOVOVM_MAINLINE_QUERY_PARAMS failed")?;
    let store_path = string_env_nonempty("NOVOVM_MAINLINE_QUERY_STORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_mainline_query_store_path);
    let runtime_method = is_mainline_runtime_query_method(&method);
    let native_execution_method = is_mainline_native_execution_query_method(&method);
    let governance_method = is_mainline_governance_query_method(&method);
    if !runtime_method && !native_execution_method && !governance_method && !store_path.exists() {
        bail!(
            "mainline canonical store does not exist: {}",
            store_path.display()
        );
    }
    let out = run_mainline_query_from_path(&store_path, &method, &params)?;
    let query_path = if runtime_method {
        default_mainline_runtime_snapshot_path()
    } else if native_execution_method {
        string_env_nonempty("NOVOVM_MAINLINE_NATIVE_EXECUTION_STORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(nov_native_execution_store_path_v1)
    } else if governance_method {
        string_env_nonempty("NOVOVM_MAINLINE_GOVERNANCE_STORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(default_mainline_governance_store_path)
    } else {
        store_path.clone()
    };
    let query_source = if runtime_method {
        "runtime_snapshot"
    } else if native_execution_method {
        "native_execution_store"
    } else if governance_method {
        "governance_surface_store"
    } else {
        "canonical_store"
    };
    println!(
        "mainline_query_out: source={} path={} method={} found={} count={}",
        query_source,
        query_path.display(),
        method,
        out.get("found")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        out.get("count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&out).context("encode query output failed")?
    );
    Ok(())
}
