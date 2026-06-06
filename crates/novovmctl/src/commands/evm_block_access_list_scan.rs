use std::collections::BTreeMap;
use std::path::PathBuf;

use novovm_node::mainline_canonical::{
    derive_mainline_eth_block_contexts_v1, load_mainline_canonical_store,
    MainlineCanonicalBatchRecordV1,
};
use serde::Serialize;

use crate::cli::EvmBlockAccessListScanArgs;
use crate::error::CtlError;
use crate::output;
use crate::runtime::files;

const COMMAND_NAME: &str = "evm-block-access-list-scan";
const DEFAULT_LATEST_COUNT: u64 = 128;

#[derive(Debug, Clone, Serialize)]
struct EvmBlockAccessListScanBlockReport {
    block_number: String,
    canonical_batch_seq: String,
    block_hash: String,
    payload_present: bool,
    block_access_list_complete: bool,
    block_access_list_hash_present: bool,
    account_count: u64,
    item_count: u64,
    storage_change_count: u64,
    storage_read_count: u64,
    balance_change_count: u64,
    nonce_change_count: u64,
    code_change_count: u64,
    issue_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct EvmBlockAccessListScanSummary {
    payload_present_count: u64,
    payload_missing_count: u64,
    complete_count: u64,
    incomplete_count: u64,
    hash_present_count: u64,
    complete_with_hash_count: u64,
    complete_missing_hash_count: u64,
    problem_block_count: u64,
    issue_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
struct EvmBlockAccessListScanReport {
    store_path: String,
    chain_id: String,
    latest_block_number: String,
    requested_latest_count: Option<u64>,
    from_block: String,
    to_block: String,
    scanned_block_count: u64,
    only_problems: bool,
    require_payload: bool,
    require_complete: bool,
    require_hash_when_complete: bool,
    summary: EvmBlockAccessListScanSummary,
    blocks: Vec<EvmBlockAccessListScanBlockReport>,
}

pub fn run(args: EvmBlockAccessListScanArgs) -> Result<(), CtlError> {
    let report = inner_run(&args)?;

    println!("[novovmctl] command={} ok=true", COMMAND_NAME);
    println!(
        "[novovmctl] store={} range={}..{} latest={} scanned={} problems={}",
        report.store_path,
        report.from_block,
        report.to_block,
        report.latest_block_number,
        report.scanned_block_count,
        report.summary.problem_block_count
    );
    println!(
        "[novovmctl] payload_present={} payload_missing={} complete={} incomplete={} hash_present={} complete_with_hash={}",
        report.summary.payload_present_count,
        report.summary.payload_missing_count,
        report.summary.complete_count,
        report.summary.incomplete_count,
        report.summary.hash_present_count,
        report.summary.complete_with_hash_count
    );

    if let Some(path) = args.json_out.as_deref() {
        files::write_json_pretty(path, &report)?;
        println!("[novovmctl] json_out={}", path);
    }

    output::print_success_json(COMMAND_NAME, &report)
}

fn inner_run(args: &EvmBlockAccessListScanArgs) -> Result<EvmBlockAccessListScanReport, CtlError> {
    if args.latest_count.is_some() && args.from_block.is_some() {
        return Err(CtlError::InvalidArgument(
            "--latest-count cannot be combined with --from-block".to_string(),
        ));
    }

    let store_path = resolve_store_path(args);
    let store = load_mainline_canonical_store(store_path.as_path()).map_err(|error| {
        CtlError::FileReadFailed(format!(
            "load canonical store `{}` failed: {error}",
            store_path.display()
        ))
    })?;
    let block_contexts = derive_mainline_eth_block_contexts_v1(&store);
    let latest_block_number = block_contexts
        .last()
        .map(|context| context.block_number)
        .unwrap_or(0);
    let (requested_latest_count, from_block, to_block) =
        resolve_scan_window(args, latest_block_number)?;

    let mut summary = EvmBlockAccessListScanSummary {
        payload_present_count: 0,
        payload_missing_count: 0,
        complete_count: 0,
        incomplete_count: 0,
        hash_present_count: 0,
        complete_with_hash_count: 0,
        complete_missing_hash_count: 0,
        problem_block_count: 0,
        issue_counts: BTreeMap::new(),
    };
    let mut blocks = Vec::new();
    let mut scanned_block_count = 0u64;

    for (batch, block_context) in store.batches.iter().zip(block_contexts.iter()) {
        if block_context.block_number < from_block || block_context.block_number > to_block {
            continue;
        }
        scanned_block_count += 1;
        let block_report = build_block_report(batch, block_context.block_hash);
        let has_problem = !block_report.issue_codes.is_empty();
        apply_block_summary(&mut summary, &block_report);
        if !args.only_problems || has_problem {
            blocks.push(block_report);
        }
    }

    enforce_scan_requirements(args, &summary, &store_path, from_block, to_block)?;

    Ok(EvmBlockAccessListScanReport {
        store_path: store_path.display().to_string(),
        chain_id: format!("0x{:x}", store.chain_id),
        latest_block_number: format!("0x{:x}", latest_block_number),
        requested_latest_count,
        from_block: format!("0x{:x}", from_block),
        to_block: format!("0x{:x}", to_block),
        scanned_block_count,
        only_problems: args.only_problems,
        require_payload: args.require_payload,
        require_complete: args.require_complete,
        require_hash_when_complete: args.require_hash_when_complete,
        summary,
        blocks,
    })
}

fn resolve_store_path(args: &EvmBlockAccessListScanArgs) -> PathBuf {
    args.store_path
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(novovm_node::mainline_query::default_mainline_query_store_path)
}

fn resolve_scan_window(
    args: &EvmBlockAccessListScanArgs,
    latest_block_number: u64,
) -> Result<(Option<u64>, u64, u64), CtlError> {
    if latest_block_number == 0 && args.from_block.is_none() && args.latest_count.is_none() {
        return Ok((Some(DEFAULT_LATEST_COUNT), 0, 0));
    }

    if let Some(from_block_raw) = args
        .from_block
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
    {
        let from_block =
            parse_block_selector(from_block_raw, latest_block_number).ok_or_else(|| {
                CtlError::InvalidArgument(format!(
                    "invalid --from-block selector: {from_block_raw}"
                ))
            })?;
        let to_selector = args.to_block.as_deref().unwrap_or("latest");
        let to_block = parse_block_selector(to_selector, latest_block_number).ok_or_else(|| {
            CtlError::InvalidArgument(format!("invalid --to-block selector: {to_selector}"))
        })?;
        if from_block > to_block {
            return Err(CtlError::InvalidArgument(format!(
                "scan window is inverted: from=0x{from_block:x} to=0x{to_block:x}"
            )));
        }
        return Ok((None, from_block, to_block));
    }

    let requested_latest_count = args.latest_count.unwrap_or(DEFAULT_LATEST_COUNT).max(1);
    if latest_block_number == 0 {
        return Ok((Some(requested_latest_count), 0, 0));
    }
    let span = requested_latest_count.saturating_sub(1);
    let from_block = latest_block_number.saturating_sub(span);
    Ok((
        Some(requested_latest_count),
        from_block,
        latest_block_number,
    ))
}

fn parse_block_selector(raw: &str, latest_block_number: u64) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("latest")
        || trimmed.eq_ignore_ascii_case("pending")
        || trimmed.eq_ignore_ascii_case("safe")
        || trimmed.eq_ignore_ascii_case("finalized")
    {
        return Some(latest_block_number);
    }
    if let Some(normalized) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u64::from_str_radix(normalized, 16).ok();
    }
    trimmed.parse::<u64>().ok()
}

