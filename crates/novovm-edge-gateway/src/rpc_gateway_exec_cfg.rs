use super::*;

pub(super) fn gateway_evm_atomic_broadcast_exec_path() -> Option<PathBuf> {
    string_env_nonempty("NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC").map(PathBuf::from)
}

pub(super) fn gateway_evm_atomic_broadcast_exec_retry_default() -> u64 {
    u64_env(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_DEFAULT,
    )
    .min(16)
}

pub(super) fn gateway_evm_atomic_broadcast_exec_timeout_ms_default() -> u64 {
    let timeout_ms = u64_env(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT,
    );
    if timeout_ms == 0 {
        0
    } else {
        timeout_ms.min(300_000)
    }
}

pub(super) fn gateway_evm_atomic_broadcast_exec_retry_backoff_ms_default() -> u64 {
    u64_env(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_DEFAULT,
    )
    .min(10_000)
}

pub(super) fn gateway_evm_atomic_broadcast_exec_batch_hard_max() -> u64 {
    u64_env(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_HARD_MAX",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_HARD_MAX,
    )
    .clamp(1, 4096)
}

pub(super) fn gateway_evm_atomic_broadcast_exec_batch_default() -> u64 {
    let hard_max = gateway_evm_atomic_broadcast_exec_batch_hard_max();
    u64_env(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_DEFAULT",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_DEFAULT,
    )
    .clamp(1, hard_max)
}

pub(super) fn gateway_atomic_broadcast_force_native(params: &serde_json::Value) -> bool {
    param_as_bool(params, "native")
        .or_else(|| param_as_bool(params, "force_native"))
        .unwrap_or(false)
}

pub(super) fn gateway_atomic_broadcast_use_external_executor(params: &serde_json::Value) -> bool {
    param_as_bool(params, "use_external_executor")
        .or_else(|| param_as_bool(params, "exec"))
        .unwrap_or(false)
}

pub(super) fn gateway_eth_public_broadcast_exec_path() -> Option<PathBuf> {
    string_env_nonempty("NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC").map(PathBuf::from)
}

pub(super) fn gateway_eth_public_broadcast_exec_retry_default() -> u64 {
    u64_env(
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY",
        GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_DEFAULT,
    )
    .min(16)
}

pub(super) fn gateway_eth_public_broadcast_exec_timeout_ms_default() -> u64 {
    let timeout_ms = u64_env(
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_TIMEOUT_MS",
        GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT,
    );
    if timeout_ms == 0 {
        0
    } else {
        timeout_ms.min(300_000)
    }
}

pub(super) fn gateway_eth_public_broadcast_exec_retry_backoff_ms_default() -> u64 {
    u64_env(
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_BACKOFF_MS",
        GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_DEFAULT,
    )
    .min(10_000)
}

#[derive(Clone, Copy)]
pub(super) struct GatewayEthPublicBroadcastPayload<'a> {
    pub(super) raw_tx: Option<&'a [u8]>,
    pub(super) tx_ir_bincode: Option<&'a [u8]>,
}

pub(super) fn build_gateway_eth_public_broadcast_executor_request(
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
) -> serde_json::Value {
    let mut req = serde_json::json!({
        "chain_id": format!("0x{:x}", chain_id),
        "tx_hash": format!("0x{}", to_hex(tx_hash)),
    });
    if let Some(raw_tx) = payload.raw_tx.filter(|payload| !payload.is_empty()) {
        if let Some(map) = req.as_object_mut() {
            map.insert(
                "raw_tx".to_string(),
                serde_json::Value::String(format!("0x{}", to_hex(raw_tx))),
            );
            map.insert(
                "raw_tx_len".to_string(),
                serde_json::Value::String(format!("0x{:x}", raw_tx.len())),
            );
        }
    }
    if let Some(tx_ir_bincode) = payload.tx_ir_bincode.filter(|payload| !payload.is_empty()) {
        if let Some(map) = req.as_object_mut() {
            map.insert(
                "tx_ir_bincode".to_string(),
                serde_json::Value::String(format!("0x{}", to_hex(tx_ir_bincode))),
            );
            map.insert(
                "tx_ir_format".to_string(),
                serde_json::Value::String("bincode_v1".to_string()),
            );
        }
    }
    req
}

