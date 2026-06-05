use std::path::PathBuf;

use novovm_node::mainline_query::{
    default_mainline_query_store_path, run_mainline_query_from_path,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::cli::EvmBlockAccessListArgs;
use crate::error::CtlError;
use crate::output;
use crate::runtime::files;

const COMMAND_NAME: &str = "evm-block-access-list";
const METHOD_BY_HASH: &str = "supervm_getEthCanonicalBlockAccessListByHash";
const METHOD_BY_NUMBER: &str = "supervm_getEthCanonicalBlockAccessListByNumber";

#[derive(Debug, Clone, Serialize)]
struct EvmBlockAccessListReport {
    query_method: String,
    store_path: String,
    selector_kind: String,
    selector_value: String,
    require_payload: bool,
    require_complete: bool,
    found: bool,
    payload_present: bool,
    block_access_list_complete: bool,
    response: Value,
}

pub fn run(args: EvmBlockAccessListArgs) -> Result<(), CtlError> {
    let report = inner_run(&args)?;

    println!("[novovmctl] command={} ok=true", COMMAND_NAME);
    println!(
        "[novovmctl] store={} selector={}={} found={} payload_present={} complete={}",
        report.store_path,
        report.selector_kind,
        report.selector_value,
        report.found,
        report.payload_present,
        report.block_access_list_complete
    );

    if let Some(path) = args.json_out.as_deref() {
        files::write_json_pretty(path, &report)?;
        println!("[novovmctl] json_out={}", path);
    }

    output::print_success_json(COMMAND_NAME, &report)
}

fn inner_run(args: &EvmBlockAccessListArgs) -> Result<EvmBlockAccessListReport, CtlError> {
    let store_path = resolve_store_path(args);
    let (query_method, selector_kind, selector_value, params) = build_query(args)?;
    let response = run_mainline_query_from_path(store_path.as_path(), query_method, &params)
        .map_err(|error| {
            CtlError::IntegrationFailed(format!(
                "mainline BAL query failed: method={} store={} error={}",
                query_method,
                store_path.display(),
                error
            ))
        })?;

    let found = response
        .get("found")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let payload_present = response
        .get("payloadPresent")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let block_access_list_complete = response
        .get("blockAccessListContext")
        .and_then(|value| value.get("blockAccessListComplete"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if args.require_payload && !payload_present {
        return Err(CtlError::IntegrationFailed(format!(
            "BAL payload missing: selector={}={} store={}",
            selector_kind,
            selector_value,
            store_path.display()
        )));
    }
    if args.require_complete && !block_access_list_complete {
        return Err(CtlError::IntegrationFailed(format!(
            "BAL payload incomplete: selector={}={} store={}",
            selector_kind,
            selector_value,
            store_path.display()
        )));
    }

    Ok(EvmBlockAccessListReport {
        query_method: query_method.to_string(),
        store_path: store_path.display().to_string(),
        selector_kind: selector_kind.to_string(),
        selector_value,
        require_payload: args.require_payload,
        require_complete: args.require_complete,
        found,
        payload_present,
        block_access_list_complete,
        response,
    })
}

fn resolve_store_path(args: &EvmBlockAccessListArgs) -> PathBuf {
    args.store_path
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_mainline_query_store_path)
}

fn build_query(
    args: &EvmBlockAccessListArgs,
) -> Result<(&'static str, &'static str, String, Value), CtlError> {
    if let Some(block_hash) = args
        .block_hash
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
    {
        return Ok((
            METHOD_BY_HASH,
            "block_hash",
            block_hash.to_string(),
            json!({ "blockHash": block_hash }),
        ));
    }
    if let Some(block_number) = args
        .block_number
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
    {
        return Ok((
            METHOD_BY_NUMBER,
            "block_number",
            block_number.to_string(),
            json!({ "blockNumber": block_number }),
        ));
    }
    Err(CtlError::InvalidArgument(
        "either --block-hash or --block-number is required".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_node::mainline_canonical::{
        derive_mainline_eth_block_contexts_v1, load_mainline_canonical_store,
    };
    use std::path::Path;

    fn sample_store_json_v1() -> Value {
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
                    "apply_state_root": vec![0x77u8; 32],
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
                            "balanceChanges": [
                                {
                                    "blockAccessIndex": 2,
                                    "postBalance": vec![0x55u8; 32]
                                }
                            ],
                            "nonceChanges": [
                                {
                                    "blockAccessIndex": 3,
                                    "postNonce": 7
                                }
                            ],
                            "codeChanges": [
                                {
                                    "blockAccessIndex": 4,
                                    "newCode": [222, 173, 190, 239]
                                }
                            ]
                        }
                    ],
                    "block_access_list_complete": false,
                    "block_access_list_hash": vec![0xacu8; 32],
                    "exported_receipt_count": 0,
                    "mirrored_receipt_count": 0,
                    "state_version": 1,
                    "ingress_bypassed": true,
                    "atomic_guard_enabled": false,
                    "receipts": [],
                    "state_mirror_updates": []
                }
            ]
        })
    }

    fn with_sample_store_path<T>(f: impl FnOnce(&Path, String) -> T) -> T {
        let temp_root = std::env::temp_dir().join(format!(
            "novovmctl-evm-bal-{}-{}",
            std::process::id(),
            output::now_unix_ms()
        ));
        std::fs::create_dir_all(&temp_root).expect("create temp root");
        let store_path = temp_root.join("canonical.json");
        let rendered =
            serde_json::to_string_pretty(&sample_store_json_v1()).expect("render sample store");
        std::fs::write(&store_path, rendered).expect("write sample store");
        let loaded = load_mainline_canonical_store(&store_path).expect("load sample store");
        let block_hash = derive_mainline_eth_block_contexts_v1(&loaded)
            .first()
            .expect("block context")
            .block_hash;
        let block_hash_hex = format!("0x{}", to_lower_hex(&block_hash));
        let out = f(store_path.as_path(), block_hash_hex);
        let _ = std::fs::remove_dir_all(temp_root);
        out
    }

    fn to_lower_hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    #[test]
    fn inner_run_by_hash_returns_payload_report() {
        with_sample_store_path(|store_path, block_hash_hex| {
            let args = EvmBlockAccessListArgs {
                block_hash: Some(block_hash_hex.clone()),
                block_number: None,
                store_path: Some(store_path.display().to_string()),
                require_payload: false,
                require_complete: false,
                json_out: None,
            };
            let report = inner_run(&args).expect("inner run by hash");
            assert_eq!(report.query_method, METHOD_BY_HASH);
            assert_eq!(report.selector_kind, "block_hash");
            assert_eq!(report.selector_value, block_hash_hex);
            assert!(report.found);
            assert!(report.payload_present);
            assert!(!report.block_access_list_complete);
            let expected_hash = format!("0x{}", "ac".repeat(32));
            assert_eq!(
                report.response["blockAccessListContext"]["blockAccessListHash"].as_str(),
                Some(expected_hash.as_str())
            );
        });
    }

    #[test]
    fn inner_run_require_complete_rejects_incomplete_payload() {
        with_sample_store_path(|store_path, block_hash_hex| {
            let args = EvmBlockAccessListArgs {
                block_hash: Some(block_hash_hex),
                block_number: None,
                store_path: Some(store_path.display().to_string()),
                require_payload: true,
                require_complete: true,
                json_out: None,
            };
            let err = inner_run(&args).expect_err("require complete should fail");
            assert!(err.to_string().contains("BAL payload incomplete"));
        });
    }

    #[test]
    fn inner_run_by_number_latest_works() {
        with_sample_store_path(|store_path, _block_hash_hex| {
            let args = EvmBlockAccessListArgs {
                block_hash: None,
                block_number: Some("latest".to_string()),
                store_path: Some(store_path.display().to_string()),
                require_payload: false,
                require_complete: false,
                json_out: None,
            };
            let report = inner_run(&args).expect("inner run by number");
            assert_eq!(report.query_method, METHOD_BY_NUMBER);
            assert_eq!(report.selector_kind, "block_number");
            assert_eq!(report.selector_value, "latest");
            assert!(report.found);
            assert!(report.payload_present);
            assert_eq!(
                report.response["blockAccessListContext"]["accountCount"].as_u64(),
                Some(1)
            );
        });
    }
}
