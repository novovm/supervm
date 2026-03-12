use super::*;

pub(super) fn vec_to_32(raw: &[u8], field: &str) -> Result<[u8; 32]> {
    if raw.len() != 32 {
        bail!("{} size mismatch: expected 32 got {}", field, raw.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(raw);
    Ok(out)
}

pub(super) fn value_to_u64(v: &serde_json::Value) -> Option<u64> {
    match v {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => parse_u64_decimal_or_hex(s),
        _ => None,
    }
}

pub(super) fn value_to_u128(v: &serde_json::Value) -> Option<u128> {
    match v {
        serde_json::Value::Number(n) => n.as_u64().map(|v| v as u128),
        serde_json::Value::String(s) => parse_u128_decimal_or_hex(s),
        _ => None,
    }
}

pub(super) fn parse_u64_decimal_or_hex(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if hex.is_empty() {
            Some(0)
        } else {
            u64::from_str_radix(hex, 16).ok()
        }
    } else {
        trimmed.parse::<u64>().ok()
    }
}

pub(super) fn parse_u128_decimal_or_hex(raw: &str) -> Option<u128> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if hex.is_empty() {
            Some(0)
        } else {
            u128::from_str_radix(hex, 16).ok()
        }
    } else {
        trimmed.parse::<u128>().ok()
    }
}

pub(super) fn value_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.trim().to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub(super) fn is_block_tag_candidate(tag: &str) -> bool {
    let normalized = tag.trim().trim_matches('"');
    if normalized.is_empty()
        || normalized.eq_ignore_ascii_case("latest")
        || normalized.eq_ignore_ascii_case("safe")
        || normalized.eq_ignore_ascii_case("finalized")
        || normalized.eq_ignore_ascii_case("pending")
        || normalized.eq_ignore_ascii_case("earliest")
    {
        return true;
    }
    parse_u64_decimal_or_hex(normalized).is_some()
}

pub(super) fn first_scalar_param_string(params: &serde_json::Value) -> Option<String> {
    params.as_array().and_then(|arr| {
        arr.iter().find_map(|v| match v {
            serde_json::Value::Object(_) | serde_json::Value::Array(_) => None,
            _ => value_to_string(v).and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
        })
    })
}

pub(super) fn last_block_tag_like_param_string(params: &serde_json::Value) -> Option<String> {
    params.as_array().and_then(|arr| {
        arr.iter().rev().find_map(|v| match v {
            serde_json::Value::Object(_) | serde_json::Value::Array(_) => None,
            _ => value_to_string(v).and_then(|text| {
                if is_block_tag_candidate(&text) {
                    Some(text)
                } else {
                    None
                }
            }),
        })
    })
}

pub(super) fn non_object_param_at<'a>(
    params: &'a serde_json::Value,
    index: usize,
) -> Option<&'a serde_json::Value> {
    params
        .as_array()
        .and_then(|arr| arr.iter().filter(|v| !v.is_object()).nth(index))
}

pub(super) fn params_object_with_any_keys<'a>(
    params: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    match params {
        serde_json::Value::Object(map) => Some(map),
        serde_json::Value::Array(arr) => {
            for item in arr {
                let Some(map) = item.as_object() else {
                    continue;
                };
                if keys.iter().any(|key| map.contains_key(*key)) {
                    return Some(map);
                }
            }
            None
        }
        _ => None,
    }
}

pub(super) fn params_object_with_any_keys_or_nested_filter<'a>(
    params: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    let matches_filter_keys = |map: &'a serde_json::Map<String, serde_json::Value>| {
        keys.is_empty() || keys.iter().any(|key| map.contains_key(*key))
    };
    match params {
        serde_json::Value::Object(map) => {
            if matches_filter_keys(map) {
                return Some(map);
            }
            map.get("filter")
                .and_then(serde_json::Value::as_object)
                .filter(|nested| matches_filter_keys(nested))
        }
        serde_json::Value::Array(arr) => arr.iter().find_map(|item| {
            let map = item.as_object()?;
            if matches_filter_keys(map) {
                return Some(map);
            }
            map.get("filter")
                .and_then(serde_json::Value::as_object)
                .filter(|nested| matches_filter_keys(nested))
        }),
        _ => None,
    }
}

pub(super) fn parse_eth_subscribe_kind(params: &serde_json::Value) -> Option<String> {
    param_as_string(params, "kind")
        .or_else(|| param_as_string(params, "subscription"))
        .or_else(|| param_as_string(params, "type"))
        .or_else(|| param_as_string(params, "event"))
        .or_else(|| non_object_param_at(params, 0).and_then(value_to_string))
}

pub(super) fn first_address_like_scalar_param_string(params: &serde_json::Value) -> Option<String> {
    params.as_array().and_then(|arr| {
        arr.iter().find_map(|v| match v {
            serde_json::Value::Object(_) | serde_json::Value::Array(_) => None,
            _ => value_to_string(v).and_then(|text| {
                let decoded = decode_hex_bytes(&text, "address").ok()?;
                if decoded.len() == 20 {
                    Some(text)
                } else {
                    None
                }
            }),
        })
    })
}