fn build_block_report(
    batch: &MainlineCanonicalBatchRecordV1,
    block_hash: [u8; 32],
) -> EvmBlockAccessListScanBlockReport {
    let payload_present = batch.block_access_list.is_some();
    let block_access_list_complete = batch.block_access_list_complete;
    let block_access_list_hash_present = batch.block_access_list_hash.is_some();

    let (
        account_count,
        item_count,
        storage_change_count,
        storage_read_count,
        balance_change_count,
        nonce_change_count,
        code_change_count,
    ) = batch
        .block_access_list
        .as_ref()
        .map(|list| {
            let account_count = list.0.len() as u64;
            let storage_change_count = list
                .0
                .iter()
                .map(|account| account.storage_changes.len() as u64)
                .sum();
            let storage_read_count = list
                .0
                .iter()
                .map(|account| account.storage_reads.len() as u64)
                .sum();
            let balance_change_count = list
                .0
                .iter()
                .map(|account| account.balance_changes.len() as u64)
                .sum();
            let nonce_change_count = list
                .0
                .iter()
                .map(|account| account.nonce_changes.len() as u64)
                .sum();
            let code_change_count = list
                .0
                .iter()
                .map(|account| account.code_changes.len() as u64)
                .sum();
            let item_count = list
                .0
                .iter()
                .map(|account| {
                    account.storage_changes.len() as u64
                        + account.storage_reads.len() as u64
                        + account.balance_changes.len() as u64
                        + account.nonce_changes.len() as u64
                        + account.code_changes.len() as u64
                })
                .sum();
            (
                account_count,
                item_count,
                storage_change_count,
                storage_read_count,
                balance_change_count,
                nonce_change_count,
                code_change_count,
            )
        })
        .unwrap_or((0, 0, 0, 0, 0, 0, 0));

    let mut issue_codes = Vec::new();
    if !payload_present {
        issue_codes.push("missing_payload".to_string());
    }
    if !block_access_list_complete {
        issue_codes.push("incomplete_payload".to_string());
    }
    if block_access_list_complete && !block_access_list_hash_present {
        issue_codes.push("missing_hash_for_complete_payload".to_string());
    }
    if !payload_present && block_access_list_complete {
        issue_codes.push("complete_without_payload".to_string());
    }
    if block_access_list_hash_present && !block_access_list_complete {
        issue_codes.push("hash_present_while_incomplete".to_string());
    }

    EvmBlockAccessListScanBlockReport {
        block_number: format!("0x{:x}", batch.seq),
        canonical_batch_seq: format!("0x{:x}", batch.seq),
        block_hash: to_hex_prefixed(&block_hash),
        payload_present,
        block_access_list_complete,
        block_access_list_hash_present,
        account_count,
        item_count,
        storage_change_count,
        storage_read_count,
        balance_change_count,
        nonce_change_count,
        code_change_count,
        issue_codes,
    }
}

