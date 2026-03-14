use super::*;

const GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS_DEFAULT: u64 = 5_000;
const GATEWAY_ETH_UPSTREAM_RPC_ATTEMPTS: usize = 3;
const GATEWAY_ETH_UPSTREAM_RPC_RETRY_BACKOFF_MS: u64 = 100;

fn gateway_eth_upstream_chain_string_env(chain_id: u64, base_key: &str) -> Option<String> {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    let chain_key_hex_upper = format!("{base_key}_CHAIN_0x{:X}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty(&chain_key_hex_upper))
        .or_else(|| string_env_nonempty(base_key))
}

fn gateway_eth_upstream_chain_u64_env(chain_id: u64, base_key: &str, default: u64) -> u64 {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    let chain_key_hex_upper = format!("{base_key}_CHAIN_0x{:X}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty(&chain_key_hex_upper))
        .and_then(|raw| parse_u64_decimal_or_hex(raw.trim()))
        .unwrap_or_else(|| u64_env(base_key, default))
}

fn gateway_eth_upstream_rpc_url(chain_id: u64) -> Option<String> {
    gateway_eth_upstream_chain_string_env(chain_id, "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC")
}

fn gateway_eth_upstream_rpc_timeout_ms(chain_id: u64) -> u64 {
    gateway_eth_upstream_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS",
        GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS_DEFAULT,
    )
}

fn build_gateway_eth_upstream_params(
    method: &str,
    params: &serde_json::Value,
) -> Result<Option<serde_json::Value>> {
    let params = match method {
        "eth_blockNumber" | "eth_gasPrice" | "eth_maxPriorityFeePerGas" | "eth_syncing" => {
            serde_json::Value::Array(Vec::new())
        }
        "eth_getBalance" | "eth_getTransactionCount" | "eth_getCode" => {
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let block_tag =
                parse_eth_tx_count_block_tag(params).unwrap_or_else(|| "latest".to_string());
            serde_json::json!([address_raw, block_tag])
        }
        "eth_getStorageAt" => {
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address is required for eth_getStorageAt"))?;
            let slot_raw = extract_eth_storage_slot_param(params)
                .ok_or_else(|| anyhow::anyhow!("slot/position is required for eth_getStorageAt"))?;
            let block_tag =
                parse_eth_get_proof_block_tag(params).unwrap_or_else(|| "latest".to_string());
            serde_json::json!([address_raw, slot_raw, block_tag])
        }
        "eth_getProof" => {
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address is required for eth_getProof"))?;
            let storage_keys = parse_eth_get_proof_storage_keys(params)?;
            let block_tag =
                parse_eth_get_proof_block_tag(params).unwrap_or_else(|| "latest".to_string());
            serde_json::json!([address_raw, storage_keys, block_tag])
        }
        "eth_getBlockReceipts" => {
            if let Some(block_hash) = extract_eth_block_hash_param(params) {
                serde_json::json!([block_hash])
            } else {
                let block_tag =
                    parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
                serde_json::json!([block_tag])
            }
        }
        "eth_getBlockByNumber" => {
            let block_tag =
                parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
            let full_transactions = parse_eth_block_query_full_transactions(params);
            serde_json::json!([block_tag, full_transactions])
        }
        "eth_getBlockByHash" => {
            let block_hash = extract_eth_block_hash_param(params)
                .ok_or_else(|| anyhow::anyhow!("block_hash (or blockHash/hash) is required"))?;
            let full_transactions = parse_eth_block_query_full_transactions(params);
            serde_json::json!([block_hash, full_transactions])
        }
        "eth_getTransactionByHash" | "eth_getTransactionReceipt" => {
            let tx_hash = extract_eth_tx_hash_query_param(params)
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or hash) is required"))?;
            serde_json::json!([tx_hash])
        }
        "eth_getTransactionByBlockNumberAndIndex" => {
            let block_tag =
                parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
            let tx_index = parse_eth_block_query_tx_index(params).ok_or_else(|| {
                anyhow::anyhow!("transaction_index (or transactionIndex/index) is required")
            })?;
            serde_json::json!([block_tag, format!("0x{:x}", tx_index)])
        }
        "eth_getTransactionByBlockHashAndIndex" => {
            let block_hash = extract_eth_block_hash_param(params)
                .ok_or_else(|| anyhow::anyhow!("block_hash (or blockHash/hash) is required"))?;
            let tx_index = parse_eth_block_query_tx_index(params).ok_or_else(|| {
                anyhow::anyhow!("transaction_index (or transactionIndex/index) is required")
            })?;
            serde_json::json!([block_hash, format!("0x{:x}", tx_index)])
        }
        "eth_getBlockTransactionCountByNumber" | "eth_getUncleCountByBlockNumber" => {
            let block_tag =
                parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
            serde_json::json!([block_tag])
        }
        "eth_getBlockTransactionCountByHash" | "eth_getUncleCountByBlockHash" => {
            let block_hash = extract_eth_block_hash_param(params)
                .ok_or_else(|| anyhow::anyhow!("block_hash (or blockHash/hash) is required"))?;
            serde_json::json!([block_hash])
        }
        "eth_call" | "eth_estimateGas" => {
            let tx = build_gateway_eth_upstream_tx_object(params);
            if tx.is_null() {
                bail!("{method} transaction object is required");
            }
            if method == "eth_call" {
                let block_tag =
                    parse_eth_tx_count_block_tag(params).unwrap_or_else(|| "latest".to_string());
                serde_json::json!([tx, block_tag])
            } else if let Some(block_tag) = parse_eth_tx_count_block_tag(params) {
                serde_json::json!([tx, block_tag])
            } else {
                serde_json::json!([tx])
            }
        }
        "eth_feeHistory" => {
            let block_count = parse_eth_fee_history_block_count(params)
                .ok_or_else(|| anyhow::anyhow!("block_count (or blockCount) is required"))?;
            let newest_block = parse_eth_fee_history_newest_block_tag(params)
                .ok_or_else(|| anyhow::anyhow!("newest_block (or newestBlock) is required"))?;
            if let Some(reward_percentiles) = parse_eth_fee_history_reward_percentiles(params)? {
                serde_json::json!([
                    format!("0x{:x}", block_count),
                    newest_block,
                    reward_percentiles
                ])
            } else {
                serde_json::json!([format!("0x{:x}", block_count), newest_block])
            }
        }
        "eth_getLogs" => serde_json::json!([build_gateway_eth_upstream_logs_filter(params)]),
        _ => return Ok(None),
    };
    Ok(Some(params))
}