pub(super) fn params_primary_object(
    params: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    match params {
        serde_json::Value::Object(map) => Some(map),
        serde_json::Value::Array(arr) => arr.iter().find_map(serde_json::Value::as_object),
        _ => None,
    }
}

pub(super) fn param_value_from_params<'a>(
    params: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    match params {
        serde_json::Value::Object(map) => map.get(key),
        serde_json::Value::Array(arr) => {
            for item in arr {
                let Some(map) = item.as_object() else {
                    continue;
                };
                if let Some(value) = map.get(key) {
                    return Some(value);
                }
            }
            None
        }
        _ => None,
    }
}

pub(super) fn param_as_u128(params: &serde_json::Value, key: &str) -> Option<u128> {
    param_value_from_params(params, key).and_then(value_to_u128)
}

pub(super) fn param_tx_object(params: &serde_json::Value) -> Option<&serde_json::Value> {
    let map = params_object_with_any_keys(params, &["tx"])?;
    match map.get("tx") {
        Some(tx_obj @ serde_json::Value::Object(_)) => Some(tx_obj),
        _ => None,
    }
}

pub(super) fn param_as_u64_any_with_tx(params: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(value) = param_as_u64(params, key) {
            return Some(value);
        }
    }
    let tx = param_tx_object(params)?;
    for key in keys {
        if let Some(value) = param_as_u64(tx, key) {
            return Some(value);
        }
    }
    None
}

pub(super) fn param_as_u128_any_with_tx(params: &serde_json::Value, keys: &[&str]) -> Option<u128> {
    for key in keys {
        if let Some(value) = param_as_u128(params, key) {
            return Some(value);
        }
    }
    let tx = param_tx_object(params)?;
    for key in keys {
        if let Some(value) = param_as_u128(tx, key) {
            return Some(value);
        }
    }
    None
}

pub(super) fn param_as_string_any_with_tx(
    params: &serde_json::Value,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        if let Some(value) = param_as_string(params, key) {
            return Some(value);
        }
    }
    let tx = param_tx_object(params)?;
    for key in keys {
        if let Some(value) = param_as_string(tx, key) {
            return Some(value);
        }
    }
    None
}

pub(super) fn param_as_bool_any_with_tx(params: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(value) = param_as_bool(params, key) {
            return Some(value);
        }
    }
    let tx = param_tx_object(params)?;
    for key in keys {
        if let Some(value) = param_as_bool(tx, key) {
            return Some(value);
        }
    }
    None
}

pub(super) fn value_to_bool(v: &serde_json::Value) -> Option<bool> {
    match v {
        serde_json::Value::Bool(b) => Some(*b),
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.eq_ignore_ascii_case("true") || t == "1" {
                Some(true)
            } else if t.eq_ignore_ascii_case("false") || t == "0" {
                Some(false)
            } else {
                None
            }
        }
        serde_json::Value::Number(n) => n.as_u64().map(|v| v != 0),
        _ => None,
    }
}

pub(super) fn param_as_u64(params: &serde_json::Value, key: &str) -> Option<u64> {
    param_value_from_params(params, key).and_then(value_to_u64)
}

pub(super) fn param_as_string(params: &serde_json::Value, key: &str) -> Option<String> {
    param_value_from_params(params, key).and_then(value_to_string)
}

pub(super) fn param_as_bool(params: &serde_json::Value, key: &str) -> Option<bool> {
    param_value_from_params(params, key).and_then(value_to_bool)
}

pub(super) fn parse_account_role(params: &serde_json::Value) -> Result<AccountRole> {
    let raw = param_as_string(params, "role")
        .unwrap_or_else(|| "owner".to_string())
        .to_ascii_lowercase();
    match raw.as_str() {
        "owner" => Ok(AccountRole::Owner),
        "delegate" => Ok(AccountRole::Delegate),
        "session" | "sessionkey" | "session_key" => Ok(AccountRole::SessionKey),
        _ => bail!("invalid role: {}; valid: owner|delegate|session_key", raw),
    }
}

pub(super) fn parse_persona_type(params: &serde_json::Value, key: &str) -> Result<PersonaType> {
    let raw = param_as_string(params, key)
        .ok_or_else(|| anyhow::anyhow!("{} is required", key))?
        .to_ascii_lowercase();
    Ok(match raw.as_str() {
        "web30" => PersonaType::Web30,
        "evm" => PersonaType::Evm,
        "bitcoin" | "btc" => PersonaType::Bitcoin,
        "solana" | "sol" => PersonaType::Solana,
        other => PersonaType::Other(other.to_string()),
    })
}

