#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_node::tx_ingress::{
    dispatch_and_persist_nov_execution_request_with_store_path_v1,
    load_nov_native_execution_store_v1, run_nov_native_call_from_params_with_store_path_v1,
    save_nov_native_execution_store_v1, NovExecutionRequestTargetV1, NovExecutionRequestV1,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_STORE_PATH: &str = "artifacts/mainline/p2d-run-phase/native-execution-store.json";

#[derive(Debug, Clone)]
struct CliOptions {
    store_path: PathBuf,
    reset: bool,
}

fn parse_cli_options() -> Result<CliOptions> {
    let mut store_path = PathBuf::from(DEFAULT_STORE_PATH);
    let mut reset = true;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--store-path" => {
                idx += 1;
                let Some(raw) = args.get(idx) else {
                    bail!("--store-path requires a value");
                };
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    bail!("--store-path cannot be empty");
                }
                store_path = PathBuf::from(trimmed);
            }
            "--no-reset" => {
                reset = false;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: supervm-mainline-p2d-sample-inject [--store-path <path>] [--no-reset]"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
        idx += 1;
    }
    Ok(CliOptions { store_path, reset })
}

fn now_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0)
}

fn reset_store(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("remove existing store failed: {}", path.display()))?;
    }
    Ok(())
}

fn with_store_mut<F>(path: &Path, f: F) -> Result<()>
where
    F: FnOnce(&mut novovm_node::tx_ingress::NovNativeExecutionStoreV1) -> Result<()>,
{
    let mut store = load_nov_native_execution_store_v1(path)
        .with_context(|| format!("load store failed: {}", path.display()))?;
    f(&mut store)?;
    save_nov_native_execution_store_v1(path, &store)
        .with_context(|| format!("save store failed: {}", path.display()))?;
    Ok(())
}

fn seed_success_case(path: &Path) -> Result<()> {
    with_store_mut(path, |store| {
        store
            .module_state
            .clearing_nov_liquidity
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 1_000_000);
        store.module_state.fee_oracle_updated_unix_ms = now_unix_millis();
        store.module_state.fee_oracle_source = "runtime_oracle".to_string();
        store.module_state.clearing_enabled = true;
        store.module_state.clearing_static_amm_pools.clear();
        Ok(())
    })
}

fn seed_insufficient_liquidity_case(path: &Path) -> Result<()> {
    with_store_mut(path, |store| {
        store
            .module_state
            .clearing_nov_liquidity
            .insert("USDT".to_string(), 1);
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 1_000_000);
        store.module_state.fee_oracle_updated_unix_ms = now_unix_millis();
        store.module_state.fee_oracle_source = "runtime_oracle".to_string();
        store.module_state.clearing_enabled = true;
        store.module_state.clearing_static_amm_pools.clear();
        Ok(())
    })
}

fn seed_slippage_case(path: &Path) -> Result<()> {
    with_store_mut(path, |store| {
        store
            .module_state
            .clearing_nov_liquidity
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 100_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 3_000_000);
        store.module_state.fee_oracle_updated_unix_ms = now_unix_millis();
        store.module_state.fee_oracle_source = "runtime_oracle".to_string();
        store.module_state.clearing_enabled = true;
        store.module_state.clearing_static_amm_pools.clear();
        Ok(())
    })
}

fn build_request(
    tx_byte: u8,
    nonce: u64,
    max_pay_amount: u128,
    slippage_bps: u32,
) -> Result<NovExecutionRequestV1> {
    Ok(NovExecutionRequestV1 {
        tx_hash: [tx_byte; 32],
        chain_id: 9_901,
        caller: vec![0x33; 20],
        target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
        method: "deposit_reserve".to_string(),
        args: serde_json::to_vec(&json!({
            "asset": "USDT",
            "amount": 2u64
        }))
        .context("encode request args failed")?,
        fee_pay_asset: "USDT".to_string(),
        fee_max_pay_amount: max_pay_amount,
        fee_slippage_bps: slippage_bps,
        gas_like_limit: Some(90_000),
        nonce,
    })
}