fn build_gateway_eth_upstream_tx_object(params: &serde_json::Value) -> serde_json::Value {
    let tx_keys = [
        "from",
        "to",
        "gas",
        "gasPrice",
        "value",
        "data",
        "input",
        "nonce",
        "type",
        "accessList",
        "maxFeePerGas",
        "maxPriorityFeePerGas",
        "maxFeePerBlobGas",
        "blobVersionedHashes",
        "authorizationList",
    ];
    let source = param_tx_object(params)
        .and_then(serde_json::Value::as_object)
        .or_else(|| params_object_with_any_keys(params, &tx_keys))
        .or_else(|| {
            params_object_with_any_keys(
                params,
                &[
                    "from",
                    "to",
                    "gas_limit",
                    "gasLimit",
                    "gas_price",
                    "value",
                    "data",
                    "input",
                    "nonce",
                    "tx_type",
                    "txType",
                    "access_list",
                    "max_fee_per_gas",
                    "max_priority_fee_per_gas",
                    "max_fee_per_blob_gas",
                    "blob_versioned_hashes",
                    "authorization_list",
                ],
            )
        });
    let Some(source) = source else {
        return serde_json::Value::Null;
    };
    let mut out = serde_json::Map::new();
    copy_first_present_json_field(source, &mut out, &["from"], "from");
    copy_first_present_json_field(source, &mut out, &["to"], "to");
    copy_first_present_json_field(source, &mut out, &["gas", "gas_limit", "gasLimit"], "gas");
    copy_first_present_json_field(source, &mut out, &["gasPrice", "gas_price"], "gasPrice");
    copy_first_present_json_field(source, &mut out, &["value"], "value");
    copy_first_present_json_field(source, &mut out, &["input", "data"], "input");
    copy_first_present_json_field(source, &mut out, &["nonce"], "nonce");
    copy_first_present_json_field(source, &mut out, &["type", "tx_type", "txType"], "type");
    copy_first_present_json_field(
        source,
        &mut out,
        &["accessList", "access_list"],
        "accessList",
    );
    copy_first_present_json_field(
        source,
        &mut out,
        &["maxFeePerGas", "max_fee_per_gas"],
        "maxFeePerGas",
    );
    copy_first_present_json_field(
        source,
        &mut out,
        &["maxPriorityFeePerGas", "max_priority_fee_per_gas"],
        "maxPriorityFeePerGas",
    );
    copy_first_present_json_field(
        source,
        &mut out,
        &["maxFeePerBlobGas", "max_fee_per_blob_gas"],
        "maxFeePerBlobGas",
    );
    copy_first_present_json_field(
        source,
        &mut out,
        &["blobVersionedHashes", "blob_versioned_hashes"],
        "blobVersionedHashes",
    );
    copy_first_present_json_field(
        source,
        &mut out,
        &["authorizationList", "authorization_list"],
        "authorizationList",
    );
    serde_json::Value::Object(out)
}

