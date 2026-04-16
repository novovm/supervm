use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const SAMPLE_SCHEMA_V1: &str = "supervm-e2e-geth-parity-sample/v1";
const FAILURE_CLASSIFICATION_CONTRACT_V1: &str = "novovm-exec/v1";
const GETH_REPO_ROOT_ENV_V1: &str = "NOVOVM_GETH_REPO_ROOT";

#[derive(Debug)]
struct CliArgsV1 {
    geth_repo_root: PathBuf,
    source_dir: PathBuf,
    output_dir: PathBuf,
    chain_id: u64,
    dry_run: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GethExportLogInputV1 {
    address: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GethExportReceiptInputV1 {
    #[serde(rename = "blockNumber", default)]
    block_number: Option<String>,
    #[serde(rename = "contractAddress", default)]
    contract_address: Option<String>,
    #[serde(rename = "cumulativeGasUsed", default)]
    cumulative_gas_used: Option<String>,
    #[serde(rename = "gasUsed", default)]
    gas_used: Option<String>,
    #[serde(rename = "logs", default)]
    logs: Vec<GethExportLogInputV1>,
    #[serde(rename = "status", default)]
    status: Option<String>,
    #[serde(rename = "transactionIndex", default)]
    transaction_index: Option<String>,
    #[serde(rename = "type", default)]
    tx_type: Option<String>,
    #[serde(rename = "revertData", default)]
    revert_data: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GethParitySampleFileOutV1 {
    schema: &'static str,
    name: String,
    chain_id: u64,
    store_format: String,
    store_path: String,
    expected: GethParityExpectedOutV1,
}

#[derive(Debug, Clone, Serialize)]
struct GethParityExpectedOutV1 {
    block: GethParityExpectedBlockOutV1,
    receipts: Vec<GethParityExpectedReceiptOutV1>,
    logs_canonical: GethParityExpectedLogsViewOutV1,
    logs_noncanonical: GethParityExpectedLogsViewOutV1,
    typed_failures: Vec<GethParityExpectedTypedFailureOutV1>,
}

#[derive(Debug, Clone, Serialize)]
struct GethParityExpectedBlockOutV1 {
    number: String,
    tx_count: u64,
    tx_types: Vec<String>,
    tx_statuses: Vec<String>,
    tx_contract_addresses: Vec<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
struct GethParityExpectedReceiptOutV1 {
    tx_index: usize,
    status: String,
    tx_type: String,
    gas_used: String,
    cumulative_gas_used: String,
    contract_address: Option<String>,
    revert_data: Option<String>,
    log_count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct GethParityExpectedLogsViewOutV1 {
    count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_removed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_log_ownership: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GethParityExpectedTypedFailureOutV1 {
    tx_index: usize,
    failure_classification_contract: &'static str,
    status: String,
    contract_address_null: bool,
}

#[derive(Debug, Clone)]
struct NormalizedReceiptV1 {
    tx_index: usize,
    status: String,
    status_ok: bool,
    tx_type: String,
    gas_used: String,
    cumulative_gas_used: String,
    contract_address: Option<String>,
    revert_data: Option<String>,
    effective_log_count: u64,
    first_log_address: Option<String>,
}

fn print_usage_v1() {
    println!("supervm-mainline-geth-sample-sync");
    println!();
    println!("Usage:");
    println!("  cargo run -p novovm-node --bin supervm-mainline-geth-sample-sync -- [options]");
    println!();
    println!("Options:");
    println!("  --source-dir <path>   Source geth export dir (default: $NOVOVM_GETH_REPO_ROOT/internal/ethapi/testdata)");
    println!("  --output-dir <path>   Output sample dir (default: crates/novovm-node/tests/fixtures/geth-parity-external)");
    println!("  --chain-id <u64>      Chain id to write in samples (default: 1)");
    println!("  --dry-run             Print generation summary without writing files");
    println!("  -h, --help            Show this help");
}

fn parse_cli_args_v1() -> Result<CliArgsV1> {
    let geth_repo_root = std::env::var(GETH_REPO_ROOT_ENV_V1)
        .context("NOVOVM_GETH_REPO_ROOT is required")?
        .trim()
        .to_string();
    if geth_repo_root.is_empty() {
        bail!("NOVOVM_GETH_REPO_ROOT is required");
    }

    let mut source_dir = None::<PathBuf>;
    let mut output_dir = None::<PathBuf>;
    let mut chain_id = 1_u64;
    let mut dry_run = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source-dir" => {
                let value = args
                    .next()
                    .context("--source-dir requires a value")?
                    .trim()
                    .to_string();
                if value.is_empty() {
                    bail!("--source-dir cannot be empty");
                }
                source_dir = Some(PathBuf::from(value));
            }
            "--output-dir" => {
                let value = args
                    .next()
                    .context("--output-dir requires a value")?
                    .trim()
                    .to_string();
                if value.is_empty() {
                    bail!("--output-dir cannot be empty");
                }
                output_dir = Some(PathBuf::from(value));
            }
            "--chain-id" => {
                let value = args.next().context("--chain-id requires a value")?;
                chain_id = value
                    .trim()
                    .parse::<u64>()
                    .with_context(|| format!("invalid --chain-id value: {value}"))?;
            }
            "--dry-run" => {
                dry_run = true;
            }
            "-h" | "--help" => {
                print_usage_v1();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    let geth_repo_root = PathBuf::from(geth_repo_root);
    let source_dir = source_dir.unwrap_or_else(|| {
        geth_repo_root
            .join("internal")
            .join("ethapi")
            .join("testdata")
    });
    let output_dir = output_dir.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("geth-parity-external")
    });

    Ok(CliArgsV1 {
        geth_repo_root,
        source_dir,
        output_dir,
        chain_id,
        dry_run,
    })
}

fn normalize_hex_payload_v1(raw: &str, trim_leading_zeros: bool) -> Result<String> {
    let trimmed = raw.trim();
    let payload = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if payload.chars().any(|ch| !ch.is_ascii_hexdigit()) {
        bail!("invalid hex payload: {raw}");
    }
    let mut normalized = payload.to_ascii_lowercase();
    if trim_leading_zeros {
        normalized = normalized.trim_start_matches('0').to_string();
        if normalized.is_empty() {
            normalized.push('0');
        }
    }
    Ok(format!("0x{normalized}"))
}

fn parse_hex_usize_v1(raw: &str, field: &str) -> Result<usize> {
    let normalized = normalize_hex_payload_v1(raw, true)?;
    let payload = normalized.strip_prefix("0x").unwrap_or(normalized.as_str());
    u64::from_str_radix(payload, 16)
        .with_context(|| format!("parse {field} as hex u64 failed: {raw}"))
        .and_then(|value| {
            usize::try_from(value).with_context(|| format!("parse {field} as usize failed: {raw}"))
        })
}

fn status_is_success_v1(raw_status: &str) -> Result<bool> {
    let normalized = normalize_hex_payload_v1(raw_status, true)?;
    let payload = normalized.strip_prefix("0x").unwrap_or(normalized.as_str());
    Ok(payload != "0")
}

fn parse_geth_export_receipts_from_value_v1(
    value: &Value,
) -> Result<Vec<GethExportReceiptInputV1>> {
    if value.is_array() {
        let receipts: Vec<GethExportReceiptInputV1> = serde_json::from_value(value.clone())
            .context("parse geth export receipt array failed")?;
        return Ok(receipts);
    }
    if value.get("transactionIndex").is_some() && value.get("blockNumber").is_some() {
        let receipt: GethExportReceiptInputV1 = serde_json::from_value(value.clone())
            .context("parse geth export receipt object failed")?;
        return Ok(vec![receipt]);
    }
    if let Some(result) = value.get("result") {
        return parse_geth_export_receipts_from_value_v1(result);
    }
    if let Some(receipts) = value.get("receipts") {
        return parse_geth_export_receipts_from_value_v1(receipts);
    }
    bail!("unsupported geth export payload shape")
}

fn is_candidate_geth_export_v1(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    if !lower.ends_with(".json") {
        return false;
    }
    if lower.starts_with("eth_gettransactionreceipt-")
        && !lower.contains("notfound")
        && !lower.contains("empty")
    {
        return true;
    }
    lower.starts_with("eth_getblockreceipts-block-with-")
}

fn sample_name_from_geth_file_v1(file_name: &str) -> String {
    let stem = file_name.strip_suffix(".json").unwrap_or(file_name);
    if let Some(suffix) = stem.strip_prefix("eth_getTransactionReceipt-") {
        let mapped = match suffix {
            "blob-tx" => "blob-tx-success",
            "dynamic-tx-with-logs" => "dynamic-tx-failure",
            "with-logs" => "legacy-with-logs",
            other => other,
        };
        return format!("ethapi-{mapped}");
    }
    if let Some(suffix) = stem.strip_prefix("eth_getBlockReceipts-block-with-") {
        return format!("ethapi-blockreceipts-{suffix}");
    }
    if let Some(suffix) = stem.strip_prefix("eth_getBlockReceipts-") {
        return format!("ethapi-blockreceipts-{suffix}");
    }
    format!("ethapi-{stem}")
}

fn store_format_from_geth_file_v1(file_name: &str) -> &'static str {
    if file_name.starts_with("eth_getBlockReceipts-") {
        "geth-export-receipts/v1"
    } else {
        "geth-export-receipt/v1"
    }
}

fn path_to_forward_slash_v1(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn build_store_path_v1(geth_repo_root: &Path, source_file: &Path) -> Result<String> {
    let relative = source_file.strip_prefix(geth_repo_root).with_context(|| {
        format!(
            "source file is not under NOVOVM_GETH_REPO_ROOT: {}",
            source_file.display()
        )
    })?;
    Ok(format!(
        "${{{}}}/{}",
        GETH_REPO_ROOT_ENV_V1,
        path_to_forward_slash_v1(relative)
    ))
}

fn normalize_receipts_v1(
    receipts: Vec<GethExportReceiptInputV1>,
) -> Result<Vec<NormalizedReceiptV1>> {
    let mut out = Vec::with_capacity(receipts.len());
    for (fallback_idx, receipt) in receipts.into_iter().enumerate() {
        let tx_index = if let Some(raw) = receipt.transaction_index.as_deref() {
            parse_hex_usize_v1(raw, "transactionIndex")?
        } else {
            fallback_idx
        };
        let status = normalize_hex_payload_v1(
            receipt
                .status
                .as_deref()
                .context("receipt missing status")?,
            true,
        )?;
        let status_ok = status_is_success_v1(status.as_str())?;
        let tx_type = normalize_hex_payload_v1(receipt.tx_type.as_deref().unwrap_or("0x0"), true)?;
        let gas_used = normalize_hex_payload_v1(
            receipt
                .gas_used
                .as_deref()
                .context("receipt missing gasUsed")?,
            true,
        )?;
        let cumulative_gas_used = normalize_hex_payload_v1(
            receipt
                .cumulative_gas_used
                .as_deref()
                .context("receipt missing cumulativeGasUsed")?,
            true,
        )?;
        let contract_address = if status_ok {
            receipt
                .contract_address
                .as_deref()
                .map(|raw| normalize_hex_payload_v1(raw, false))
                .transpose()?
        } else {
            None
        };
        let revert_data = receipt
            .revert_data
            .as_deref()
            .map(|raw| normalize_hex_payload_v1(raw, false))
            .transpose()?
            .and_then(|value| if value == "0x" { None } else { Some(value) });
        let effective_log_count = if status_ok {
            receipt.logs.len() as u64
        } else {
            0
        };
        let first_log_address = if status_ok {
            receipt
                .logs
                .first()
                .map(|log| normalize_hex_payload_v1(log.address.as_str(), false))
                .transpose()?
        } else {
            None
        };

        out.push(NormalizedReceiptV1 {
            tx_index,
            status,
            status_ok,
            tx_type,
            gas_used,
            cumulative_gas_used,
            contract_address,
            revert_data,
            effective_log_count,
            first_log_address,
        });
    }
    out.sort_by_key(|item| item.tx_index);
    Ok(out)
}

fn build_expected_v1(
    block_number_hex: String,
    receipts: &[NormalizedReceiptV1],
) -> GethParityExpectedOutV1 {
    let tx_types = receipts
        .iter()
        .map(|receipt| receipt.tx_type.clone())
        .collect();
    let tx_statuses = receipts
        .iter()
        .map(|receipt| receipt.status.clone())
        .collect::<Vec<_>>();
    let tx_contract_addresses = receipts
        .iter()
        .map(|receipt| receipt.contract_address.clone())
        .collect::<Vec<_>>();

    let receipts_out = receipts
        .iter()
        .map(|receipt| GethParityExpectedReceiptOutV1 {
            tx_index: receipt.tx_index,
            status: receipt.status.clone(),
            tx_type: receipt.tx_type.clone(),
            gas_used: receipt.gas_used.clone(),
            cumulative_gas_used: receipt.cumulative_gas_used.clone(),
            contract_address: receipt.contract_address.clone(),
            revert_data: receipt.revert_data.clone(),
            log_count: receipt.effective_log_count,
        })
        .collect::<Vec<_>>();

    let first_canonical_log_address = receipts.iter().find_map(|receipt| {
        if receipt.effective_log_count > 0 {
            receipt.first_log_address.clone()
        } else {
            None
        }
    });
    let canonical_log_count = receipts
        .iter()
        .map(|receipt| receipt.effective_log_count)
        .sum::<u64>();

    let logs_canonical = GethParityExpectedLogsViewOutV1 {
        count: canonical_log_count,
        first_removed: first_canonical_log_address.as_ref().map(|_| false),
        first_address: first_canonical_log_address.clone(),
        first_log_ownership: None,
    };
    let logs_noncanonical = GethParityExpectedLogsViewOutV1 {
        count: canonical_log_count,
        first_removed: first_canonical_log_address.as_ref().map(|_| true),
        first_address: first_canonical_log_address,
        first_log_ownership: if canonical_log_count > 0 {
            Some("non_canonical".to_string())
        } else {
            None
        },
    };

    let typed_failures = receipts
        .iter()
        .filter(|receipt| !receipt.status_ok)
        .map(|receipt| GethParityExpectedTypedFailureOutV1 {
            tx_index: receipt.tx_index,
            failure_classification_contract: FAILURE_CLASSIFICATION_CONTRACT_V1,
            status: receipt.status.clone(),
            contract_address_null: true,
        })
        .collect::<Vec<_>>();

    GethParityExpectedOutV1 {
        block: GethParityExpectedBlockOutV1 {
            number: block_number_hex,
            tx_count: receipts.len() as u64,
            tx_types,
            tx_statuses,
            tx_contract_addresses,
        },
        receipts: receipts_out,
        logs_canonical,
        logs_noncanonical,
        typed_failures,
    }
}

fn build_sample_from_geth_export_v1(
    cfg: &CliArgsV1,
    source_file: &Path,
) -> Result<GethParitySampleFileOutV1> {
    let file_name = source_file
        .file_name()
        .and_then(|value| value.to_str())
        .context("invalid source file name")?
        .to_string();
    let payload = fs::read_to_string(source_file)
        .with_context(|| format!("read source file failed: {}", source_file.display()))?;
    let json: Value = serde_json::from_str(payload.as_str()).with_context(|| {
        format!(
            "parse source file as json failed: {}",
            source_file.display()
        )
    })?;
    let raw_receipts = parse_geth_export_receipts_from_value_v1(&json)
        .with_context(|| format!("parse geth receipts failed: {}", source_file.display()))?;
    if raw_receipts.is_empty() {
        bail!("no receipts found in {}", source_file.display());
    }

    let block_number = raw_receipts
        .first()
        .and_then(|receipt| receipt.block_number.as_deref())
        .context("receipt missing blockNumber")?;
    let block_number_hex = normalize_hex_payload_v1(block_number, true)?;
    let receipts = normalize_receipts_v1(raw_receipts)?;
    let expected = build_expected_v1(block_number_hex, receipts.as_slice());
    let store_path = build_store_path_v1(cfg.geth_repo_root.as_path(), source_file)?;

    Ok(GethParitySampleFileOutV1 {
        schema: SAMPLE_SCHEMA_V1,
        name: sample_name_from_geth_file_v1(file_name.as_str()),
        chain_id: cfg.chain_id,
        store_format: store_format_from_geth_file_v1(file_name.as_str()).to_string(),
        store_path,
        expected,
    })
}

fn discover_source_files_v1(source_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = fs::read_dir(source_dir)
        .with_context(|| format!("read source dir failed: {}", source_dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(is_candidate_geth_export_v1)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    out.sort();
    Ok(out)
}

fn main() -> Result<()> {
    let cfg = parse_cli_args_v1()?;
    if !cfg.geth_repo_root.exists() {
        bail!(
            "NOVOVM_GETH_REPO_ROOT does not exist: {}",
            cfg.geth_repo_root.display()
        );
    }
    if !cfg.source_dir.exists() {
        bail!("source dir does not exist: {}", cfg.source_dir.display());
    }
    if !cfg.dry_run {
        fs::create_dir_all(cfg.output_dir.as_path())
            .with_context(|| format!("create output dir failed: {}", cfg.output_dir.display()))?;
    }

    let source_files = discover_source_files_v1(cfg.source_dir.as_path())?;
    if source_files.is_empty() {
        bail!(
            "no candidate geth export files found in {}",
            cfg.source_dir.display()
        );
    }

    let mut created = 0_u64;
    let mut updated = 0_u64;
    let mut unchanged = 0_u64;
    for source_file in &source_files {
        let sample = build_sample_from_geth_export_v1(&cfg, source_file)?;
        let output_path = cfg
            .output_dir
            .join(format!("{}.sample.json", sample.name.as_str()));
        let mut rendered = serde_json::to_string_pretty(&sample)
            .with_context(|| format!("serialize sample failed: {}", source_file.display()))?;
        rendered.push('\n');
        let previous = fs::read_to_string(output_path.as_path()).ok();
        if previous.as_deref() == Some(rendered.as_str()) {
            unchanged += 1;
        } else if output_path.exists() {
            updated += 1;
        } else {
            created += 1;
        }
        if !cfg.dry_run {
            fs::write(output_path.as_path(), rendered.as_bytes())
                .with_context(|| format!("write sample file failed: {}", output_path.display()))?;
        }
    }

    println!(
        "supervm-mainline-geth-sample-sync: source={}, output={}, processed={}, created={}, updated={}, unchanged={}, dryRun={}",
        cfg.source_dir.display(),
        cfg.output_dir.display(),
        source_files.len(),
        created,
        updated,
        unchanged,
        cfg.dry_run
    );
    Ok(())
}