pub(super) fn parse_primary_key_ref(params: &serde_json::Value, uca_id: &str) -> Result<Vec<u8>> {
    if let Some(raw) = param_as_string(params, "primary_key_ref") {
        return decode_hex_bytes(&raw, "primary_key_ref");
    }
    let mut hasher = Sha256::new();
    hasher.update(GATEWAY_UA_PRIMARY_KEY_DOMAIN);
    hasher.update(uca_id.as_bytes());
    Ok(hasher.finalize().to_vec())
}

pub(super) fn parse_external_address(params: &serde_json::Value, key: &str) -> Result<Vec<u8>> {
    let raw = param_as_string(params, key).ok_or_else(|| anyhow::anyhow!("{} is required", key))?;
    decode_hex_bytes(&raw, key)
}

pub(super) fn pick_first_nonempty_string(
    map: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        if let Some(value) = map.get(*key).and_then(value_to_string) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub(super) fn extract_web30_raw_payload_param(params: &serde_json::Value) -> Option<Vec<u8>> {
    const CANDIDATE_KEYS: &[&str] = &[
        "raw_tx",
        "rawTransaction",
        "raw_transaction",
        "raw",
        "payload_hex",
    ];
    let raw = match params {
        serde_json::Value::Object(map) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
        serde_json::Value::Array(arr) => match arr.first() {
            Some(serde_json::Value::Object(map)) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
            Some(first) => value_to_string(first).and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            None => None,
        },
        _ => None,
    }?;
    decode_hex_bytes(&raw, "raw_tx").ok()
}

pub(super) fn extract_web30_tx_payload(params: &serde_json::Value) -> Result<Vec<u8>> {
    if let Some(raw_hex) = extract_web30_raw_payload_param(params) {
        return Ok(raw_hex);
    }
    if let serde_json::Value::Object(map) = params {
        if let Some(value) = map.get("payload").and_then(value_to_string) {
            let trimmed = value.trim();
            if let Some(hex) = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                if !hex.is_empty() {
                    return decode_hex_bytes(trimmed, "payload");
                }
            }
            if !trimmed.is_empty() {
                return Ok(trimmed.as_bytes().to_vec());
            }
        }
        if let Some(tx_obj) = map.get("tx") {
            return serde_json::to_vec(tx_obj)
                .context("serialize tx object for web30 payload failed");
        }
    }
    serde_json::to_vec(params).context("serialize web30 transaction params payload failed")
}

pub(super) fn param_privacy_object(
    params: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let map = params_primary_object(params)?;
    if let Some(privacy) = map.get("privacy").and_then(serde_json::Value::as_object) {
        return Some(privacy);
    }
    let tx = map.get("tx").and_then(serde_json::Value::as_object)?;
    tx.get("privacy").and_then(serde_json::Value::as_object)
}

pub(super) fn parse_hex32_from_string(raw: &str, field: &str) -> Result<[u8; 32]> {
    let bytes = decode_hex_bytes(raw, field)?;
    vec_to_32(&bytes, field)
}