fn apply_block_summary(
    summary: &mut EvmBlockAccessListScanSummary,
    block: &EvmBlockAccessListScanBlockReport,
) {
    if block.payload_present {
        summary.payload_present_count += 1;
    } else {
        summary.payload_missing_count += 1;
    }
    if block.block_access_list_complete {
        summary.complete_count += 1;
    } else {
        summary.incomplete_count += 1;
    }
    if block.block_access_list_hash_present {
        summary.hash_present_count += 1;
    }
    if block.block_access_list_complete && block.block_access_list_hash_present {
        summary.complete_with_hash_count += 1;
    }
    if block.block_access_list_complete && !block.block_access_list_hash_present {
        summary.complete_missing_hash_count += 1;
    }
    if !block.issue_codes.is_empty() {
        summary.problem_block_count += 1;
    }
    for issue in &block.issue_codes {
        *summary.issue_counts.entry(issue.clone()).or_insert(0) += 1;
    }
}

fn enforce_scan_requirements(
    args: &EvmBlockAccessListScanArgs,
    summary: &EvmBlockAccessListScanSummary,
    store_path: &PathBuf,
    from_block: u64,
    to_block: u64,
) -> Result<(), CtlError> {
    let mut failures = Vec::new();
    if args.require_payload && summary.payload_missing_count > 0 {
        failures.push(format!("payload_missing={}", summary.payload_missing_count));
    }
    if args.require_complete && summary.incomplete_count > 0 {
        failures.push(format!("incomplete={}", summary.incomplete_count));
    }
    if args.require_hash_when_complete && summary.complete_missing_hash_count > 0 {
        failures.push(format!(
            "complete_missing_hash={}",
            summary.complete_missing_hash_count
        ));
    }
    if failures.is_empty() {
        return Ok(());
    }
    Err(CtlError::IntegrationFailed(format!(
        "BAL scan requirements failed: {} range=0x{:x}..0x{:x} store={}",
        failures.join(" "),
        from_block,
        to_block,
        store_path.display()
    )))
}