fn copy_first_present_json_field(
    source: &serde_json::Map<String, serde_json::Value>,
    dest: &mut serde_json::Map<String, serde_json::Value>,
    source_keys: &[&str],
    dest_key: &str,
) {
    for key in source_keys {
        if let Some(value) = source.get(*key) {
            dest.insert(dest_key.to_string(), value.clone());
            return;
        }
    }
}

fn build_gateway_eth_upstream_logs_filter(params: &serde_json::Value) -> serde_json::Value {
    let filter_source = params_object_with_any_keys_or_nested_filter(
        params,
        &[
            "block_hash",
            "blockHash",
            "fromBlock",
            "from_block",
            "toBlock",
            "to_block",
            "topics",
            "address",
            "addresses",
        ],
    );
    let Some(filter_source) = filter_source else {
        return serde_json::Value::Object(serde_json::Map::new());
    };
    let mut out = serde_json::Map::new();
    copy_first_present_json_field(
        filter_source,
        &mut out,
        &["blockHash", "block_hash"],
        "blockHash",
    );
    copy_first_present_json_field(
        filter_source,
        &mut out,
        &["fromBlock", "from_block"],
        "fromBlock",
    );
    copy_first_present_json_field(filter_source, &mut out, &["toBlock", "to_block"], "toBlock");
    copy_first_present_json_field(
        filter_source,
        &mut out,
        &["address", "addresses"],
        "address",
    );
    copy_first_present_json_field(filter_source, &mut out, &["topics"], "topics");
    serde_json::Value::Object(out)
}

