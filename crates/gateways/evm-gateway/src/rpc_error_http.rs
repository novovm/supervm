use super::*;

pub(super) fn parse_named_error_counter(message: &str, name: &str) -> Option<u64> {
    let marker = format!("{name}=");
    let start = message.find(&marker)?;
    let tail = &message[start + marker.len()..];
    let digits: String = tail.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u64>().ok()
}

pub(super) fn parse_named_error_token(message: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=");
    let start = message.find(&marker)?;
    let tail = &message[start + marker.len()..];
    let token: String = tail
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect();
    if token.is_empty() {
        return None;
    }
    Some(token)
}

pub(super) fn parse_named_error_csv_tokens(message: &str, name: &str) -> Vec<String> {
    let marker = format!("{name}=");
    let Some(start) = message.find(&marker) else {
        return Vec::new();
    };
    let tail = &message[start + marker.len()..];
    let raw: String = tail
        .chars()
        .take_while(|ch| !ch.is_ascii_whitespace())
        .collect();
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(',')
        .map(str::trim)
        .filter(|token| {
            !token.is_empty()
                && token
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        })
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn gateway_error_code_for_method(method: &str, message: &str) -> i64 {
    let lower = message.to_ascii_lowercase();
    if lower.contains("standalone evm control namespace disabled on supervm host mode") {
        return -32601;
    }
    let is_evm_write_method = method == "eth_sendRawTransaction"
        || method == "eth_sendTransaction"
        || method == "evm_sendRawTransaction"
        || method == "evm_send_raw_transaction"
        || method == "evm_sendTransaction"
        || method == "evm_send_transaction"
        || method == "evm_publicSendRawTransaction"
        || method == "evm_public_send_raw_transaction"
        || method == "evm_publicSendTransaction"
        || method == "evm_public_send_transaction"
        || method == "evm_publicSendRawTransactionBatch"
        || method == "evm_public_send_raw_transaction_batch"
        || method == "evm_publicSendTransactionBatch"
        || method == "evm_public_send_transaction_batch"
        || method == "web30_sendRawTransaction"
        || method == "web30_sendTransaction";
    if is_evm_write_method && lower.contains("plugin_atomic_gate_rejected") {
        return -32036;
    }
    if is_evm_write_method && lower.contains("plugin_atomic_gate_not_ready") {
        return -32039;
    }
    if is_evm_write_method
        && (lower.contains("kyc_verified")
            || lower.contains("kyc_attestor_pubkey")
            || lower.contains("kyc_attestation_sig")
            || lower.contains("kyc attestation signature"))
    {
        return -32033;
    }
    if is_evm_write_method && lower.contains("gateway evm txpool rejected tx") {
        if let Some(reason) = parse_named_error_csv_tokens(message, "reasons")
            .first()
            .cloned()
            .or_else(|| parse_named_error_token(message, "reason"))
        {
            match reason.as_str() {
                "replacement_underpriced" => return -32034,
                "nonce_too_low" => return -32035,
                "nonce_too_high" => return -32037,
                "pool_full" => return -32038,
                "rejected" => return -32030,
                _ => {}
            }
        }
        // Backward compatibility: old format without reason/reasons keeps counter-based mapping.
        if let Some(reason) = parse_named_error_token(message, "reason") {
            match reason.as_str() {
                "replacement_underpriced" => return -32034,
                "nonce_too_low" => return -32035,
                "nonce_too_high" => return -32037,
                "pool_full" => return -32038,
                "rejected" => return -32030,
                _ => {}
            }
        }
        let dropped_underpriced =
            parse_named_error_counter(message, "dropped_underpriced").unwrap_or(0);
        let dropped_nonce_too_low =
            parse_named_error_counter(message, "dropped_nonce_too_low").unwrap_or(0);
        let dropped_nonce_gap =
            parse_named_error_counter(message, "dropped_nonce_gap").unwrap_or(0);
        let dropped_over_capacity =
            parse_named_error_counter(message, "dropped_over_capacity").unwrap_or(0);
        if dropped_underpriced > 0 {
            return -32034;
        }
        if dropped_nonce_too_low > 0 {
            return -32035;
        }
        if dropped_nonce_gap > 0 {
            return -32037;
        }
        if dropped_over_capacity > 0 {
            return -32038;
        }
        return -32030;
    }
    if method == "eth_sendRawTransaction" || method == "eth_sendTransaction" {
        if lower.contains("blob (type 3) write path disabled") {
            return -32031;
        }
        if lower.contains("public broadcast failed") || lower.contains("public-broadcast executor")
        {
            return -32040;
        }
        if lower.contains("nonce mismatch")
            || lower.contains("chain_id mismatch")
            || lower.contains("binding")
            || lower.contains("domain mismatch")
        {
            return -32033;
        }
    }
    if method == "eth_getTransactionCount"
        && (lower.contains("binding")
            || lower.contains("uca_id mismatch")
            || lower.contains("nonce")
            || lower.contains("chain_id")
            || lower.contains("block")
            || lower.contains("tag")
            || lower.contains("address")
            || lower.contains("hex"))
    {
        return -32033;
    }
    if method == "eth_getBalance"
        && (lower.contains("address")
            || lower.contains("block")
            || lower.contains("tag")
            || lower.contains("hex"))
    {
        return -32033;
    }
    if (method == "eth_estimateGas" || method == "eth_call")
        && (lower.contains("from")
            || lower.contains("to")
            || lower.contains("data")
            || lower.contains("input")
            || lower.contains("hex")
            || lower.contains("chain_id"))
    {
        return -32033;
    }
    if method == "eth_getCode" && (lower.contains("address") || lower.contains("hex")) {
        return -32033;
    }
    if method == "eth_getStorageAt"
        && (lower.contains("address")
            || lower.contains("slot")
            || lower.contains("position")
            || lower.contains("hex"))
    {
        return -32033;
    }
    if method == "eth_getProof"
        && (lower.contains("address")
            || lower.contains("storage")
            || lower.contains("key")
            || lower.contains("slot")
            || lower.contains("position")
            || lower.contains("block")
            || lower.contains("tag")
            || lower.contains("hex"))
    {
        return -32033;
    }
    if (method == "eth_getBlockByNumber"
        || method == "eth_getBlockByHash"
        || method == "eth_getTransactionByBlockNumberAndIndex"
        || method == "eth_getTransactionByBlockHashAndIndex"
        || method == "eth_getBlockTransactionCountByNumber"
        || method == "eth_getBlockTransactionCountByHash"
        || method == "eth_getBlockReceipts"
        || method == "eth_getUncleCountByBlockNumber"
        || method == "eth_getUncleCountByBlockHash"
        || method == "eth_getUncleByBlockNumberAndIndex"
        || method == "eth_getUncleByBlockHashAndIndex"
        || method == "eth_feeHistory"
        || method == "eth_getLogs")
        && (lower.contains("block")
            || lower.contains("hash")
            || lower.contains("index")
            || lower.contains("address")
            || lower.contains("topic")
            || lower.contains("tag")
            || lower.contains("percentile")
            || lower.contains("hex"))
    {
        return -32033;
    }
    if (method == "eth_newFilter"
        || method == "eth_subscribe"
        || method == "eth_unsubscribe"
        || method == "eth_newBlockFilter"
        || method == "eth_newPendingTransactionFilter"
        || method == "eth_getFilterChanges"
        || method == "eth_getFilterLogs"
        || method == "eth_uninstallFilter")
        && (lower.contains("filter")
            || lower.contains("subscription")
            || lower.contains("subscribe")
            || lower.contains("unsubscribe")
            || lower.contains("id")
            || lower.contains("block")
            || lower.contains("hash")
            || lower.contains("address")
            || lower.contains("topic")
            || lower.contains("tag")
            || lower.contains("hex"))
    {
        return -32033;
    }
    if (method == "eth_getTransactionByHash" || method == "eth_getTransactionReceipt")
        && (lower.contains("tx_hash")
            || lower.contains("hash")
            || lower.contains("hex")
            || lower.contains("size mismatch"))
    {
        return -32033;
    }
    if (method == "web30_sendRawTransaction" || method == "web30_sendTransaction")
        && (lower.contains("nonce mismatch")
            || lower.contains("chain_id mismatch")
            || lower.contains("binding")
            || lower.contains("domain mismatch")
            || lower.contains("nonce")
            || lower.contains("address")
            || lower.contains("external_address")
            || lower.contains("payload")
            || lower.contains("privacy")
            || lower.contains("ring_members")
            || lower.contains("signer_index")
            || lower.contains("stealth"))
    {
        return -32033;
    }
    if method == "ua_createUca"
        || method == "ua_rotatePrimaryKey"
        || method == "ua_bindPersona"
        || method == "ua_revokePersona"
        || method == "ua_getBindingOwner"
        || method == "ua_setPolicy"
    {
        return -32010;
    }
    -32000
}

pub(super) fn gateway_error_message_for_method(
    method: &str,
    code: i64,
    raw_message: &str,
) -> String {
    let is_evm_write_method = method == "eth_sendRawTransaction"
        || method == "eth_sendTransaction"
        || method == "evm_sendRawTransaction"
        || method == "evm_send_raw_transaction"
        || method == "evm_sendTransaction"
        || method == "evm_send_transaction"
        || method == "evm_publicSendRawTransaction"
        || method == "evm_public_send_raw_transaction"
        || method == "evm_publicSendTransaction"
        || method == "evm_public_send_transaction"
        || method == "evm_publicSendRawTransactionBatch"
        || method == "evm_public_send_raw_transaction_batch"
        || method == "evm_publicSendTransactionBatch"
        || method == "evm_public_send_transaction_batch"
        || method == "web30_sendRawTransaction"
        || method == "web30_sendTransaction";
    if is_evm_write_method {
        return match code {
            -32034 => "replacement transaction underpriced".to_string(),
            -32035 => "nonce too low".to_string(),
            -32036 => "atomic intent rejected".to_string(),
            -32037 => "nonce too high".to_string(),
            -32038 => "txpool is full".to_string(),
            -32039 => "atomic intent not ready".to_string(),
            -32040 => "public broadcast failed".to_string(),
            -32030 => "transaction rejected".to_string(),
            _ => raw_message.to_string(),
        };
    }
    raw_message.to_string()
}

pub(super) fn gateway_error_data_for_method(
    method: &str,
    code: i64,
    raw_message: &str,
) -> Option<serde_json::Value> {
    let is_evm_write_method = method == "eth_sendRawTransaction"
        || method == "eth_sendTransaction"
        || method == "evm_sendRawTransaction"
        || method == "evm_send_raw_transaction"
        || method == "evm_sendTransaction"
        || method == "evm_send_transaction"
        || method == "evm_publicSendRawTransaction"
        || method == "evm_public_send_raw_transaction"
        || method == "evm_publicSendTransaction"
        || method == "evm_public_send_transaction"
        || method == "evm_publicSendRawTransactionBatch"
        || method == "evm_public_send_raw_transaction_batch"
        || method == "evm_publicSendTransactionBatch"
        || method == "evm_public_send_transaction_batch"
        || method == "web30_sendRawTransaction"
        || method == "web30_sendTransaction";
    if !is_evm_write_method {
        return None;
    }
    let lower = raw_message.to_ascii_lowercase();
    if lower.contains("plugin_atomic_gate_rejected") {
        let reasons = {
            let parsed = parse_named_error_csv_tokens(raw_message, "reasons");
            if parsed.is_empty() {
                vec!["rejected".to_string()]
            } else {
                parsed
            }
        };
        return Some(serde_json::json!({
            "category": "atomic_gate",
            "state": "rejected",
            "rejected_receipts": parse_named_error_counter(raw_message, "rejected_receipts"),
            "reasons": reasons,
        }));
    }
    if lower.contains("plugin_atomic_gate_not_ready") {
        return Some(serde_json::json!({
            "category": "atomic_gate",
            "state": "not_ready",
            "ready_items": parse_named_error_counter(raw_message, "ready_items"),
            "matched_ready": parse_named_error_counter(raw_message, "matched_ready"),
        }));
    }
    if lower.contains("public broadcast failed") || lower.contains("public-broadcast executor") {
        return Some(serde_json::json!({
            "category": "public_broadcast",
            "reason": "broadcast_failed",
            "attempts": parse_named_error_counter(raw_message, "attempts"),
            "chain_id": parse_named_error_counter(raw_message, "chain_id"),
            "tx_hash": parse_named_error_token(raw_message, "tx_hash"),
        }));
    }
    if !lower.contains("gateway evm txpool rejected tx") {
        return None;
    }
    let reasons = {
        let parsed = parse_named_error_csv_tokens(raw_message, "reasons");
        if parsed.is_empty() {
            vec![
                parse_named_error_token(raw_message, "reason").unwrap_or_else(|| {
                    match code {
                        -32034 => "replacement_underpriced",
                        -32035 => "nonce_too_low",
                        -32036 => "atomic_rejected",
                        -32037 => "nonce_too_high",
                        -32038 => "pool_full",
                        -32039 => "atomic_not_ready",
                        -32030 => "rejected",
                        _ => "unknown",
                    }
                    .to_string()
                }),
            ]
        } else {
            parsed
        }
    };
    let reason = reasons
        .first()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    Some(serde_json::json!({
        "category": "txpool_reject",
        "reason": reason,
        "reasons": reasons,
        "requested": parse_named_error_counter(raw_message, "requested"),
        "accepted": parse_named_error_counter(raw_message, "accepted"),
        "dropped": parse_named_error_counter(raw_message, "dropped"),
        "dropped_underpriced": parse_named_error_counter(raw_message, "dropped_underpriced"),
        "dropped_nonce_too_low": parse_named_error_counter(raw_message, "dropped_nonce_too_low"),
        "dropped_nonce_gap": parse_named_error_counter(raw_message, "dropped_nonce_gap"),
        "dropped_over_capacity": parse_named_error_counter(raw_message, "dropped_over_capacity"),
    }))
}

pub(super) fn rpc_error_body_with_data(
    id: serde_json::Value,
    code: i64,
    message: &str,
    data: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut error = serde_json::Map::new();
    error.insert("code".to_string(), serde_json::json!(code));
    error.insert("message".to_string(), serde_json::json!(message));
    if let Some(data) = data {
        error.insert("data".to_string(), data);
    }
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": serde_json::Value::Object(error),
    })
}

pub(super) fn rpc_error_body(id: serde_json::Value, code: i64, message: &str) -> serde_json::Value {
    rpc_error_body_with_data(id, code, message, None)
}

pub(super) fn respond_json_http(
    request: tiny_http::Request,
    status: u16,
    body: &serde_json::Value,
) -> Result<()> {
    let payload = serde_json::to_string(body).context("serialize rpc response json failed")?;
    let mut response =
        tiny_http::Response::from_string(payload).with_status_code(tiny_http::StatusCode(status));
    if let Ok(header) =
        tiny_http::Header::from_bytes(b"Content-Type".to_vec(), b"application/json".to_vec())
    {
        response = response.with_header(header);
    }
    request
        .respond(response)
        .map_err(|e| anyhow::anyhow!("gateway response send failed: {e}"))?;
    Ok(())
}

pub(super) fn ensure_parent_dir(path: &Path, label: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create {} parent dir failed: {}", label, parent.display())
            })?;
        }
    }
    Ok(())
}

pub(super) fn ensure_dir(path: &Path, label: &str) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("create {} failed: {}", label, path.display()))?;
    Ok(())
}