fn failure_code(receipt: &novovm_node::tx_ingress::NovNativeExecutionReceiptV1) -> String {
    receipt
        .failure_reason
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn ensure_failure_prefix(
    receipt: &novovm_node::tx_ingress::NovNativeExecutionReceiptV1,
    prefix: &str,
) -> Result<()> {
    if receipt.status {
        bail!(
            "expected failure `{prefix}` but receipt succeeded (tx_hash={})",
            receipt.tx_hash
        );
    }
    let code = failure_code(receipt);
    if !code.starts_with(prefix) {
        bail!(
            "expected failure prefix `{prefix}`, got `{code}` (tx_hash={})",
            receipt.tx_hash
        );
    }
    Ok(())
}

fn dispatch_case(
    path: &Path,
    request: &NovExecutionRequestV1,
) -> Result<novovm_node::tx_ingress::NovNativeExecutionReceiptV1> {
    dispatch_and_persist_nov_execution_request_with_store_path_v1(path, request)
        .with_context(|| format!("dispatch failed for tx byte {:02x}", request.tx_hash[0]))
}

fn run_case_success(path: &Path) -> Result<novovm_node::tx_ingress::NovNativeExecutionReceiptV1> {
    seed_success_case(path)?;
    let request = build_request(0xa1, 1, 10_000, 10_000)?;
    let receipt = dispatch_case(path, &request)?;
    if !receipt.status {
        bail!(
            "expected success for case=success, failure_reason={:?}",
            receipt.failure_reason
        );
    }
    Ok(receipt)
}

fn run_case_insufficient(
    path: &Path,
) -> Result<novovm_node::tx_ingress::NovNativeExecutionReceiptV1> {
    seed_insufficient_liquidity_case(path)?;
    let request = build_request(0xa2, 2, 100, 100)?;
    let receipt = dispatch_case(path, &request)?;
    ensure_failure_prefix(&receipt, "fee.clearing.insufficient_liquidity")?;
    Ok(receipt)
}

fn run_case_slippage(path: &Path) -> Result<novovm_node::tx_ingress::NovNativeExecutionReceiptV1> {
    seed_slippage_case(path)?;
    let request = build_request(0xa3, 3, 100, 50)?;
    let receipt = dispatch_case(path, &request)?;
    ensure_failure_prefix(&receipt, "fee.clearing.slippage_exceeded")?;
    Ok(receipt)
}

fn query_metrics(path: &Path) -> Result<serde_json::Value> {
    let out = run_nov_native_call_from_params_with_store_path_v1(
        &json!({
            "target": {"kind": "native_module", "id": "treasury"},
            "method": "get_clearing_metrics_summary",
            "args": {}
        }),
        Some(path),
    )?;
    Ok(out.get("result").cloned().unwrap_or_else(|| json!({})))
}

fn main() -> Result<()> {
    let options = parse_cli_options()?;
    if options.reset {
        reset_store(options.store_path.as_path())?;
    }

    let success_receipt = run_case_success(options.store_path.as_path())?;
    let insufficient_receipt = run_case_insufficient(options.store_path.as_path())?;
    let slippage_receipt = run_case_slippage(options.store_path.as_path())?;
    let metrics = query_metrics(options.store_path.as_path())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema": "supervm-mainline-p2d-sample-inject/v1",
            "store_path": options.store_path.display().to_string(),
            "cases": {
                "success": {
                    "tx_hash": success_receipt.tx_hash,
                    "status": success_receipt.status,
                    "failure_code": failure_code(&success_receipt),
                },
                "insufficient_liquidity": {
                    "tx_hash": insufficient_receipt.tx_hash,
                    "status": insufficient_receipt.status,
                    "failure_code": failure_code(&insufficient_receipt),
                },
                "slippage_exceeded": {
                    "tx_hash": slippage_receipt.tx_hash,
                    "status": slippage_receipt.status,
                    "failure_code": failure_code(&slippage_receipt),
                }
            },
            "metrics_snapshot": metrics,
        }))?
    );

    Ok(())
}