pub(super) fn parse_gateway_web30_privacy_plan(
    params: &serde_json::Value,
) -> Result<Option<GatewayWeb30PrivacyTxPlan>> {
    let Some(privacy) = param_privacy_object(params) else {
        return Ok(None);
    };

    let value = privacy
        .get("value")
        .and_then(value_to_u128)
        .or_else(|| param_as_u128_any_with_tx(params, &["value"]))
        .unwrap_or(0);
    let gas_limit = privacy
        .get("gas_limit")
        .or_else(|| privacy.get("gasLimit"))
        .or_else(|| privacy.get("gas"))
        .and_then(value_to_u64)
        .or_else(|| param_as_u64_any_with_tx(params, &["gas_limit", "gasLimit", "gas"]))
        .unwrap_or(21_000);
    let gas_price = privacy
        .get("gas_price")
        .or_else(|| privacy.get("gasPrice"))
        .or_else(|| privacy.get("max_fee_per_gas"))
        .or_else(|| privacy.get("maxFeePerGas"))
        .or_else(|| privacy.get("max_priority_fee_per_gas"))
        .or_else(|| privacy.get("maxPriorityFeePerGas"))
        .and_then(value_to_u64)
        .or_else(|| {
            param_as_u64_any_with_tx(
                params,
                &[
                    "gas_price",
                    "gasPrice",
                    "max_fee_per_gas",
                    "maxFeePerGas",
                    "max_priority_fee_per_gas",
                    "maxPriorityFeePerGas",
                ],
            )
        })
        .unwrap_or(1);
    let view_key_raw = privacy
        .get("view_key")
        .or_else(|| privacy.get("stealth_view_key"))
        .and_then(value_to_string)
        .ok_or_else(|| anyhow::anyhow!("privacy.view_key (or stealth_view_key) is required"))?;
    let stealth_view_key = parse_hex32_from_string(&view_key_raw, "privacy.view_key")?;
    let spend_key_raw = privacy
        .get("spend_key")
        .or_else(|| privacy.get("stealth_spend_key"))
        .and_then(value_to_string)
        .ok_or_else(|| anyhow::anyhow!("privacy.spend_key (or stealth_spend_key) is required"))?;
    let stealth_spend_key = parse_hex32_from_string(&spend_key_raw, "privacy.spend_key")?;
    let signer_index = privacy
        .get("signer_index")
        .or_else(|| privacy.get("signerIndex"))
        .and_then(value_to_u64)
        .unwrap_or(0) as usize;
    let ring_members_value = privacy
        .get("ring_members")
        .or_else(|| privacy.get("ringMembers"))
        .or_else(|| privacy.get("members"))
        .ok_or_else(|| anyhow::anyhow!("privacy.ring_members (or ringMembers) is required"))?;
    let ring_members_array = ring_members_value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("privacy.ring_members must be an array"))?;
    if ring_members_array.is_empty() {
        bail!("privacy.ring_members must not be empty");
    }
    let mut ring_members = Vec::with_capacity(ring_members_array.len());
    for (idx, raw) in ring_members_array.iter().enumerate() {
        let text = value_to_string(raw)
            .ok_or_else(|| anyhow::anyhow!("privacy.ring_members[{idx}] must be hex string"))?;
        let parsed = parse_hex32_from_string(&text, &format!("privacy.ring_members[{idx}]"))?;
        ring_members.push(parsed);
    }
    if signer_index >= ring_members.len() {
        bail!(
            "privacy.signer_index out of range: {} >= {}",
            signer_index,
            ring_members.len()
        );
    }
    let private_key_raw = if let Some(raw) = privacy
        .get("private_key")
        .or_else(|| privacy.get("secret_key"))
        .and_then(value_to_string)
    {
        raw
    } else if let Some(env_name) = privacy
        .get("private_key_env")
        .or_else(|| privacy.get("secret_key_env"))
        .and_then(value_to_string)
    {
        string_env_nonempty(&env_name)
            .ok_or_else(|| anyhow::anyhow!("privacy private_key_env not set: {}", env_name))?
    } else {
        string_env_nonempty("NOVOVM_GATEWAY_WEB30_PRIVACY_SIGNER_SECRET_HEX").ok_or_else(|| {
            anyhow::anyhow!("privacy.private_key (or secret_key/private_key_env) is required")
        })?
    };
    let private_key = parse_hex32_from_string(&private_key_raw, "privacy.private_key")?;

    Ok(Some(GatewayWeb30PrivacyTxPlan {
        value,
        gas_limit,
        gas_price,
        stealth_view_key,
        stealth_spend_key,
        ring_members,
        signer_index,
        private_key,
    }))
}

pub(super) fn compute_gateway_web30_tx_hash(input: &GatewayWeb30TxHashInput<'_>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_gateway_web30_tx_hash_v1");
    hasher.update(input.uca_id.as_bytes());
    hasher.update(input.chain_id.to_le_bytes());
    hasher.update(input.nonce.to_le_bytes());
    hasher.update((input.from.len() as u64).to_le_bytes());
    hasher.update(input.from);
    hasher.update((input.payload.len() as u64).to_le_bytes());
    hasher.update(input.payload);
    hasher.update(input.signature_domain.as_bytes());
    hasher.update([if input.is_raw { 1 } else { 0 }]);
    hasher.update([if input.wants_cross_chain_atomic { 1 } else { 0 }]);
    let digest: [u8; 32] = hasher.finalize().into();
    digest
}

pub(super) fn compute_gateway_eth_tx_hash(input: &GatewayEthTxHashInput<'_>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_gateway_eth_tx_hash_v1");
    hasher.update(input.uca_id.as_bytes());
    hasher.update(input.chain_id.to_le_bytes());
    hasher.update(input.nonce.to_le_bytes());
    hasher.update([input.tx_type]);
    hasher.update([if input.tx_type4 { 1 } else { 0 }]);
    hasher.update((input.from.len() as u64).to_le_bytes());
    hasher.update(input.from);
    match input.to {
        Some(to) => {
            hasher.update([1]);
            hasher.update((to.len() as u64).to_le_bytes());
            hasher.update(to);
        }
        None => {
            hasher.update([0]);
        }
    }
    hasher.update(input.value.to_le_bytes());
    hasher.update(input.gas_limit.to_le_bytes());
    hasher.update(input.gas_price.to_le_bytes());
    hasher.update((input.data.len() as u64).to_le_bytes());
    hasher.update(input.data);
    hasher.update((input.signature.len() as u64).to_le_bytes());
    hasher.update(input.signature);
    hasher.update(input.access_list_address_count.to_le_bytes());
    hasher.update(input.access_list_storage_key_count.to_le_bytes());
    hasher.update(input.signature_domain.as_bytes());
    hasher.update([if input.wants_cross_chain_atomic { 1 } else { 0 }]);
    hasher.finalize().into()
}

pub(super) fn extract_eth_raw_tx_param(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &[
        "raw_tx",
        "rawTransaction",
        "raw_transaction",
        "raw",
        "signed_tx",
    ];
    if let Some(map) = params_object_with_any_keys(params, CANDIDATE_KEYS) {
        if let Some(raw) = pick_first_nonempty_string(map, CANDIDATE_KEYS) {
            return Some(raw);
        }
    }
    first_scalar_param_string(params)
}