pub(super) fn execute_gateway_eth_upstream_json_rpc(
    url: &str,
    method: &str,
    params: serde_json::Value,
    timeout_ms: u64,
) -> Result<serde_json::Value> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(timeout_ms))
        .build();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let response = agent
        .post(url)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|error| anyhow::anyhow!("upstream rpc request failed: {}", error))?;
    let payload: serde_json::Value = response
        .into_json()
        .context("decode upstream rpc response json failed")?;
    if let Some(error) = payload.get("error") {
        bail!("upstream rpc returned error: {}", error);
    }
    Ok(payload
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

pub(super) fn maybe_gateway_eth_upstream_read(
    chain_id: u64,
    method: &str,
    params: &serde_json::Value,
) -> Result<Option<serde_json::Value>> {
    let Some(url) = gateway_eth_upstream_rpc_url(chain_id) else {
        return Ok(None);
    };
    let Some(upstream_params) = build_gateway_eth_upstream_params(method, params)? else {
        return Ok(None);
    };
    let timeout_ms = gateway_eth_upstream_rpc_timeout_ms(chain_id);
    let mut last_error: Option<anyhow::Error> = None;
    for attempt in 0..GATEWAY_ETH_UPSTREAM_RPC_ATTEMPTS {
        match execute_gateway_eth_upstream_json_rpc(
            &url,
            method,
            upstream_params.clone(),
            timeout_ms,
        ) {
            Ok(result) if result.is_null() => {
                if attempt + 1 == GATEWAY_ETH_UPSTREAM_RPC_ATTEMPTS {
                    return Ok(None);
                }
            }
            Ok(result) => return Ok(Some(result)),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 == GATEWAY_ETH_UPSTREAM_RPC_ATTEMPTS {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(
            GATEWAY_ETH_UPSTREAM_RPC_RETRY_BACKOFF_MS,
        ));
    }
    if let Some(error) = last_error {
        if gateway_warn_enabled() {
            eprintln!(
                "gateway_warn: eth upstream read failed method={} chain_id={} url={} error={}",
                method, chain_id, url, error
            );
        }
        return Ok(None);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_eth_upstream_chain_string_env_prefers_chain_specific_key() {
        let _guard = gateway_env_mutex()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let captured = [
            "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC",
            "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_1",
        ]
        .iter()
        .map(|key| ((*key).to_string(), std::env::var(key).ok()))
        .collect::<Vec<_>>();
        std::env::set_var("NOVOVM_GATEWAY_ETH_UPSTREAM_RPC", "https://global.example");
        std::env::set_var(
            "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_1",
            "https://chain1.example",
        );
        assert_eq!(
            gateway_eth_upstream_rpc_url(1).as_deref(),
            Some("https://chain1.example")
        );
        assert_eq!(
            gateway_eth_upstream_rpc_url(2).as_deref(),
            Some("https://global.example")
        );
        for (key, value) in captured {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn build_gateway_eth_upstream_params_translates_get_balance_object_params() {
        let params = serde_json::json!({
            "address": "0x1111111111111111111111111111111111111111",
            "block": "latest",
            "chain_id": "0x1"
        });
        let built = build_gateway_eth_upstream_params("eth_getBalance", &params)
            .expect("build should succeed")
            .expect("method should be supported");
        assert_eq!(
            built,
            serde_json::json!(["0x1111111111111111111111111111111111111111", "latest"])
        );
    }

    #[test]
    fn build_gateway_eth_upstream_params_translates_get_block_by_number_mixed_array() {
        let params = serde_json::json!([
            {"chain_id": "0x1"},
            "pending",
            true
        ]);
        let built = build_gateway_eth_upstream_params("eth_getBlockByNumber", &params)
            .expect("build should succeed")
            .expect("method should be supported");
        assert_eq!(built, serde_json::json!(["pending", true]));
    }

    #[test]
    fn build_gateway_eth_upstream_params_uses_tx_hash_scalar() {
        let params = serde_json::json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ]);
        let built = build_gateway_eth_upstream_params("eth_getTransactionReceipt", &params)
            .expect("build should succeed")
            .expect("method should be supported");
        assert_eq!(
            built,
            serde_json::json!([
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ])
        );
    }

    #[test]
    fn build_gateway_eth_upstream_params_translates_fee_history_with_rewards() {
        let params = serde_json::json!({
            "block_count": "0x4",
            "newest_block": "pending",
            "reward_percentiles": [25.0, 75.0]
        });
        let built = build_gateway_eth_upstream_params("eth_feeHistory", &params)
            .expect("build should succeed")
            .expect("method should be supported");
        assert_eq!(built, serde_json::json!(["0x4", "pending", [25.0, 75.0]]));
    }

    #[test]
    fn build_gateway_eth_upstream_params_translates_logs_filter_aliases() {
        let params = serde_json::json!({
            "filter": {
                "from_block": "0x10",
                "to_block": "latest",
                "addresses": [
                    "0x1111111111111111111111111111111111111111",
                    "0x2222222222222222222222222222222222222222"
                ],
                "topics": [
                    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    null,
                    [
                        "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    ]
                ]
            }
        });
        let built = build_gateway_eth_upstream_params("eth_getLogs", &params)
            .expect("build should succeed")
            .expect("method should be supported");
        assert_eq!(
            built,
            serde_json::json!([{
                "fromBlock": "0x10",
                "toBlock": "latest",
                "address": [
                    "0x1111111111111111111111111111111111111111",
                    "0x2222222222222222222222222222222222222222"
                ],
                "topics": [
                    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    null,
                    [
                        "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    ]
                ]
            }])
        );
    }

    #[test]
    fn build_gateway_eth_upstream_params_translates_call_tx_object_and_tag() {
        let params = serde_json::json!({
            "tx": {
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x2222222222222222222222222222222222222222",
                "gas_limit": "0x5208",
                "gas_price": "0x3b9aca00",
                "data": "0x1234",
                "max_fee_per_gas": "0x5",
                "max_priority_fee_per_gas": "0x2",
                "access_list": [],
                "blob_versioned_hashes": ["0x01"]
            },
            "block": "pending"
        });
        let built = build_gateway_eth_upstream_params("eth_call", &params)
            .expect("build should succeed")
            .expect("method should be supported");
        assert_eq!(
            built,
            serde_json::json!([{
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x2222222222222222222222222222222222222222",
                "gas": "0x5208",
                "gasPrice": "0x3b9aca00",
                "input": "0x1234",
                "accessList": [],
                "maxFeePerGas": "0x5",
                "maxPriorityFeePerGas": "0x2",
                "blobVersionedHashes": ["0x01"]
            }, "pending"])
        );
    }
}