pub(super) fn validate_gateway_eth_public_broadcast_executor_output(
    output: &str,
    chain_id: u64,
    tx_hash: &[u8; 32],
) -> Result<()> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(map) = value.as_object() else {
        return Ok(());
    };
    if let Some(flag) = map
        .get("broadcasted")
        .or_else(|| map.get("ok"))
        .and_then(serde_json::Value::as_bool)
    {
        if !flag {
            bail!("eth public-broadcast executor reported broadcasted=false");
        }
    }
    if let Some(error) = map.get("error").and_then(serde_json::Value::as_str) {
        let reason = error.trim();
        if !reason.is_empty() {
            bail!(
                "eth public-broadcast executor reported error: tx_hash=0x{} reason={}",
                to_hex(tx_hash),
                reason
            );
        }
    }
    if let Some(raw_tx_hash) = map.get("tx_hash").or_else(|| map.get("txHash")) {
        let Some(tx_hash_hex) = raw_tx_hash.as_str() else {
            bail!("eth public-broadcast executor tx_hash must be string");
        };
        let actual = parse_hex32_from_string(tx_hash_hex, "executor.tx_hash")
            .context("decode executor tx_hash failed")?;
        if actual != *tx_hash {
            bail!(
                "eth public-broadcast executor tx_hash mismatch: expected=0x{} actual=0x{}",
                to_hex(tx_hash),
                to_hex(&actual)
            );
        }
    }
    if let Some(raw_chain_id) = map.get("chain_id").or_else(|| map.get("chainId")) {
        let Some(actual_chain_id) = value_to_u64(raw_chain_id) else {
            bail!("eth public-broadcast executor chain_id must be decimal or hex number");
        };
        if actual_chain_id != chain_id {
            bail!(
                "eth public-broadcast executor chain_id mismatch: expected={} actual={}",
                chain_id,
                actual_chain_id
            );
        }
    }
    Ok(())
}