fn to_hex_prefixed(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push_str("0x");
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_node::mainline_canonical::{
        derive_mainline_eth_block_contexts_v1, load_mainline_canonical_store,
    };
    use serde_json::json;
    use std::path::Path;

    fn sample_store_json_v1() -> serde_json::Value {
        json!({
            "schema": "supervm-mainline-canonical/v1",
            "generated_unix_ms": 1,
            "chain_type": "evm",
            "chain_id": 1,
            "batches": [
                {
                    "seq": 1,
                    "source_detail": "novovmctl-test",
                    "tx_count": 1,
                    "tap_requested": 1,
                    "tap_accepted": 1,
                    "tap_dropped": 0,
                    "apply_verified": true,
                    "apply_applied": true,
                    "apply_state_root": vec![0x11u8; 32],
                    "block_access_list": [
                        {
                            "address": vec![0x11u8; 20],
                            "storageChanges": [
                                {
                                    "slot": vec![0x22u8; 32],
                                    "slotChanges": [
                                        {
                                            "blockAccessIndex": 1,
                                            "postValue": vec![0x33u8; 32]
                                        }
                                    ]
                                }
                            ],
                            "storageReads": [vec![0x44u8; 32]],
                            "balanceChanges": [],
                            "nonceChanges": [],
                            "codeChanges": []
                        }
                    ],
                    "block_access_list_complete": true,
                    "block_access_list_hash": vec![0xa1u8; 32],
                    "exported_receipt_count": 0,
                    "mirrored_receipt_count": 0,
                    "state_version": 1,
                    "ingress_bypassed": true,
                    "atomic_guard_enabled": false,
                    "receipts": [],
                    "state_mirror_updates": []
                },
                {
                    "seq": 2,
                    "source_detail": "novovmctl-test",
                    "tx_count": 1,
                    "tap_requested": 1,
                    "tap_accepted": 1,
                    "tap_dropped": 0,
                    "apply_verified": true,
                    "apply_applied": true,
                    "apply_state_root": vec![0x22u8; 32],
                    "block_access_list": [
                        {
                            "address": vec![0x12u8; 20],
                            "storageChanges": [],
                            "storageReads": [],
                            "balanceChanges": [
                                {
                                    "blockAccessIndex": 2,
                                    "postBalance": vec![0x55u8; 32]
                                }
                            ],
                            "nonceChanges": [],
                            "codeChanges": []
                        }
                    ],
                    "block_access_list_complete": false,
                    "block_access_list_hash": null,
                    "exported_receipt_count": 0,
                    "mirrored_receipt_count": 0,
                    "state_version": 2,
                    "ingress_bypassed": true,
                    "atomic_guard_enabled": false,
                    "receipts": [],
                    "state_mirror_updates": []
                },
                {
                    "seq": 3,
                    "source_detail": "novovmctl-test",
                    "tx_count": 1,
                    "tap_requested": 1,
                    "tap_accepted": 1,
                    "tap_dropped": 0,
                    "apply_verified": true,
                    "apply_applied": true,
                    "apply_state_root": vec![0x33u8; 32],
                    "block_access_list": null,
                    "block_access_list_complete": false,
                    "block_access_list_hash": null,
                    "exported_receipt_count": 0,
                    "mirrored_receipt_count": 0,
                    "state_version": 3,
                    "ingress_bypassed": true,
                    "atomic_guard_enabled": false,
                    "receipts": [],
                    "state_mirror_updates": []
                }
            ]
        })
    }

    fn with_sample_store_path<T>(f: impl FnOnce(&Path, Vec<String>) -> T) -> T {
        let temp_root = std::env::temp_dir().join(format!(
            "novovmctl-evm-bal-scan-{}-{}",
            std::process::id(),
            output::now_unix_ms()
        ));
        std::fs::create_dir_all(&temp_root).expect("create temp root");
        let store_path = temp_root.join("canonical.json");
        let rendered =
            serde_json::to_string_pretty(&sample_store_json_v1()).expect("render sample store");
        std::fs::write(&store_path, rendered).expect("write sample store");
        let loaded = load_mainline_canonical_store(&store_path).expect("load sample store");
        let block_hashes = derive_mainline_eth_block_contexts_v1(&loaded)
            .into_iter()
            .map(|context| to_hex_prefixed(&context.block_hash))
            .collect::<Vec<_>>();
        let out = f(store_path.as_path(), block_hashes);
        let _ = std::fs::remove_dir_all(temp_root);
        out
    }

    #[test]
    fn inner_run_latest_count_reports_summary() {
        with_sample_store_path(|store_path, _block_hashes| {
            let args = EvmBlockAccessListScanArgs {
                latest_count: Some(2),
                from_block: None,
                to_block: None,
                store_path: Some(store_path.display().to_string()),
                only_problems: false,
                require_payload: false,
                require_complete: false,
                require_hash_when_complete: false,
                json_out: None,
            };
            let report = inner_run(&args).expect("inner run latest count");
            assert_eq!(report.from_block, "0x2");
            assert_eq!(report.to_block, "0x3");
            assert_eq!(report.scanned_block_count, 2);
            assert_eq!(report.summary.payload_present_count, 1);
            assert_eq!(report.summary.payload_missing_count, 1);
            assert_eq!(report.summary.incomplete_count, 2);
            assert_eq!(report.summary.problem_block_count, 2);
            assert_eq!(
                report.summary.issue_counts.get("missing_payload").copied(),
                Some(1)
            );
            assert_eq!(report.blocks.len(), 2);
        });
    }

    #[test]
    fn inner_run_only_problems_filters_healthy_blocks() {
        with_sample_store_path(|store_path, _block_hashes| {
            let args = EvmBlockAccessListScanArgs {
                latest_count: Some(3),
                from_block: None,
                to_block: None,
                store_path: Some(store_path.display().to_string()),
                only_problems: true,
                require_payload: false,
                require_complete: false,
                require_hash_when_complete: false,
                json_out: None,
            };
            let report = inner_run(&args).expect("inner run only problems");
            assert_eq!(report.scanned_block_count, 3);
            assert_eq!(report.summary.problem_block_count, 2);
            assert_eq!(report.blocks.len(), 2);
            assert!(report
                .blocks
                .iter()
                .all(|block| !block.issue_codes.is_empty()));
        });
    }

    #[test]
    fn inner_run_require_complete_rejects_incomplete_blocks() {
        with_sample_store_path(|store_path, _block_hashes| {
            let args = EvmBlockAccessListScanArgs {
                latest_count: Some(3),
                from_block: None,
                to_block: None,
                store_path: Some(store_path.display().to_string()),
                only_problems: true,
                require_payload: false,
                require_complete: true,
                require_hash_when_complete: false,
                json_out: None,
            };
            let err = inner_run(&args).expect_err("require complete should fail");
            assert!(err.to_string().contains("BAL scan requirements failed"));
            assert!(err.to_string().contains("incomplete=2"));
        });
    }

    #[test]
    fn inner_run_from_to_window_works() {
        with_sample_store_path(|store_path, block_hashes| {
            let args = EvmBlockAccessListScanArgs {
                latest_count: None,
                from_block: Some("0x1".to_string()),
                to_block: Some("0x2".to_string()),
                store_path: Some(store_path.display().to_string()),
                only_problems: false,
                require_payload: false,
                require_complete: false,
                require_hash_when_complete: false,
                json_out: None,
            };
            let report = inner_run(&args).expect("inner run from/to");
            assert_eq!(report.requested_latest_count, None);
            assert_eq!(report.scanned_block_count, 2);
            assert_eq!(report.blocks[0].block_hash, block_hashes[0]);
            assert_eq!(report.blocks[1].block_hash, block_hashes[1]);
        });
    }
}