pub(super) fn extract_web3_sha3_input_hex(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &["data", "input"];
    if let Some(map) = params_object_with_any_keys(params, CANDIDATE_KEYS) {
        if let Some(raw) = pick_first_nonempty_string(map, CANDIDATE_KEYS) {
            return Some(raw);
        }
    }
    first_scalar_param_string(params)
}

pub(super) fn extract_eth_block_hash_param(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &["block_hash", "blockHash", "hash"];
    if let Some(map) = params_object_with_any_keys(params, CANDIDATE_KEYS) {
        if let Some(raw) = pick_first_nonempty_string(map, CANDIDATE_KEYS) {
            return Some(raw);
        }
    }
    first_scalar_param_string(params)
}

pub(super) fn extract_eth_tx_hash_query_param(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &["tx_hash", "txHash", "transaction_hash", "hash"];
    if let Some(map) = params_object_with_any_keys(params, CANDIDATE_KEYS) {
        if let Some(raw) = pick_first_nonempty_string(map, CANDIDATE_KEYS) {
            return Some(raw);
        }
    }
    first_scalar_param_string(params)
}

pub(super) fn extract_eth_persona_address_param(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &["external_address", "from", "address"];
    if let Some(map) = params_object_with_any_keys(params, CANDIDATE_KEYS) {
        if let Some(found) = pick_first_nonempty_string(map, CANDIDATE_KEYS) {
            return Some(found);
        }
        if let Some(serde_json::Value::Object(tx_obj)) = map.get("tx") {
            if let Some(found) = pick_first_nonempty_string(tx_obj, CANDIDATE_KEYS) {
                return Some(found);
            }
        }
    }
    first_address_like_scalar_param_string(params)
}

pub(super) fn parse_eth_logs_address_filters(
    params: &serde_json::Value,
) -> Result<Option<Vec<Vec<u8>>>> {
    let Some(filter) =
        params_object_with_any_keys_or_nested_filter(params, &["address", "addresses"])
    else {
        return Ok(None);
    };
    let Some(raw) = filter.get("address").or_else(|| filter.get("addresses")) else {
        return Ok(None);
    };
    match raw {
        serde_json::Value::String(text) => Ok(Some(vec![decode_hex_bytes(text, "address")?])),
        serde_json::Value::Array(items) => {
            let mut out = Vec::new();
            for (idx, item) in items.iter().enumerate() {
                let text = value_to_string(item)
                    .ok_or_else(|| anyhow::anyhow!("address[{}] must be string", idx))?;
                let parsed = decode_hex_bytes(&text, &format!("address[{}]", idx))?;
                if !out.contains(&parsed) {
                    out.push(parsed);
                }
            }
            if out.is_empty() {
                Ok(None)
            } else {
                Ok(Some(out))
            }
        }
        _ => bail!("address filter must be string or string[]"),
    }
}

pub(super) fn parse_eth_logs_topic_filters(
    params: &serde_json::Value,
) -> Result<Option<GatewayEthTopicFilterSlots>> {
    let Some(filter) = params_object_with_any_keys_or_nested_filter(params, &["topics"]) else {
        return Ok(None);
    };
    let Some(raw_topics) = filter.get("topics") else {
        return Ok(None);
    };
    let Some(topics) = raw_topics.as_array() else {
        bail!("topics filter must be array");
    };
    if topics.is_empty() {
        return Ok(None);
    }

    let mut out = Vec::with_capacity(topics.len());
    for (topic_idx, topic_filter) in topics.iter().enumerate() {
        match topic_filter {
            serde_json::Value::Null => out.push(None),
            serde_json::Value::String(text) => {
                let parsed = parse_hex32_from_string(text, &format!("topics[{}]", topic_idx))?;
                out.push(Some(vec![parsed]));
            }
            serde_json::Value::Array(list) => {
                let mut candidates = Vec::new();
                for (candidate_idx, item) in list.iter().enumerate() {
                    let text = value_to_string(item).ok_or_else(|| {
                        anyhow::anyhow!("topics[{}][{}] must be string", topic_idx, candidate_idx)
                    })?;
                    let parsed = parse_hex32_from_string(
                        &text,
                        &format!("topics[{}][{}]", topic_idx, candidate_idx),
                    )?;
                    if !candidates.contains(&parsed) {
                        candidates.push(parsed);
                    }
                }
                out.push(Some(candidates));
            }
            _ => bail!("topics[{}] must be null, string, or string[]", topic_idx),
        }
    }
    Ok(Some(out))
}