pub(super) fn execute_gateway_eth_public_broadcast(
    exec_path: &Path,
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
    timeout_ms: u64,
) -> Result<String> {
    let req = build_gateway_eth_public_broadcast_executor_request(chain_id, tx_hash, payload);
    let req_body =
        serde_json::to_vec(&req).context("serialize eth public-broadcast request failed")?;
    let mut child = Command::new(exec_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "spawn eth public-broadcast executor failed: {}",
                exec_path.display()
            )
        })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(&req_body).with_context(|| {
            format!(
                "write eth public-broadcast request into executor stdin failed: {}",
                exec_path.display()
            )
        })?;
    }
    let output = if timeout_ms == 0 {
        child.wait_with_output().with_context(|| {
            format!(
                "wait eth public-broadcast executor output failed: {}",
                exec_path.display()
            )
        })?
    } else {
        let timeout = Duration::from_millis(timeout_ms);
        let start = SystemTime::now();
        loop {
            match child.try_wait().with_context(|| {
                format!(
                    "poll eth public-broadcast executor failed: {}",
                    exec_path.display()
                )
            })? {
                Some(_) => {
                    break child.wait_with_output().with_context(|| {
                        format!(
                            "read eth public-broadcast executor output failed: {}",
                            exec_path.display()
                        )
                    })?;
                }
                None => {
                    if start.elapsed().unwrap_or_else(|_| Duration::from_millis(0)) >= timeout {
                        let _ = child.kill();
                        let timed_out_output = child.wait_with_output().with_context(|| {
                            format!(
                                "read timed-out eth public-broadcast executor output failed: {}",
                                exec_path.display()
                            )
                        })?;
                        let stderr = String::from_utf8_lossy(&timed_out_output.stderr);
                        bail!(
                            "eth public-broadcast executor timed out: timeout_ms={} stderr={}",
                            timeout_ms,
                            stderr.trim()
                        );
                    }
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "eth public-broadcast executor exit={} stderr={}",
            output.status,
            stderr.trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    validate_gateway_eth_public_broadcast_executor_output(&stdout, chain_id, tx_hash)?;
    Ok(stdout)
}

pub(super) fn execute_gateway_eth_public_broadcast_with_retry(
    exec_path: &Path,
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
    retry: u64,
    timeout_ms: u64,
    retry_backoff_ms: u64,
) -> std::result::Result<(String, u64), (anyhow::Error, u64)> {
    let mut attempts = 0u64;
    loop {
        attempts = attempts.saturating_add(1);
        match execute_gateway_eth_public_broadcast(
            exec_path, chain_id, tx_hash, payload, timeout_ms,
        ) {
            Ok(output) => return Ok((output, attempts)),
            Err(e) => {
                if attempts > retry {
                    return Err((e, attempts));
                }
                if retry_backoff_ms > 0 {
                    thread::sleep(Duration::from_millis(retry_backoff_ms));
                }
            }
        }
    }
}

pub(super) fn maybe_execute_gateway_eth_public_broadcast(
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
) -> Result<Option<(String, u64, String)>> {
    let Some(exec_path) = gateway_eth_public_broadcast_exec_path() else {
        return Ok(None);
    };
    let retry = gateway_eth_public_broadcast_exec_retry_default();
    let timeout_ms = gateway_eth_public_broadcast_exec_timeout_ms_default();
    let retry_backoff_ms = gateway_eth_public_broadcast_exec_retry_backoff_ms_default();
    match execute_gateway_eth_public_broadcast_with_retry(
        exec_path.as_path(),
        chain_id,
        tx_hash,
        payload,
        retry,
        timeout_ms,
        retry_backoff_ms,
    ) {
        Ok((output, attempts)) => Ok(Some((output, attempts, exec_path.display().to_string()))),
        Err((e, attempts)) => {
            bail!(
                "public broadcast failed: chain_id={} tx_hash=0x{} attempts={} err={}",
                chain_id,
                to_hex(tx_hash),
                attempts,
                e
            );
        }
    }
}

pub(super) fn validate_gateway_atomic_broadcast_executor_output(
    output: &str,
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
) -> Result<()> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(map) = value.as_object() else {
        return Ok(());
    };

    if let Some(flag) = map
        .get("broadcasted")
        .or_else(|| map.get("ok"))
        .and_then(serde_json::Value::as_bool)
    {
        if !flag {
            bail!("evm atomic-broadcast executor reported broadcasted=false");
        }
    }

    if let Some(error) = map.get("error").and_then(serde_json::Value::as_str) {
        let reason = error.trim();
        if !reason.is_empty() {
            bail!(
                "evm atomic-broadcast executor reported error: intent_id={} reason={}",
                ticket.intent_id,
                reason
            );
        }
    }

    if let Some(raw_intent_id) = map.get("intent_id").or_else(|| map.get("intentId")) {
        let Some(intent_id) = raw_intent_id.as_str() else {
            bail!("evm atomic-broadcast executor intent_id must be string");
        };
        if intent_id != ticket.intent_id {
            bail!(
                "evm atomic-broadcast executor intent_id mismatch: expected={} actual={}",
                ticket.intent_id,
                intent_id
            );
        }
    }

    if let Some(raw_tx_hash) = map.get("tx_hash").or_else(|| map.get("txHash")) {
        let Some(tx_hash_hex) = raw_tx_hash.as_str() else {
            bail!("evm atomic-broadcast executor tx_hash must be string");
        };
        let tx_hash = parse_hex32_from_string(tx_hash_hex, "executor.tx_hash")
            .context("decode executor tx_hash failed")?;
        if tx_hash != ticket.tx_hash {
            bail!(
                "evm atomic-broadcast executor tx_hash mismatch: expected=0x{} actual=0x{}",
                to_hex(&ticket.tx_hash),
                to_hex(&tx_hash)
            );
        }
    }

    if let Some(raw_chain_id) = map.get("chain_id").or_else(|| map.get("chainId")) {
        let Some(chain_id) = value_to_u64(raw_chain_id) else {
            bail!("evm atomic-broadcast executor chain_id must be decimal or hex number");
        };
        if chain_id != ticket.chain_id {
            bail!(
                "evm atomic-broadcast executor chain_id mismatch: expected={} actual={}",
                ticket.chain_id,
                chain_id
            );
        }
    }

    Ok(())
}

pub(super) fn build_gateway_atomic_broadcast_executor_request(
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
    tx_ir_bincode: Option<&[u8]>,
) -> serde_json::Value {
    let mut req = serde_json::json!({
        "intent_id": ticket.intent_id,
        "chain_id": format!("0x{:x}", ticket.chain_id),
        "tx_hash": format!("0x{}", to_hex(&ticket.tx_hash)),
    });
    if let Some(tx_ir_bincode) = tx_ir_bincode.filter(|payload| !payload.is_empty()) {
        if let Some(map) = req.as_object_mut() {
            map.insert(
                "tx_ir_bincode".to_string(),
                serde_json::Value::String(format!("0x{}", to_hex(tx_ir_bincode))),
            );
            map.insert(
                "tx_ir_format".to_string(),
                serde_json::Value::String("bincode_v1".to_string()),
            );
        }
    }
    req
}

pub(super) fn decode_gateway_atomic_broadcast_tx_ir_bincode(payload: &[u8]) -> Result<TxIR> {
    if payload.is_empty() {
        bail!("atomic-broadcast tx_ir_bincode is empty");
    }
    if let Ok(tx) = bincode::deserialize::<TxIR>(payload) {
        return Ok(tx);
    }
    if let Ok(mut txs) = bincode::deserialize::<Vec<TxIR>>(payload) {
        if txs.len() == 1 {
            return Ok(txs.remove(0));
        }
        bail!(
            "atomic-broadcast tx_ir_bincode must contain exactly one tx, got {}",
            txs.len()
        );
    }
    bail!("decode atomic-broadcast tx_ir_bincode failed");
}
