#![forbid(unsafe_code)]

use anyhow::bail;
use novovm_adapter_api::{BlockIR, ChainType, TxIR, TxType};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvmChainProfileKind {
    EthereumMainnet,
    BnbMainnet,
    PolygonMainnet,
    AvalancheCChainMainnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxType4Policy {
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobPolicy {
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvmRawTxEnvelopeType {
    Legacy,
    Type1AccessList,
    Type2DynamicFee,
    Type3Blob,
    Type4SetCode,
}

impl EvmRawTxEnvelopeType {
    #[must_use]
    pub fn tx_type_number(self) -> u8 {
        match self {
            EvmRawTxEnvelopeType::Legacy => 0,
            EvmRawTxEnvelopeType::Type1AccessList => 1,
            EvmRawTxEnvelopeType::Type2DynamicFee => 2,
            EvmRawTxEnvelopeType::Type3Blob => 3,
            EvmRawTxEnvelopeType::Type4SetCode => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvmRawTxRouteHint {
    pub envelope: EvmRawTxEnvelopeType,
    pub tx_type_number: u8,
    pub tx_type4: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvmRawTxFieldsM0 {
    pub hint: EvmRawTxRouteHint,
    pub chain_id: Option<u64>,
    pub nonce: Option<u64>,
    pub gas_limit: Option<u64>,
    pub gas_price: Option<u64>,
    pub max_priority_fee_per_gas: Option<u64>,
    pub max_fee_per_blob_gas: Option<u64>,
    pub blob_hash_count: Option<u64>,
    pub access_list_address_count: Option<u64>,
    pub access_list_storage_key_count: Option<u64>,
    pub to: Option<Vec<u8>>,
    pub value: Option<u128>,
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvmRawBlockTxM0 {
    pub from: Vec<u8>,
    pub raw_tx: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvmRawBlockM0 {
    pub hash: Vec<u8>,
    pub parent_hash: Vec<u8>,
    pub number: u64,
    pub timestamp: u64,
    pub transactions: Vec<EvmRawBlockTxM0>,
    pub state_root: Vec<u8>,
    pub transactions_root: Vec<u8>,
    pub receipts_root: Vec<u8>,
    pub miner: Vec<u8>,
    pub difficulty: u64,
    pub gas_used: u64,
    pub gas_limit: u64,
}

#[derive(Debug, Clone, Copy)]
enum RlpItem<'a> {
    Bytes(&'a [u8]),
    List(&'a [u8]),
}

#[derive(Debug, Clone)]
pub struct EvmChainProfile {
    pub kind: EvmChainProfileKind,
    pub chain_type: ChainType,
    pub chain_id: u64,
    pub tx_type4_policy: TxType4Policy,
    pub blob_policy: BlobPolicy,
}

const ETH_MAINNET_PRECOMPILES_M0: &[&str] = &[
    "ecrecover",
    "sha256",
    "ripemd160",
    "identity",
    "modexp",
    "bn256_add",
    "bn256_scalar_mul",
    "bn256_pairing",
    "blake2f",
];

const BNB_MAINNET_PRECOMPILES_M0: &[&str] = &[
    "ecrecover",
    "sha256",
    "ripemd160",
    "identity",
    "modexp",
    "bn256_add",
    "bn256_scalar_mul",
    "bn256_pairing",
    "blake2f",
];

const POLYGON_MAINNET_PRECOMPILES_M0: &[&str] = &[
    "ecrecover",
    "sha256",
    "ripemd160",
    "identity",
    "modexp",
    "bn256_add",
    "bn256_scalar_mul",
    "bn256_pairing",
    "blake2f",
];

const AVALANCHE_CCHAIN_MAINNET_PRECOMPILES_M0: &[&str] = &[
    "ecrecover",
    "sha256",
    "ripemd160",
    "identity",
    "modexp",
    "bn256_add",
    "bn256_scalar_mul",
    "bn256_pairing",
    "blake2f",
];

static EVM_CHAIN_TYPE_OVERRIDES: OnceLock<HashMap<u64, ChainType>> = OnceLock::new();

fn parse_evm_chain_type_overrides(raw: &str) -> HashMap<u64, ChainType> {
    let mut out = HashMap::<u64, ChainType>::new();
    for entry in raw.split([',', ';']) {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (chain_id_raw, chain_type_raw) = if let Some((left, right)) = trimmed.split_once('=') {
            (left.trim(), right.trim())
        } else if let Some((left, right)) = trimmed.split_once(':') {
            (left.trim(), right.trim())
        } else {
            continue;
        };
        let Ok(chain_id) = chain_id_raw.parse::<u64>() else {
            continue;
        };
        let Ok(chain_type) = ChainType::parse(chain_type_raw) else {
            continue;
        };
        if supports_evm_family(chain_type) {
            out.insert(chain_id, chain_type);
        }
    }
    out
}

fn evm_chain_type_overrides() -> &'static HashMap<u64, ChainType> {
    EVM_CHAIN_TYPE_OVERRIDES.get_or_init(|| {
        std::env::var("NOVOVM_EVM_CHAIN_TYPE_OVERRIDES")
            .ok()
            .map(|raw| parse_evm_chain_type_overrides(&raw))
            .unwrap_or_default()
    })
}

fn parse_bool_env(raw: &str) -> Option<bool> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn evm_bool_env(base_key: &str, default: bool) -> bool {
    std::env::var(base_key)
        .ok()
        .and_then(|raw| parse_bool_env(&raw))
        .unwrap_or(default)
}

fn evm_chain_bool_env(chain_id: u64, base_key: &str, default: bool) -> bool {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    std::env::var(&chain_key_dec)
        .ok()
        .or_else(|| std::env::var(&chain_key_hex).ok())
        .and_then(|raw| parse_bool_env(&raw))
        .unwrap_or_else(|| evm_bool_env(base_key, default))
}

fn evm_type1_write_enabled_for_chain(chain_id: u64) -> bool {
    evm_chain_bool_env(chain_id, "NOVOVM_EVM_ENABLE_TYPE1_WRITE", true)
}

fn evm_type2_write_enabled_for_chain(chain_id: u64) -> bool {
    evm_chain_bool_env(chain_id, "NOVOVM_EVM_ENABLE_TYPE2_WRITE", true)
}

fn evm_type3_write_enabled_for_chain(chain_id: u64) -> bool {
    evm_chain_bool_env(chain_id, "NOVOVM_EVM_ENABLE_TYPE3_WRITE", false)
}

#[must_use]
pub fn resolve_evm_chain_type_from_chain_id(chain_id: u64) -> ChainType {
    if let Some(chain_type) = evm_chain_type_overrides().get(&chain_id).copied() {
        return chain_type;
    }
    match chain_id {
        56 => ChainType::BNB,
        137 => ChainType::Polygon,
        43114 => ChainType::Avalanche,
        _ => ChainType::EVM,
    }
}

#[must_use]
pub fn supports_evm_family(chain_type: ChainType) -> bool {
    matches!(
        chain_type,
        ChainType::EVM | ChainType::BNB | ChainType::Polygon | ChainType::Avalanche
    )
}

pub fn resolve_evm_profile(
    chain_type: ChainType,
    chain_id: u64,
) -> anyhow::Result<EvmChainProfile> {
    match chain_type {
        ChainType::EVM => Ok(EvmChainProfile {
            kind: EvmChainProfileKind::EthereumMainnet,
            chain_type,
            chain_id,
            tx_type4_policy: TxType4Policy::Reject,
            blob_policy: BlobPolicy::ReadOnly,
        }),
        ChainType::BNB => Ok(EvmChainProfile {
            kind: EvmChainProfileKind::BnbMainnet,
            chain_type,
            chain_id,
            tx_type4_policy: TxType4Policy::Reject,
            blob_policy: BlobPolicy::ReadOnly,
        }),
        ChainType::Polygon => Ok(EvmChainProfile {
            kind: EvmChainProfileKind::PolygonMainnet,
            chain_type,
            chain_id,
            tx_type4_policy: TxType4Policy::Reject,
            blob_policy: BlobPolicy::ReadOnly,
        }),
        ChainType::Avalanche => Ok(EvmChainProfile {
            kind: EvmChainProfileKind::AvalancheCChainMainnet,
            chain_type,
            chain_id,
            tx_type4_policy: TxType4Policy::Reject,
            blob_policy: BlobPolicy::ReadOnly,
        }),
        _ => bail!("unsupported EVM family chain_type={}", chain_type.as_str()),
    }
}

pub fn classify_raw_evm_tx_envelope(raw: &[u8]) -> anyhow::Result<EvmRawTxEnvelopeType> {
    if raw.is_empty() {
        bail!("raw tx is empty");
    }
    let first = raw[0];
    if first >= 0xc0 {
        return Ok(EvmRawTxEnvelopeType::Legacy);
    }
    Ok(match first {
        0x01 => EvmRawTxEnvelopeType::Type1AccessList,
        0x02 => EvmRawTxEnvelopeType::Type2DynamicFee,
        0x03 => EvmRawTxEnvelopeType::Type3Blob,
        0x04 => EvmRawTxEnvelopeType::Type4SetCode,
        0x00..=0x7f => {
            bail!("unsupported typed tx envelope: type={}", first);
        }
        _ => {
            bail!("invalid tx envelope prefix: 0x{:02x}", first);
        }
    })
}

pub fn resolve_raw_evm_tx_route_hint_m0(raw: &[u8]) -> anyhow::Result<EvmRawTxRouteHint> {
    let envelope = classify_raw_evm_tx_envelope(raw)?;
    match envelope {
        EvmRawTxEnvelopeType::Legacy
        | EvmRawTxEnvelopeType::Type1AccessList
        | EvmRawTxEnvelopeType::Type2DynamicFee => Ok(EvmRawTxRouteHint {
            envelope,
            tx_type_number: envelope.tx_type_number(),
            tx_type4: false,
        }),
        EvmRawTxEnvelopeType::Type3Blob => Ok(EvmRawTxRouteHint {
            envelope,
            tx_type_number: envelope.tx_type_number(),
            tx_type4: false,
        }),
        EvmRawTxEnvelopeType::Type4SetCode => Ok(EvmRawTxRouteHint {
            envelope,
            tx_type_number: envelope.tx_type_number(),
            tx_type4: true,
        }),
    }
}

fn parse_usize_be(raw: &[u8], field: &str) -> anyhow::Result<usize> {
    if raw.is_empty() {
        bail!("{} is empty", field);
    }
    if raw.len() > std::mem::size_of::<usize>() {
        bail!("{} overflows usize", field);
    }
    if raw.len() > 1 && raw[0] == 0 {
        bail!("{} has non-canonical leading zero", field);
    }
    let mut out = 0usize;
    for b in raw {
        out = (out << 8) | (*b as usize);
    }
    Ok(out)
}

fn parse_rlp_item(input: &[u8]) -> anyhow::Result<(RlpItem<'_>, usize)> {
    if input.is_empty() {
        bail!("rlp input is empty");
    }
    let b0 = input[0];
    match b0 {
        0x00..=0x7f => Ok((RlpItem::Bytes(&input[..1]), 1)),
        0x80..=0xb7 => {
            let len = (b0 - 0x80) as usize;
            if input.len() < 1 + len {
                bail!("rlp bytes short input");
            }
            Ok((RlpItem::Bytes(&input[1..1 + len]), 1 + len))
        }
        0xb8..=0xbf => {
            let len_of_len = (b0 - 0xb7) as usize;
            if input.len() < 1 + len_of_len {
                bail!("rlp bytes length-of-length short input");
            }
            let len = parse_usize_be(&input[1..1 + len_of_len], "rlp bytes length")?;
            if input.len() < 1 + len_of_len + len {
                bail!("rlp bytes payload short input");
            }
            Ok((
                RlpItem::Bytes(&input[1 + len_of_len..1 + len_of_len + len]),
                1 + len_of_len + len,
            ))
        }
        0xc0..=0xf7 => {
            let len = (b0 - 0xc0) as usize;
            if input.len() < 1 + len {
                bail!("rlp list short input");
            }
            Ok((RlpItem::List(&input[1..1 + len]), 1 + len))
        }
        0xf8..=0xff => {
            let len_of_len = (b0 - 0xf7) as usize;
            if input.len() < 1 + len_of_len {
                bail!("rlp list length-of-length short input");
            }
            let len = parse_usize_be(&input[1..1 + len_of_len], "rlp list length")?;
            if input.len() < 1 + len_of_len + len {
                bail!("rlp list payload short input");
            }
            Ok((
                RlpItem::List(&input[1 + len_of_len..1 + len_of_len + len]),
                1 + len_of_len + len,
            ))
        }
    }
}

fn parse_rlp_list_payload_items<'a>(payload: &'a [u8]) -> anyhow::Result<Vec<RlpItem<'a>>> {
    let mut items = Vec::new();
    let mut offset = 0usize;
    while offset < payload.len() {
        let (item, used) = parse_rlp_item(&payload[offset..])?;
        items.push(item);
        offset += used;
    }
    if offset != payload.len() {
        bail!("rlp list payload decode did not consume all bytes");
    }
    Ok(items)
}

fn parse_top_level_rlp_list(raw: &[u8]) -> anyhow::Result<Vec<RlpItem<'_>>> {
    let (top, used) = parse_rlp_item(raw)?;
    if used != raw.len() {
        bail!("rlp top-level item has trailing bytes");
    }
    match top {
        RlpItem::List(payload) => parse_rlp_list_payload_items(payload),
        RlpItem::Bytes(_) => bail!("rlp top-level is not a list"),
    }
}

fn rlp_item_as_bytes<'a>(item: &'a RlpItem<'a>, field: &str) -> anyhow::Result<&'a [u8]> {
    match item {
        RlpItem::Bytes(v) => Ok(*v),
        RlpItem::List(_) => bail!("{} must be bytes, got list", field),
    }
}

fn rlp_item_as_u64(item: &RlpItem<'_>, field: &str) -> anyhow::Result<u64> {
    let raw = rlp_item_as_bytes(item, field)?;
    if raw.is_empty() {
        return Ok(0);
    }
    if raw.len() > 8 {
        bail!("{} overflows u64", field);
    }
    if raw.len() > 1 && raw[0] == 0 {
        bail!("{} has non-canonical leading zero", field);
    }
    let mut out = 0u64;
    for b in raw {
        out = (out << 8) | (*b as u64);
    }
    Ok(out)
}

fn rlp_item_as_u128(item: &RlpItem<'_>, field: &str) -> anyhow::Result<u128> {
    let raw = rlp_item_as_bytes(item, field)?;
    if raw.is_empty() {
        return Ok(0);
    }
    if raw.len() > 16 {
        bail!("{} overflows u128", field);
    }
    if raw.len() > 1 && raw[0] == 0 {
        bail!("{} has non-canonical leading zero", field);
    }
    let mut out = 0u128;
    for b in raw {
        out = (out << 8) | (*b as u128);
    }
    Ok(out)
}

fn rlp_item_as_address(item: &RlpItem<'_>, field: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let raw = rlp_item_as_bytes(item, field)?;
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.len() != 20 {
        bail!("{} must be 20 bytes or empty, got {}", field, raw.len());
    }
    Ok(Some(raw.to_vec()))
}

fn rlp_item_as_list_items<'a>(
    item: &'a RlpItem<'a>,
    field: &str,
) -> anyhow::Result<Vec<RlpItem<'a>>> {
    match item {
        RlpItem::List(payload) => parse_rlp_list_payload_items(payload),
        RlpItem::Bytes(_) => bail!("{} must be list", field),
    }
}

fn rlp_access_list_intrinsic_counts(item: &RlpItem<'_>, field: &str) -> anyhow::Result<(u64, u64)> {
    let entries = rlp_item_as_list_items(item, field)?;
    let mut address_count = 0u64;
    let mut storage_key_count = 0u64;
    for (entry_idx, entry) in entries.iter().enumerate() {
        let entry_field = format!("{}[{}]", field, entry_idx);
        let entry_items = rlp_item_as_list_items(entry, &entry_field)?;
        if entry_items.len() < 2 {
            bail!("{} must be [address,storageKeys]", entry_field);
        }
        let address_field = format!("{}.address", entry_field);
        let address_raw = rlp_item_as_bytes(&entry_items[0], &address_field)?;
        if address_raw.len() != 20 {
            bail!(
                "{} must be 20 bytes, got {}",
                address_field,
                address_raw.len()
            );
        }
        address_count = address_count.saturating_add(1);

        let storage_keys_field = format!("{}.storageKeys", entry_field);
        let storage_keys = rlp_item_as_list_items(&entry_items[1], &storage_keys_field)?;
        for (key_idx, storage_key) in storage_keys.iter().enumerate() {
            let key_field = format!("{}[{}]", storage_keys_field, key_idx);
            let key_raw = rlp_item_as_bytes(storage_key, &key_field)?;
            if key_raw.len() != 32 {
                bail!("{} must be 32 bytes, got {}", key_field, key_raw.len());
            }
            storage_key_count = storage_key_count.saturating_add(1);
        }
    }
    Ok((address_count, storage_key_count))
}

fn tx_fields_from_legacy_list(
    items: &[RlpItem<'_>],
    hint: EvmRawTxRouteHint,
) -> anyhow::Result<EvmRawTxFieldsM0> {
    if items.len() < 6 {
        bail!("legacy tx rlp list too short: expected >=6 fields");
    }
    let nonce = rlp_item_as_u64(&items[0], "legacy.nonce")?;
    let gas_price = rlp_item_as_u64(&items[1], "legacy.gas_price")?;
    let gas_limit = rlp_item_as_u64(&items[2], "legacy.gas_limit")?;
    let to = rlp_item_as_address(&items[3], "legacy.to")?;
    let value = rlp_item_as_u128(&items[4], "legacy.value")?;
    let data = rlp_item_as_bytes(&items[5], "legacy.data")?.to_vec();

    let chain_id = if items.len() > 6 {
        let v = rlp_item_as_u128(&items[6], "legacy.v")?;
        if v >= 35 {
            let cid = ((v - 35) / 2) as u64;
            Some(cid)
        } else {
            None
        }
    } else {
        None
    };

    Ok(EvmRawTxFieldsM0 {
        hint,
        chain_id,
        nonce: Some(nonce),
        gas_limit: Some(gas_limit),
        gas_price: Some(gas_price),
        max_priority_fee_per_gas: None,
        max_fee_per_blob_gas: None,
        blob_hash_count: None,
        access_list_address_count: None,
        access_list_storage_key_count: None,
        to,
        value: Some(value),
        data: Some(data),
    })
}

fn tx_fields_from_type1_list(
    items: &[RlpItem<'_>],
    hint: EvmRawTxRouteHint,
) -> anyhow::Result<EvmRawTxFieldsM0> {
    if items.len() < 8 {
        bail!("type1 tx rlp list too short: expected >=8 fields");
    }
    let chain_id = rlp_item_as_u64(&items[0], "type1.chain_id")?;
    let nonce = rlp_item_as_u64(&items[1], "type1.nonce")?;
    let gas_price = rlp_item_as_u64(&items[2], "type1.gas_price")?;
    let gas_limit = rlp_item_as_u64(&items[3], "type1.gas_limit")?;
    let to = rlp_item_as_address(&items[4], "type1.to")?;
    let value = rlp_item_as_u128(&items[5], "type1.value")?;
    let data = rlp_item_as_bytes(&items[6], "type1.data")?.to_vec();
    let (access_list_address_count, access_list_storage_key_count) =
        rlp_access_list_intrinsic_counts(&items[7], "type1.access_list")?;
    Ok(EvmRawTxFieldsM0 {
        hint,
        chain_id: Some(chain_id),
        nonce: Some(nonce),
        gas_limit: Some(gas_limit),
        gas_price: Some(gas_price),
        max_priority_fee_per_gas: None,
        max_fee_per_blob_gas: None,
        blob_hash_count: None,
        access_list_address_count: Some(access_list_address_count),
        access_list_storage_key_count: Some(access_list_storage_key_count),
        to,
        value: Some(value),
        data: Some(data),
    })
}

fn tx_fields_from_type2_list(
    items: &[RlpItem<'_>],
    hint: EvmRawTxRouteHint,
) -> anyhow::Result<EvmRawTxFieldsM0> {
    if items.len() < 9 {
        bail!("type2 tx rlp list too short: expected >=9 fields");
    }
    let chain_id = rlp_item_as_u64(&items[0], "type2.chain_id")?;
    let nonce = rlp_item_as_u64(&items[1], "type2.nonce")?;
    let max_priority_fee_per_gas = rlp_item_as_u64(&items[2], "type2.max_priority_fee_per_gas")?;
    let max_fee_per_gas = rlp_item_as_u64(&items[3], "type2.max_fee_per_gas")?;
    let gas_limit = rlp_item_as_u64(&items[4], "type2.gas_limit")?;
    let to = rlp_item_as_address(&items[5], "type2.to")?;
    let value = rlp_item_as_u128(&items[6], "type2.value")?;
    let data = rlp_item_as_bytes(&items[7], "type2.data")?.to_vec();
    let (access_list_address_count, access_list_storage_key_count) =
        rlp_access_list_intrinsic_counts(&items[8], "type2.access_list")?;
    Ok(EvmRawTxFieldsM0 {
        hint,
        chain_id: Some(chain_id),
        nonce: Some(nonce),
        gas_limit: Some(gas_limit),
        gas_price: Some(max_fee_per_gas),
        max_priority_fee_per_gas: Some(max_priority_fee_per_gas),
        max_fee_per_blob_gas: None,
        blob_hash_count: None,
        access_list_address_count: Some(access_list_address_count),
        access_list_storage_key_count: Some(access_list_storage_key_count),
        to,
        value: Some(value),
        data: Some(data),
    })
}

fn rlp_blob_hash_count(item: &RlpItem<'_>, field: &str) -> anyhow::Result<u64> {
    let RlpItem::List(payload) = item else {
        bail!("{field} must be rlp list");
    };
    let mut cursor = 0usize;
    let mut count = 0u64;
    while cursor < payload.len() {
        let (entry, consumed) = parse_rlp_item(&payload[cursor..])?;
        let raw = rlp_item_as_bytes(&entry, field)?;
        if raw.len() != 32 {
            bail!("{field} entry must be 32 bytes");
        }
        cursor = cursor.saturating_add(consumed);
        count = count.saturating_add(1);
    }
    if cursor != payload.len() {
        bail!("{field} malformed list payload");
    }
    Ok(count)
}

fn tx_fields_from_type3_list(
    items: &[RlpItem<'_>],
    hint: EvmRawTxRouteHint,
) -> anyhow::Result<EvmRawTxFieldsM0> {
    if items.len() < 11 {
        bail!("type3 tx rlp list too short: expected >=11 fields");
    }
    let chain_id = rlp_item_as_u64(&items[0], "type3.chain_id")?;
    let nonce = rlp_item_as_u64(&items[1], "type3.nonce")?;
    let max_priority_fee_per_gas = rlp_item_as_u64(&items[2], "type3.max_priority_fee_per_gas")?;
    let max_fee_per_gas = rlp_item_as_u64(&items[3], "type3.max_fee_per_gas")?;
    let gas_limit = rlp_item_as_u64(&items[4], "type3.gas_limit")?;
    let to = rlp_item_as_address(&items[5], "type3.to")?;
    let value = rlp_item_as_u128(&items[6], "type3.value")?;
    let data = rlp_item_as_bytes(&items[7], "type3.data")?.to_vec();
    let (access_list_address_count, access_list_storage_key_count) =
        rlp_access_list_intrinsic_counts(&items[8], "type3.access_list")?;
    let max_fee_per_blob_gas = rlp_item_as_u64(&items[9], "type3.max_fee_per_blob_gas")?;
    let blob_hash_count = rlp_blob_hash_count(&items[10], "type3.blob_versioned_hashes")?;
    Ok(EvmRawTxFieldsM0 {
        hint,
        chain_id: Some(chain_id),
        nonce: Some(nonce),
        gas_limit: Some(gas_limit),
        gas_price: Some(max_fee_per_gas),
        max_priority_fee_per_gas: Some(max_priority_fee_per_gas),
        max_fee_per_blob_gas: Some(max_fee_per_blob_gas),
        blob_hash_count: Some(blob_hash_count),
        access_list_address_count: Some(access_list_address_count),
        access_list_storage_key_count: Some(access_list_storage_key_count),
        to,
        value: Some(value),
        data: Some(data),
    })
}

pub fn translate_raw_evm_tx_fields_m0(raw: &[u8]) -> anyhow::Result<EvmRawTxFieldsM0> {
    let hint = resolve_raw_evm_tx_route_hint_m0(raw)?;
    let fields = match hint.envelope {
        EvmRawTxEnvelopeType::Legacy => {
            let items = parse_top_level_rlp_list(raw)?;
            tx_fields_from_legacy_list(&items, hint)
        }
        EvmRawTxEnvelopeType::Type1AccessList => {
            if raw.len() < 2 {
                bail!("type1 tx payload is empty");
            }
            let items = parse_top_level_rlp_list(&raw[1..])?;
            tx_fields_from_type1_list(&items, hint)
        }
        EvmRawTxEnvelopeType::Type2DynamicFee => {
            if raw.len() < 2 {
                bail!("type2 tx payload is empty");
            }
            let items = parse_top_level_rlp_list(&raw[1..])?;
            tx_fields_from_type2_list(&items, hint)
        }
        EvmRawTxEnvelopeType::Type3Blob => {
            if raw.len() < 2 {
                bail!("type3 tx payload is empty");
            }
            let items = parse_top_level_rlp_list(&raw[1..])?;
            tx_fields_from_type3_list(&items, hint)
        }
        EvmRawTxEnvelopeType::Type4SetCode => Ok(EvmRawTxFieldsM0 {
            hint,
            chain_id: None,
            nonce: None,
            gas_limit: None,
            gas_price: None,
            max_priority_fee_per_gas: None,
            max_fee_per_blob_gas: None,
            blob_hash_count: None,
            access_list_address_count: None,
            access_list_storage_key_count: None,
            to: None,
            value: None,
            data: None,
        }),
    }?;
    let chain_id = fields.chain_id.unwrap_or_default();
    match fields.hint.envelope {
        EvmRawTxEnvelopeType::Type1AccessList => {
            if !evm_type1_write_enabled_for_chain(chain_id) {
                bail!(
                    "unsupported eth tx type: access-list (type 1) write path disabled in M0 for chain_id={}",
                    chain_id
                );
            }
        }
        EvmRawTxEnvelopeType::Type2DynamicFee => {
            if !evm_type2_write_enabled_for_chain(chain_id) {
                bail!(
                    "unsupported eth tx type: dynamic-fee (type 2) write path disabled in M0 for chain_id={}",
                    chain_id
                );
            }
        }
        EvmRawTxEnvelopeType::Type3Blob => {
            if !evm_type3_write_enabled_for_chain(chain_id) {
                bail!(
                    "unsupported eth tx type: blob (type 3) write path disabled in M0 for chain_id={}",
                    chain_id
                );
            }
        }
        _ => {}
    }
    Ok(fields)
}

pub fn translate_raw_evm_tx_to_ir_m0(
    raw: &[u8],
    from: Vec<u8>,
    fallback_chain_id: u64,
) -> anyhow::Result<TxIR> {
    let fields = translate_raw_evm_tx_fields_m0(raw)?;
    Ok(tx_ir_from_raw_fields_m0(
        &fields,
        raw,
        from,
        fallback_chain_id,
    ))
}

#[must_use]
pub fn tx_ir_from_raw_fields_m0(
    fields: &EvmRawTxFieldsM0,
    raw: &[u8],
    from: Vec<u8>,
    fallback_chain_id: u64,
) -> TxIR {
    let chain_id = fields.chain_id.unwrap_or(fallback_chain_id);
    let nonce = fields.nonce.unwrap_or(0);
    let gas_limit = fields.gas_limit.unwrap_or(21_000);
    let gas_price = fields.gas_price.unwrap_or(1);
    let to = fields.to.clone();
    let data = fields.data.clone().unwrap_or_default();
    let tx_type = if fields.hint.tx_type4 {
        TxType::ContractCall
    } else if to.is_none() {
        TxType::ContractDeploy
    } else if data.is_empty() {
        TxType::Transfer
    } else {
        TxType::ContractCall
    };

    let mut tx = TxIR {
        hash: Vec::new(),
        from,
        to,
        value: fields.value.unwrap_or(0),
        gas_limit,
        gas_price,
        nonce,
        data,
        signature: raw.to_vec(),
        chain_id,
        tx_type,
        source_chain: None,
        target_chain: None,
    };
    tx.compute_hash();
    tx
}

pub fn translate_raw_evm_block_to_ir_m0(
    block: &EvmRawBlockM0,
    fallback_chain_id: u64,
) -> anyhow::Result<BlockIR> {
    if block.hash.is_empty() {
        bail!("evm block hash is required");
    }
    if block.parent_hash.is_empty() {
        bail!("evm block parent_hash is required");
    }
    let mut transactions = Vec::with_capacity(block.transactions.len());
    for (idx, tx) in block.transactions.iter().enumerate() {
        if tx.from.is_empty() {
            bail!("evm block tx.from is required at index {}", idx);
        }
        if tx.raw_tx.is_empty() {
            bail!("evm block tx.raw_tx is empty at index {}", idx);
        }
        transactions.push(translate_raw_evm_tx_to_ir_m0(
            &tx.raw_tx,
            tx.from.clone(),
            fallback_chain_id,
        )?);
    }

    Ok(BlockIR {
        hash: block.hash.clone(),
        parent_hash: block.parent_hash.clone(),
        number: block.number,
        timestamp: block.timestamp,
        transactions,
        state_root: block.state_root.clone(),
        transactions_root: block.transactions_root.clone(),
        receipts_root: block.receipts_root.clone(),
        miner: block.miner.clone(),
        difficulty: block.difficulty,
        gas_used: block.gas_used,
        gas_limit: block.gas_limit,
    })
}

#[must_use]
pub fn active_precompile_set_m0(profile: &EvmChainProfile) -> &'static [&'static str] {
    match profile.kind {
        EvmChainProfileKind::EthereumMainnet => ETH_MAINNET_PRECOMPILES_M0,
        EvmChainProfileKind::BnbMainnet => BNB_MAINNET_PRECOMPILES_M0,
        EvmChainProfileKind::PolygonMainnet => POLYGON_MAINNET_PRECOMPILES_M0,
        EvmChainProfileKind::AvalancheCChainMainnet => AVALANCHE_CCHAIN_MAINNET_PRECOMPILES_M0,
    }
}

#[must_use]
pub fn estimate_intrinsic_gas_m0(tx: &TxIR) -> u64 {
    const TX_BASE_GAS: u64 = 21_000;
    const TX_CREATE_EXTRA_GAS: u64 = 32_000;
    const TX_DATA_ZERO_BYTE_GAS: u64 = 4;
    const TX_DATA_NON_ZERO_BYTE_GAS: u64 = 16;
    const TX_INITCODE_WORD_GAS: u64 = 2;

    let zero_bytes = tx.data.iter().filter(|b| **b == 0).count() as u64;
    let non_zero_bytes = tx.data.len() as u64 - zero_bytes;
    let mut intrinsic = TX_BASE_GAS
        .saturating_add(zero_bytes.saturating_mul(TX_DATA_ZERO_BYTE_GAS))
        .saturating_add(non_zero_bytes.saturating_mul(TX_DATA_NON_ZERO_BYTE_GAS));
    if matches!(tx.tx_type, TxType::ContractDeploy) {
        let initcode_words = (tx.data.len() as u64).saturating_add(31) / 32;
        intrinsic = intrinsic
            .saturating_add(TX_CREATE_EXTRA_GAS)
            .saturating_add(initcode_words.saturating_mul(TX_INITCODE_WORD_GAS));
    }
    intrinsic
}

#[must_use]
pub fn estimate_access_list_intrinsic_extra_gas_m0(
    access_list_address_count: u64,
    access_list_storage_key_count: u64,
) -> u64 {
    access_list_address_count
        .saturating_mul(2_400)
        .saturating_add(access_list_storage_key_count.saturating_mul(1_900))
}

#[must_use]
pub fn estimate_intrinsic_gas_with_access_list_m0(
    tx: &TxIR,
    access_list_address_count: u64,
    access_list_storage_key_count: u64,
) -> u64 {
    estimate_intrinsic_gas_m0(tx).saturating_add(estimate_access_list_intrinsic_extra_gas_m0(
        access_list_address_count,
        access_list_storage_key_count,
    ))
}

#[must_use]
pub fn estimate_blob_intrinsic_extra_gas_m0(blob_hash_count: u64) -> u64 {
    const BLOB_GAS_PER_BLOB: u64 = 131_072;
    blob_hash_count.saturating_mul(BLOB_GAS_PER_BLOB)
}

#[must_use]
pub fn estimate_intrinsic_gas_with_envelope_extras_m0(
    tx: &TxIR,
    access_list_address_count: u64,
    access_list_storage_key_count: u64,
    blob_hash_count: u64,
) -> u64 {
    estimate_intrinsic_gas_with_access_list_m0(
        tx,
        access_list_address_count,
        access_list_storage_key_count,
    )
    .saturating_add(estimate_blob_intrinsic_extra_gas_m0(blob_hash_count))
}

pub fn validate_tx_semantics_m0(profile: &EvmChainProfile, tx: &TxIR) -> anyhow::Result<()> {
    if tx.chain_id != profile.chain_id {
        bail!(
            "chain_id mismatch for profile: tx_chain={} profile_chain={}",
            tx.chain_id,
            profile.chain_id
        );
    }

    match tx.tx_type {
        TxType::Transfer | TxType::ContractCall => {
            if tx.to.is_none() {
                bail!("tx missing recipient for {:?}", tx.tx_type);
            }
        }
        TxType::ContractDeploy => {
            const MAX_INITCODE_SIZE_BYTES: usize = 49_152;
            if tx.to.is_some() {
                bail!("contract deploy tx must not set recipient");
            }
            if tx.data.is_empty() {
                bail!("contract deploy tx missing init code");
            }
            if tx.data.len() > MAX_INITCODE_SIZE_BYTES {
                bail!(
                    "contract deploy init code too large: len={} max={}",
                    tx.data.len(),
                    MAX_INITCODE_SIZE_BYTES
                );
            }
        }
        _ => {
            bail!(
                "unsupported tx_type in M0 boundary: {:?} (expected Transfer|ContractCall|ContractDeploy)",
                tx.tx_type
            );
        }
    }

    if tx.signature.is_empty() {
        bail!("missing signature");
    }

    let parsed_fields = translate_raw_evm_tx_fields_m0(&tx.signature).ok();
    if let Some(fields) = parsed_fields.as_ref() {
        if let (Some(max_priority), Some(max_fee)) =
            (fields.max_priority_fee_per_gas, fields.gas_price)
        {
            if max_priority > max_fee {
                bail!(
                    "max_priority_fee_per_gas exceeds max_fee_per_gas: priority={} max_fee={}",
                    max_priority,
                    max_fee
                );
            }
        }
        match fields.hint.envelope {
            EvmRawTxEnvelopeType::Type1AccessList => {
                if !evm_type1_write_enabled_for_chain(profile.chain_id) {
                    bail!(
                        "unsupported eth tx type: access-list (type 1) write path disabled in M0 for chain_id={}",
                        profile.chain_id
                    );
                }
            }
            EvmRawTxEnvelopeType::Type2DynamicFee => {
                if !evm_type2_write_enabled_for_chain(profile.chain_id) {
                    bail!(
                        "unsupported eth tx type: dynamic-fee (type 2) write path disabled in M0 for chain_id={}",
                        profile.chain_id
                    );
                }
            }
            EvmRawTxEnvelopeType::Type3Blob => {
                if !evm_type3_write_enabled_for_chain(profile.chain_id) {
                    bail!(
                        "unsupported eth tx type: blob (type 3) write path disabled in M0 for chain_id={}",
                        profile.chain_id
                    );
                }
                if fields.max_fee_per_blob_gas.unwrap_or(0) == 0 {
                    bail!("blob tx max_fee_per_blob_gas must be non-zero");
                }
                if fields.blob_hash_count.unwrap_or(0) == 0 {
                    bail!("blob tx must include at least one blob_versioned_hash");
                }
            }
            EvmRawTxEnvelopeType::Type4SetCode => {
                if matches!(profile.tx_type4_policy, TxType4Policy::Reject) {
                    bail!("unsupported eth tx type: set-code (type 4) rejected by profile");
                }
            }
            _ => {}
        }
    }

    let access_list_address_count = parsed_fields
        .as_ref()
        .and_then(|fields| fields.access_list_address_count)
        .unwrap_or(0);
    let access_list_storage_key_count = parsed_fields
        .as_ref()
        .and_then(|fields| fields.access_list_storage_key_count)
        .unwrap_or(0);
    let blob_hash_count = parsed_fields
        .as_ref()
        .and_then(|fields| fields.blob_hash_count)
        .unwrap_or(0);
    let intrinsic = estimate_intrinsic_gas_with_envelope_extras_m0(
        tx,
        access_list_address_count,
        access_list_storage_key_count,
        blob_hash_count,
    );
    if tx.gas_limit < intrinsic {
        bail!(
            "intrinsic gas too low: gas_limit={} intrinsic={}",
            tx.gas_limit,
            intrinsic
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_bytes(raw: &[u8]) -> Vec<u8> {
        if raw.len() == 1 && raw[0] < 0x80 {
            return vec![raw[0]];
        }
        if raw.len() <= 55 {
            let mut out = Vec::with_capacity(1 + raw.len());
            out.push(0x80 + raw.len() as u8);
            out.extend_from_slice(raw);
            return out;
        }
        let len = raw.len();
        let mut len_bytes = Vec::new();
        let mut n = len;
        while n > 0 {
            len_bytes.push((n & 0xff) as u8);
            n >>= 8;
        }
        len_bytes.reverse();
        let mut out = Vec::with_capacity(1 + len_bytes.len() + raw.len());
        out.push(0xb7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
        out.extend_from_slice(raw);
        out
    }

    fn enc_u64(v: u64) -> Vec<u8> {
        if v == 0 {
            return enc_bytes(&[]);
        }
        let bytes = v.to_be_bytes();
        let first_non_zero = bytes
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(bytes.len() - 1);
        enc_bytes(&bytes[first_non_zero..])
    }

    fn enc_u128(v: u128) -> Vec<u8> {
        if v == 0 {
            return enc_bytes(&[]);
        }
        let bytes = v.to_be_bytes();
        let first_non_zero = bytes
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(bytes.len() - 1);
        enc_bytes(&bytes[first_non_zero..])
    }

    fn enc_list(items: &[Vec<u8>]) -> Vec<u8> {
        let total_len: usize = items.iter().map(Vec::len).sum();
        if total_len <= 55 {
            let mut out = Vec::with_capacity(1 + total_len);
            out.push(0xc0 + total_len as u8);
            for item in items {
                out.extend_from_slice(item);
            }
            return out;
        }
        let mut len_bytes = Vec::new();
        let mut n = total_len;
        while n > 0 {
            len_bytes.push((n & 0xff) as u8);
            n >>= 8;
        }
        len_bytes.reverse();
        let mut out = Vec::with_capacity(1 + len_bytes.len() + total_len);
        out.push(0xf7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
        for item in items {
            out.extend_from_slice(item);
        }
        out
    }

    fn sample_tx(chain_id: u64) -> TxIR {
        TxIR {
            hash: Vec::new(),
            from: vec![1u8; 20],
            to: Some(vec![2u8; 20]),
            value: 1,
            gas_limit: 21_000,
            gas_price: 1,
            nonce: 0,
            data: Vec::new(),
            signature: vec![9u8; 32],
            chain_id,
            tx_type: TxType::Transfer,
            source_chain: None,
            target_chain: None,
        }
    }

    #[test]
    fn supports_evm_family_includes_polygon_and_avalanche() {
        assert!(supports_evm_family(ChainType::EVM));
        assert!(supports_evm_family(ChainType::BNB));
        assert!(supports_evm_family(ChainType::Polygon));
        assert!(supports_evm_family(ChainType::Avalanche));
        assert!(!supports_evm_family(ChainType::Solana));
    }

    #[test]
    fn resolve_profile_supports_m0_evm_family() {
        let eth = resolve_evm_profile(ChainType::EVM, 1).expect("eth profile");
        assert_eq!(eth.kind, EvmChainProfileKind::EthereumMainnet);
        let bnb = resolve_evm_profile(ChainType::BNB, 56).expect("bnb profile");
        assert_eq!(bnb.kind, EvmChainProfileKind::BnbMainnet);
        let polygon = resolve_evm_profile(ChainType::Polygon, 137).expect("polygon profile");
        assert_eq!(polygon.kind, EvmChainProfileKind::PolygonMainnet);
        let avalanche =
            resolve_evm_profile(ChainType::Avalanche, 43114).expect("avalanche profile");
        assert_eq!(avalanche.kind, EvmChainProfileKind::AvalancheCChainMainnet);
    }

    #[test]
    fn intrinsic_gas_matches_base_for_empty_data() {
        let tx = sample_tx(1);
        assert_eq!(estimate_intrinsic_gas_m0(&tx), 21_000);
    }

    #[test]
    fn intrinsic_gas_contract_deploy_includes_create_and_initcode_word_cost() {
        let mut tx = sample_tx(1);
        tx.tx_type = TxType::ContractDeploy;
        tx.to = None;
        tx.data = vec![0x60, 0x00, 0x60, 0x00];
        assert_eq!(estimate_intrinsic_gas_m0(&tx), 53_042);
    }

    #[test]
    fn validate_tx_m0_rejects_low_gas() {
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("profile");
        let mut tx = sample_tx(1);
        tx.gas_limit = 20_999;
        let err = validate_tx_semantics_m0(&profile, &tx).expect_err("must reject low gas");
        assert!(err.to_string().contains("intrinsic gas too low"));
    }

    #[test]
    fn validate_tx_m0_accepts_transfer() {
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("profile");
        let tx = sample_tx(1);
        validate_tx_semantics_m0(&profile, &tx).expect("valid transfer");
    }

    #[test]
    fn validate_tx_m0_accepts_contract_call() {
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("profile");
        let mut tx = sample_tx(1);
        tx.tx_type = TxType::ContractCall;
        tx.data = vec![1, 2, 3];
        tx.gas_limit = 22_000;
        validate_tx_semantics_m0(&profile, &tx).expect("valid contract call");
    }

    #[test]
    fn validate_tx_m0_accepts_contract_deploy() {
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("profile");
        let mut tx = sample_tx(1);
        tx.tx_type = TxType::ContractDeploy;
        tx.to = None;
        tx.data = vec![0x60, 0x00, 0x60, 0x00];
        tx.gas_limit = 53_042;
        validate_tx_semantics_m0(&profile, &tx).expect("valid contract deploy");
    }

    #[test]
    fn validate_tx_m0_rejects_contract_deploy_oversized_initcode() {
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("profile");
        let mut tx = sample_tx(1);
        tx.tx_type = TxType::ContractDeploy;
        tx.to = None;
        tx.data = vec![0x60; 49_153];
        tx.gas_limit = u64::MAX;
        let err =
            validate_tx_semantics_m0(&profile, &tx).expect_err("must reject oversized initcode");
        assert!(err.to_string().contains("init code too large"));
    }

    #[test]
    fn precompile_set_not_empty() {
        let profile = resolve_evm_profile(ChainType::BNB, 56).expect("profile");
        assert!(!active_precompile_set_m0(&profile).is_empty());
        let profile = resolve_evm_profile(ChainType::Polygon, 137).expect("profile");
        assert!(!active_precompile_set_m0(&profile).is_empty());
        let profile = resolve_evm_profile(ChainType::Avalanche, 43114).expect("profile");
        assert!(!active_precompile_set_m0(&profile).is_empty());
    }

    #[test]
    fn resolve_chain_type_from_chain_id_uses_builtin_defaults() {
        assert_eq!(resolve_evm_chain_type_from_chain_id(1), ChainType::EVM);
        assert_eq!(resolve_evm_chain_type_from_chain_id(56), ChainType::BNB);
        assert_eq!(
            resolve_evm_chain_type_from_chain_id(137),
            ChainType::Polygon
        );
        assert_eq!(
            resolve_evm_chain_type_from_chain_id(43114),
            ChainType::Avalanche
        );
        assert_eq!(resolve_evm_chain_type_from_chain_id(8453), ChainType::EVM);
    }

    #[test]
    fn classify_raw_tx_envelope_supports_legacy_and_typed() {
        let legacy = classify_raw_evm_tx_envelope(&[0xf8, 0x00]).expect("legacy envelope");
        assert_eq!(legacy, EvmRawTxEnvelopeType::Legacy);
        let t1 = classify_raw_evm_tx_envelope(&[0x01, 0xc0]).expect("type1 envelope");
        assert_eq!(t1, EvmRawTxEnvelopeType::Type1AccessList);
        let t2 = classify_raw_evm_tx_envelope(&[0x02, 0xc0]).expect("type2 envelope");
        assert_eq!(t2, EvmRawTxEnvelopeType::Type2DynamicFee);
    }

    #[test]
    fn route_hint_m0_accepts_blob_envelope_type() {
        let hint = resolve_raw_evm_tx_route_hint_m0(&[0x03, 0xc0]).expect("blob route hint");
        assert_eq!(hint.tx_type_number, 3);
        assert!(!hint.tx_type4);
    }

    #[test]
    fn route_hint_m0_marks_type4_flag() {
        let hint = resolve_raw_evm_tx_route_hint_m0(&[0x04, 0xc0]).expect("type4 hint");
        assert_eq!(hint.tx_type_number, 4);
        assert!(hint.tx_type4);
    }

    #[test]
    fn translate_legacy_fields_extracts_nonce_and_chain_id() {
        let to = vec![0x11u8; 20];
        let raw = enc_list(&[
            enc_u64(7),
            enc_u64(1),
            enc_u64(21_000),
            enc_bytes(&to),
            enc_u128(9),
            enc_bytes(&[]),
            enc_u64(37),
            enc_u64(1),
            enc_u64(1),
        ]);
        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("legacy tx decode");
        assert_eq!(fields.hint.tx_type_number, 0);
        assert_eq!(fields.chain_id, Some(1));
        assert_eq!(fields.nonce, Some(7));
        assert_eq!(fields.gas_limit, Some(21_000));
        assert_eq!(fields.gas_price, Some(1));
        assert_eq!(fields.access_list_address_count, None);
        assert_eq!(fields.access_list_storage_key_count, None);
        assert_eq!(fields.to, Some(to));
        assert_eq!(fields.value, Some(9));
    }

    #[test]
    fn translate_type1_fields_extracts_core_values() {
        let to = vec![0x22u8; 20];
        let payload = enc_list(&[
            enc_u64(1),
            enc_u64(8),
            enc_u64(2),
            enc_u64(22_000),
            enc_bytes(&to),
            enc_u128(3),
            enc_bytes(&[0xaa, 0xbb]),
            enc_list(&[]),
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw = vec![0x01];
        raw.extend_from_slice(&payload);
        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("type1 tx decode");
        assert_eq!(fields.hint.tx_type_number, 1);
        assert_eq!(fields.chain_id, Some(1));
        assert_eq!(fields.nonce, Some(8));
        assert_eq!(fields.gas_limit, Some(22_000));
        assert_eq!(fields.gas_price, Some(2));
        assert_eq!(fields.access_list_address_count, Some(0));
        assert_eq!(fields.access_list_storage_key_count, Some(0));
        assert_eq!(fields.to, Some(to));
        assert_eq!(fields.value, Some(3));
        assert_eq!(fields.data, Some(vec![0xaa, 0xbb]));
    }

    #[test]
    fn translate_type2_fields_extracts_max_fee_and_nonce() {
        let to = vec![0x33u8; 20];
        let payload = enc_list(&[
            enc_u64(1),
            enc_u64(9),
            enc_u64(2),
            enc_u64(30),
            enc_u64(30_000),
            enc_bytes(&to),
            enc_u128(4),
            enc_bytes(&[]),
            enc_list(&[]),
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw = vec![0x02];
        raw.extend_from_slice(&payload);
        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("type2 tx decode");
        assert_eq!(fields.hint.tx_type_number, 2);
        assert_eq!(fields.chain_id, Some(1));
        assert_eq!(fields.nonce, Some(9));
        assert_eq!(fields.gas_limit, Some(30_000));
        assert_eq!(fields.gas_price, Some(30));
        assert_eq!(fields.access_list_address_count, Some(0));
        assert_eq!(fields.access_list_storage_key_count, Some(0));
        assert_eq!(fields.to, Some(to));
        assert_eq!(fields.value, Some(4));
    }

    #[test]
    fn translate_type3_fields_extracts_blob_fee_and_hash_count() {
        let to = vec![0x44u8; 20];
        let payload = enc_list(&[
            enc_u64(1),
            enc_u64(9),
            enc_u64(2),
            enc_u64(30),
            enc_u64(30_000),
            enc_bytes(&to),
            enc_u128(4),
            enc_bytes(&[]),
            enc_list(&[]),
            enc_u64(7),
            enc_list(&[enc_bytes(&[0x11; 32]), enc_bytes(&[0x22; 32])]),
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let items = parse_top_level_rlp_list(&payload).expect("type3 payload");
        let hint = EvmRawTxRouteHint {
            envelope: EvmRawTxEnvelopeType::Type3Blob,
            tx_type_number: 3,
            tx_type4: false,
        };
        let fields = tx_fields_from_type3_list(&items, hint).expect("type3 decode");
        assert_eq!(fields.chain_id, Some(1));
        assert_eq!(fields.nonce, Some(9));
        assert_eq!(fields.gas_limit, Some(30_000));
        assert_eq!(fields.gas_price, Some(30));
        assert_eq!(fields.max_fee_per_blob_gas, Some(7));
        assert_eq!(fields.blob_hash_count, Some(2));
    }

    #[test]
    fn translate_type3_fields_respects_chain_scoped_type3_toggle() {
        let to = vec![0x44u8; 20];
        let payload = enc_list(&[
            enc_u64(137),
            enc_u64(9),
            enc_u64(2),
            enc_u64(30),
            enc_u64(30_000),
            enc_bytes(&to),
            enc_u128(4),
            enc_bytes(&[]),
            enc_list(&[]),
            enc_u64(7),
            enc_list(&[enc_bytes(&[0x11; 32])]),
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw = vec![0x03];
        raw.extend_from_slice(&payload);

        let keys = [
            "NOVOVM_EVM_ENABLE_TYPE3_WRITE",
            "NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_137",
        ];
        let captured = keys
            .iter()
            .map(|k| (k.to_string(), std::env::var(k).ok()))
            .collect::<Vec<_>>();
        std::env::set_var("NOVOVM_EVM_ENABLE_TYPE3_WRITE", "0");
        std::env::remove_var("NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_137");
        let err = translate_raw_evm_tx_fields_m0(&raw).expect_err("type3 should reject by default");
        assert!(err
            .to_string()
            .contains("blob (type 3) write path disabled in M0"));

        std::env::set_var("NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_137", "1");
        let fields =
            translate_raw_evm_tx_fields_m0(&raw).expect("type3 should pass on chain override");
        assert_eq!(fields.chain_id, Some(137));
        assert_eq!(fields.hint.tx_type_number, 3);

        for (key, value) in captured {
            if let Some(value) = value {
                std::env::set_var(&key, value);
            } else {
                std::env::remove_var(&key);
            }
        }
    }

    #[test]
    fn translate_type1_fields_extracts_access_list_intrinsic_counts() {
        let to = vec![0x55u8; 20];
        let access_list = enc_list(&[
            enc_list(&[
                enc_bytes(&[0x10; 20]),
                enc_list(&[enc_bytes(&[0x01; 32]), enc_bytes(&[0x02; 32])]),
            ]),
            enc_list(&[enc_bytes(&[0x20; 20]), enc_list(&[enc_bytes(&[0x03; 32])])]),
        ]);
        let payload = enc_list(&[
            enc_u64(1),
            enc_u64(8),
            enc_u64(2),
            enc_u64(30_500),
            enc_bytes(&to),
            enc_u128(3),
            enc_bytes(&[0xaa]),
            access_list,
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw = vec![0x01];
        raw.extend_from_slice(&payload);
        let fields = translate_raw_evm_tx_fields_m0(&raw).expect("type1 tx decode");
        assert_eq!(fields.access_list_address_count, Some(2));
        assert_eq!(fields.access_list_storage_key_count, Some(3));
    }

    #[test]
    fn validate_tx_m0_rejects_type1_raw_when_access_list_intrinsic_not_covered() {
        let profile = resolve_evm_profile(ChainType::EVM, 1).expect("profile");
        let to = vec![0x77u8; 20];
        let access_list = enc_list(&[enc_list(&[
            enc_bytes(&[0x31; 20]),
            enc_list(&[enc_bytes(&[0x91; 32])]),
        ])]);
        let payload = enc_list(&[
            enc_u64(1),
            enc_u64(8),
            enc_u64(2),
            enc_u64(25_000),
            enc_bytes(&to),
            enc_u128(3),
            enc_bytes(&[0xaa]),
            access_list,
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw = vec![0x01];
        raw.extend_from_slice(&payload);
        let tx =
            translate_raw_evm_tx_to_ir_m0(&raw, vec![0x7fu8; 20], 1).expect("translate tx to ir");
        let err = validate_tx_semantics_m0(&profile, &tx)
            .expect_err("must reject low gas after access list intrinsic");
        assert!(err.to_string().contains("intrinsic gas too low"));
    }

    #[test]
    fn translate_type2_to_ir_maps_to_transfer() {
        let payload = enc_list(&[
            enc_u64(1),
            enc_u64(0),
            enc_u64(2),
            enc_u64(30),
            enc_u64(30_000),
            enc_bytes(&[0x4e; 20]),
            enc_u128(4),
            enc_bytes(&[]),
            enc_list(&[]),
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw = vec![0x02];
        raw.extend_from_slice(&payload);
        let tx =
            translate_raw_evm_tx_to_ir_m0(&raw, vec![0x7f; 20], 1).expect("translate tx to ir");
        assert_eq!(tx.chain_id, 1);
        assert_eq!(tx.nonce, 0);
        assert_eq!(tx.gas_limit, 30_000);
        assert_eq!(tx.gas_price, 30);
        assert_eq!(tx.tx_type, TxType::Transfer);
        assert_eq!(tx.hash.len(), 32);
        assert!(!tx.signature.is_empty());
    }

    #[test]
    fn translate_type1_with_data_maps_to_contract_call() {
        let payload = enc_list(&[
            enc_u64(1),
            enc_u64(8),
            enc_u64(2),
            enc_u64(22_000),
            enc_bytes(&[0x3e; 20]),
            enc_u128(3),
            enc_bytes(&[0xaa, 0xbb]),
            enc_list(&[]),
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw = vec![0x01];
        raw.extend_from_slice(&payload);
        let tx =
            translate_raw_evm_tx_to_ir_m0(&raw, vec![0x5f; 20], 1).expect("translate tx to ir");
        assert_eq!(tx.chain_id, 1);
        assert_eq!(tx.nonce, 8);
        assert_eq!(tx.tx_type, TxType::ContractCall);
        assert_eq!(tx.data, vec![0xaa, 0xbb]);
    }

    #[test]
    fn translate_raw_block_to_ir_m0_maps_header_and_transactions() {
        let payload_transfer = enc_list(&[
            enc_u64(1),
            enc_u64(0),
            enc_u64(2),
            enc_u64(30),
            enc_u64(30_000),
            enc_bytes(&[0x4e; 20]),
            enc_u128(4),
            enc_bytes(&[]),
            enc_list(&[]),
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw_transfer = vec![0x02];
        raw_transfer.extend_from_slice(&payload_transfer);

        let payload_call = enc_list(&[
            enc_u64(1),
            enc_u64(8),
            enc_u64(2),
            enc_u64(22_000),
            enc_bytes(&[0x3e; 20]),
            enc_u128(3),
            enc_bytes(&[0xaa, 0xbb]),
            enc_list(&[]),
            enc_u64(1),
            enc_u64(1),
            enc_u64(1),
        ]);
        let mut raw_call = vec![0x01];
        raw_call.extend_from_slice(&payload_call);

        let raw_block = EvmRawBlockM0 {
            hash: vec![0x11; 32],
            parent_hash: vec![0x22; 32],
            number: 99,
            timestamp: 1_234_567,
            transactions: vec![
                EvmRawBlockTxM0 {
                    from: vec![0x7f; 20],
                    raw_tx: raw_transfer,
                },
                EvmRawBlockTxM0 {
                    from: vec![0x5f; 20],
                    raw_tx: raw_call,
                },
            ],
            state_root: vec![0x33; 32],
            transactions_root: vec![0x44; 32],
            receipts_root: vec![0x55; 32],
            miner: vec![0x66; 20],
            difficulty: 10,
            gas_used: 52_000,
            gas_limit: 30_000_000,
        };

        let block = translate_raw_evm_block_to_ir_m0(&raw_block, 1).expect("block translator");
        assert_eq!(block.number, 99);
        assert_eq!(block.timestamp, 1_234_567);
        assert_eq!(block.transactions.len(), 2);
        assert_eq!(block.transactions[0].chain_id, 1);
        assert_eq!(block.transactions[0].nonce, 0);
        assert_eq!(block.transactions[0].tx_type, TxType::Transfer);
        assert_eq!(block.transactions[1].nonce, 8);
        assert_eq!(block.transactions[1].tx_type, TxType::ContractCall);
        assert_eq!(block.miner, vec![0x66; 20]);
    }

    #[test]
    fn translate_raw_block_to_ir_m0_rejects_missing_hash() {
        let block = EvmRawBlockM0 {
            hash: Vec::new(),
            parent_hash: vec![0x22; 32],
            number: 1,
            timestamp: 10,
            transactions: Vec::new(),
            state_root: vec![0x33; 32],
            transactions_root: vec![0x44; 32],
            receipts_root: vec![0x55; 32],
            miner: vec![0x66; 20],
            difficulty: 0,
            gas_used: 0,
            gas_limit: 30_000_000,
        };
        let err =
            translate_raw_evm_block_to_ir_m0(&block, 1).expect_err("missing hash should reject");
        assert!(err.to_string().contains("evm block hash is required"));
    }

    #[test]
    fn estimate_intrinsic_with_blob_adds_blob_gas() {
        let tx = sample_tx(1);
        let plain = estimate_intrinsic_gas_with_access_list_m0(&tx, 0, 0);
        let with_blob = estimate_intrinsic_gas_with_envelope_extras_m0(&tx, 0, 0, 2);
        assert_eq!(with_blob, plain.saturating_add(2 * 131_072));
    }
}