pub(super) fn parse_eth_logs_query_from_params(
    params: &serde_json::Value,
    latest: u64,
) -> Result<GatewayEthLogsQuery> {
    let filter_obj = params_object_with_any_keys_or_nested_filter(
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
    let address_filters = parse_eth_logs_address_filters(params)?;
    let topic_filters = parse_eth_logs_topic_filters(params)?;
    let block_hash = filter_obj
        .and_then(|map| map.get("block_hash").or_else(|| map.get("blockHash")))
        .and_then(value_to_string)
        .map(|raw| parse_hex32_from_string(&raw, "blockHash"))
        .transpose()?;
    let (from_block, to_block, include_pending_block) = if block_hash.is_some() {
        let from_block_conflict = filter_obj
            .and_then(|map| map.get("fromBlock").or_else(|| map.get("from_block")))
            .is_some_and(|value| !value.is_null());
        let to_block_conflict = filter_obj
            .and_then(|map| map.get("toBlock").or_else(|| map.get("to_block")))
            .is_some_and(|value| !value.is_null());
        if from_block_conflict || to_block_conflict {
            bail!("blockHash is mutually exclusive with fromBlock/toBlock");
        }
        (None, None, false)
    } else {
        let from_tag = filter_obj
            .and_then(|map| map.get("fromBlock").or_else(|| map.get("from_block")))
            .and_then(value_to_string)
            .unwrap_or_else(|| "earliest".to_string());
        let to_tag = filter_obj
            .and_then(|map| map.get("toBlock").or_else(|| map.get("to_block")))
            .and_then(value_to_string)
            .unwrap_or_else(|| "latest".to_string());
        let (from_block, from_pending) = parse_eth_logs_block_number_from_tag(&from_tag, latest)?;
        let (to_block, to_pending) = parse_eth_logs_block_number_from_tag(&to_tag, latest)?;
        (from_block, to_block, from_pending || to_pending)
    };
    Ok(GatewayEthLogsQuery {
        address_filters,
        topic_filters,
        block_hash,
        from_block,
        to_block,
        include_pending_block,
    })
}

pub(super) fn parse_eth_logs_block_number_from_tag(
    tag: &str,
    latest: u64,
) -> Result<(Option<u64>, bool)> {
    let normalized = tag.trim().trim_matches('"');
    if normalized.eq_ignore_ascii_case("pending") {
        return Ok((Some(latest.saturating_add(1)), true));
    }
    Ok((parse_eth_block_number_from_tag(tag, latest)?, false))
}

pub(super) fn collect_gateway_eth_logs_with_query(
    chain_id: u64,
    entries: Vec<GatewayEthTxIndexEntry>,
    query: &GatewayEthLogsQuery,
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) -> Result<Vec<serde_json::Value>> {
    let latest = resolve_gateway_eth_latest_block_number(chain_id, &entries, eth_tx_index_store)?;
    let blocks = gateway_eth_group_entries_by_block(entries);
    let mut selected_blocks: BTreeMap<u64, (Vec<GatewayEthTxIndexEntry>, [u8; 32])> =
        BTreeMap::new();
    if let Some(block_hash) = query.block_hash {
        let mut matched_confirmed = false;
        for (block_number, block_txs) in blocks {
            let candidate = gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
            if candidate == block_hash {
                selected_blocks.insert(block_number, (block_txs, candidate));
                matched_confirmed = true;
                break;
            }
        }
        if !matched_confirmed {
            if let Some((pending_block_number, pending_block_hash, pending_block_txs)) =
                gateway_eth_pending_block_from_runtime(chain_id, latest, false)
            {
                if pending_block_hash == block_hash {
                    selected_blocks.insert(
                        pending_block_number,
                        (pending_block_txs, pending_block_hash),
                    );
                }
            }
        }
        if !matched_confirmed {
            if let Some((block_number, block_txs)) =
                collect_gateway_eth_block_entries_by_hash_precise(
                    eth_tx_index,
                    eth_tx_index_store,
                    chain_id,
                    &block_hash,
                    gateway_eth_query_scan_max(),
                )?
            {
                selected_blocks.insert(block_number, (block_txs, block_hash));
            }
        }
    } else {
        let from_block = query.from_block.unwrap_or(0);
        let to_block = query.to_block.unwrap_or(latest);
        let (start, end) = if from_block <= to_block {
            (from_block, to_block)
        } else {
            (to_block, from_block)
        };
        for (block_number, block_txs) in blocks {
            if block_number < start || block_number > end {
                continue;
            }
            let block_hash = gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
            selected_blocks.insert(block_number, (block_txs, block_hash));
        }
        // If range is explicit and reasonably bounded, recover missing blocks from precise store index.
        let span = end.saturating_sub(start).saturating_add(1);
        let recover_limit = gateway_eth_query_scan_max() as u64;
        if span > 0 && span <= recover_limit {
            for block_number in start..=end {
                if selected_blocks.contains_key(&block_number) {
                    continue;
                }
                let precise_block_txs = collect_gateway_eth_block_entries_precise(
                    eth_tx_index,
                    eth_tx_index_store,
                    chain_id,
                    block_number,
                    gateway_eth_query_scan_max(),
                )?;
                if precise_block_txs.is_empty() {
                    continue;
                }
                let block_hash =
                    gateway_eth_block_hash_for_txs(chain_id, block_number, &precise_block_txs);
                selected_blocks.insert(block_number, (precise_block_txs, block_hash));
            }
        }
        if query.include_pending_block {
            if let Some((pending_block_number, pending_block_hash, pending_block_txs)) =
                gateway_eth_pending_block_from_runtime(chain_id, latest, false)
            {
                if pending_block_number >= start && pending_block_number <= end {
                    selected_blocks.insert(
                        pending_block_number,
                        (pending_block_txs, pending_block_hash),
                    );
                }
            }
        }
    }

    let mut logs = Vec::new();
    let mut log_index = 0u64;
    for (block_number, (block_txs, block_hash)) in selected_blocks {
        for (tx_index, entry) in block_txs.iter().enumerate() {
            let log_topics = vec![entry.tx_hash];
            let log_address = entry.to.as_ref().unwrap_or(&entry.from);
            if let Some(filters) = query.address_filters.as_ref() {
                if !filters.iter().any(|candidate| candidate == log_address) {
                    continue;
                }
            }
            if let Some(filters) = query.topic_filters.as_ref() {
                if !gateway_eth_log_matches_topics(&log_topics, filters) {
                    continue;
                }
            }
            logs.push(serde_json::json!({
                "removed": false,
                "logIndex": format!("0x{:x}", log_index),
                "transactionIndex": format!("0x{:x}", tx_index),
                "transactionHash": format!("0x{}", to_hex(&entry.tx_hash)),
                "blockHash": format!("0x{}", to_hex(&block_hash)),
                "blockNumber": format!("0x{:x}", block_number),
                "address": format!("0x{}", to_hex(log_address)),
                "data": format!("0x{}", to_hex(&entry.input)),
                "topics": log_topics
                    .iter()
                    .map(|topic| format!("0x{}", to_hex(topic)))
                    .collect::<Vec<String>>(),
            }));
            log_index = log_index.saturating_add(1);
        }
    }
    Ok(logs)
}

pub(super) fn gateway_eth_log_matches_topics(
    log_topics: &[[u8; 32]],
    topic_filters: &GatewayEthTopicFilterSlots,
) -> bool {
    for (topic_index, slot_filter) in topic_filters.iter().enumerate() {
        let Some(accepted_topics) = slot_filter else {
            continue;
        };
        if accepted_topics.is_empty() {
            return false;
        }
        let Some(current_topic) = log_topics.get(topic_index) else {
            return false;
        };
        if !accepted_topics.contains(current_topic) {
            return false;
        }
    }
    true
}

pub(super) fn parse_eth_filter_id(params: &serde_json::Value) -> Option<u64> {
    if let Some(map) = params_primary_object(params) {
        let from_object = map
            .get("filter_id")
            .or_else(|| map.get("filterId"))
            .or_else(|| map.get("subscription"))
            .or_else(|| map.get("subscription_id"))
            .or_else(|| map.get("subscriptionId"))
            .or_else(|| map.get("sub_id"))
            .or_else(|| map.get("subId"))
            .or_else(|| map.get("id"))
            .and_then(value_to_string)
            .and_then(|raw| parse_u64_decimal_or_hex(&raw));
        if from_object.is_some() {
            return from_object;
        }
    }
    params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(value_to_string)
        .and_then(|raw| parse_u64_decimal_or_hex(&raw))
}

pub(super) fn gateway_eth_tx_type_number_from_ir(tx_type: TxType) -> u8 {
    match tx_type {
        TxType::Transfer => 0,
        TxType::ContractCall => 1,
        TxType::ContractDeploy => 2,
        TxType::Privacy => 3,
        TxType::CrossShard => 4,
        TxType::CrossChainTransfer => 5,
        TxType::CrossChainCall => 6,
    }
}

pub(super) fn extract_eth_storage_slot_param(params: &serde_json::Value) -> Option<String> {
    const CANDIDATE_KEYS: &[&str] = &["position", "slot", "index", "storage_slot", "storageSlot"];
    match params {
        serde_json::Value::Object(map) => pick_first_nonempty_string(map, CANDIDATE_KEYS),
        serde_json::Value::Array(_) => match non_object_param_at(params, 1) {
            Some(value) => value_to_string(value).and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            None => None,
        },
        _ => None,
    }
}

pub(super) fn parse_eth_get_proof_storage_keys(params: &serde_json::Value) -> Result<Vec<String>> {
    let raw = if let Some(map) =
        params_object_with_any_keys(params, &["storage_keys", "storageKeys", "keys", "slots"])
    {
        map.get("storage_keys")
            .or_else(|| map.get("storageKeys"))
            .or_else(|| map.get("keys"))
            .or_else(|| map.get("slots"))
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()))
    } else if let Some(arr) = params.as_array() {
        arr.iter()
            .find(|v| !v.is_object() && v.is_array())
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()))
    } else {
        serde_json::Value::Array(Vec::new())
    };
    let Some(items) = raw.as_array() else {
        bail!("storage_keys (or storageKeys) must be string[]");
    };
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let text = value_to_string(item)
            .ok_or_else(|| anyhow::anyhow!("storage_keys[{}] must be string", idx))?;
        let normalized = text.trim().to_string();
        if normalized.is_empty() {
            bail!("storage_keys[{}] must be non-empty", idx);
        }
        out.push(normalized);
    }
    Ok(out)
}

pub(super) fn parse_eth_get_proof_block_tag(params: &serde_json::Value) -> Option<String> {
    if let Some(map) = params_object_with_any_keys(
        params,
        &[
            "block",
            "tag",
            "block_tag",
            "blockTag",
            "block_number",
            "blockNumber",
            "default_block",
            "defaultBlock",
        ],
    ) {
        let from_object = [
            "block",
            "tag",
            "block_tag",
            "blockTag",
            "block_number",
            "blockNumber",
            "default_block",
            "defaultBlock",
        ]
        .iter()
        .find_map(|key| map.get(*key))
        .and_then(value_to_string);
        if from_object.is_some() {
            return from_object;
        }
    }
    if let Some(tag_like) = last_block_tag_like_param_string(params) {
        return Some(tag_like);
    }
    non_object_param_at(params, 2).and_then(value_to_string)
}

pub(super) fn resolve_gateway_eth_get_proof_entries(
    chain_id: u64,
    entries: Vec<GatewayEthTxIndexEntry>,
    block_tag: &str,
    latest: u64,
) -> Result<Option<Vec<GatewayEthTxIndexEntry>>> {
    let normalized = block_tag.trim().trim_matches('"');
    if normalized.eq_ignore_ascii_case("pending") {
        let mut out = entries.clone();
        if let Some((_pending_block_number, _pending_block_hash, pending_entries)) =
            gateway_eth_pending_block_from_runtime(chain_id, latest, false)
        {
            out.extend(pending_entries);
        }
        return Ok(Some(out));
    }
    if normalized.eq_ignore_ascii_case("earliest") {
        return Ok(Some(Vec::new()));
    }
    if normalized.is_empty()
        || normalized.eq_ignore_ascii_case("latest")
        || normalized.eq_ignore_ascii_case("safe")
        || normalized.eq_ignore_ascii_case("finalized")
    {
        return Ok(Some(entries));
    }
    let Some(block_number) = parse_u64_decimal_or_hex(normalized) else {
        bail!("invalid block number/tag: {}", block_tag);
    };
    if block_number > latest {
        return Ok(None);
    }
    let oldest_block = entries
        .iter()
        .map(|entry| entry.nonce)
        .min()
        .unwrap_or(latest);
    let likely_scan_truncated =
        !entries.is_empty() && entries.len() >= gateway_eth_query_scan_max() && oldest_block > 0;
    if likely_scan_truncated && block_number < oldest_block {
        return Ok(None);
    }
    Ok(Some(
        entries
            .into_iter()
            .filter(|entry| entry.nonce <= block_number)
            .collect(),
    ))
}

pub(super) fn parse_u128_hex_or_dec(raw: &str) -> Option<u128> {
    let trimmed = raw.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if hex.is_empty() {
            Some(0)
        } else {
            u128::from_str_radix(hex, 16).ok()
        }
    } else {
        trimmed.parse::<u128>().ok()
    }
}

pub(super) fn decode_hex_bytes(raw: &str, field: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim();
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if normalized.is_empty() {
        bail!("{} is empty", field);
    }
    if !normalized.len().is_multiple_of(2) {
        bail!("{} must have even hex length", field);
    }
    if !normalized.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("{} must be hex", field);
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    let bytes = normalized.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let pair = std::str::from_utf8(&bytes[idx..idx + 2])
            .with_context(|| format!("{} contains invalid utf8", field))?;
        let v = u8::from_str_radix(pair, 16)
            .with_context(|| format!("{} contains invalid hex byte {}", field, pair))?;
        out.push(v);
        idx += 2;
    }
    Ok(out)
}

pub(super) fn to_hex(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len() * 2);
    for b in raw {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

#[cfg(test)]
pub(super) fn gateway_env_mutex() -> &'static std::sync::Mutex<()> {
    static ENV_LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

pub(super) fn bool_env(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .and_then(|s| {
            let v = s.trim().to_ascii_lowercase();
            match v.as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            }
        })
        .unwrap_or(default)
}

pub(super) fn gateway_warn_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| bool_env("NOVOVM_GATEWAY_WARN_LOG", false))
}

pub(super) fn gateway_summary_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| bool_env("NOVOVM_GATEWAY_SUMMARY_LOG", false))
}

pub(super) fn string_env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(super) fn string_env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

pub(super) fn u64_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

pub(super) fn u32_env_allow_zero(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(default)
}

pub(super) fn now_unix_sec() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

pub(super) fn now_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
}
