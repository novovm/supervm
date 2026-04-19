use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_consensus::{
    AmmGovernanceParams, BFTConfig, BFTEngine, BondGovernanceParams, BuybackGovernanceParams,
    CdpGovernanceParams, FeeSplit, GovernanceAccessPolicy, GovernanceCouncilMember,
    GovernanceCouncilPolicy, GovernanceCouncilSeat, GovernanceEngineSnapshot, GovernanceOp,
    GovernanceProposal, GovernanceVote, GovernanceVoteVerifierScheme, MarketGovernancePolicy,
    NavGovernanceParams, NetworkDosPolicy, ReserveGovernanceParams, SlashMode, SlashPolicy,
    TokenEconomicsPolicy, ValidatorSet, Web30MarketEngineSnapshot,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::governance_verifier_ext::{
    apply_governance_vote_verifier_v1, encode_mldsa87_vote_signature_envelope_v1,
    GovernanceVoteVerifierConfigV1,
};

const GOVERNANCE_SURFACE_STORE_SCHEMA_V1: &str = "novovm-mainline-governance-surface/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GovernanceSurfaceProposalViewV1 {
    proposal_id: u64,
    proposer: u32,
    created_height: u64,
    proposal_digest: String,
    op: String,
    payload: Value,
    votes_collected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceSurfaceAuditEventV1 {
    pub seq: u64,
    pub ts_sec: u64,
    pub action: String,
    pub proposal_id: u64,
    pub actor: Option<u32>,
    pub outcome: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GovernanceSurfaceSignedVoteMetaV1 {
    signature_scheme: String,
    #[serde(default)]
    external_pubkey_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GovernanceSurfaceStoreV1 {
    schema: String,
    generated_unix_sec: u64,
    validator_ids: Vec<u32>,
    signer_secret_keys_hex: BTreeMap<u32, String>,
    engine_snapshot: GovernanceEngineSnapshot,
    #[serde(default)]
    votes: BTreeMap<u64, Vec<GovernanceVote>>,
    #[serde(default)]
    signed_votes: BTreeMap<String, String>,
    #[serde(default)]
    signed_vote_meta: BTreeMap<String, GovernanceSurfaceSignedVoteMetaV1>,
    #[serde(default)]
    audit_events: Vec<GovernanceSurfaceAuditEventV1>,
    #[serde(default)]
    next_audit_seq: u64,
}

struct GovernanceSurfaceRuntimeV1 {
    engine: BFTEngine,
    signers: BTreeMap<u32, SigningKey>,
    store: GovernanceSurfaceStoreV1,
    store_path: PathBuf,
}

fn now_unix_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn string_env_nonempty_v1(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn encode_hex_v1(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn decode_hex_bytes_v1(raw: &str, field: &str) -> Result<Vec<u8>> {
    let normalized = raw
        .trim()
        .strip_prefix("0x")
        .or_else(|| raw.trim().strip_prefix("0X"))
        .unwrap_or(raw.trim());
    if normalized.is_empty() {
        bail!("{field} is empty");
    }
    if !normalized.len().is_multiple_of(2) {
        bail!("{field} must be even-length hex");
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    for pair in normalized.as_bytes().chunks_exact(2) {
        let hex =
            std::str::from_utf8(pair).with_context(|| format!("{field} contains invalid utf8"))?;
        let byte = u8::from_str_radix(hex, 16)
            .with_context(|| format!("{field} contains invalid hex byte {}", hex))?;
        out.push(byte);
    }
    Ok(out)
}

fn decode_signing_key_v1(raw: &str, node_id: u32) -> Result<SigningKey> {
    let bytes = decode_hex_bytes_v1(raw, "signer_secret_key")?;
    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signer_secret_key for node {} must be 32 bytes", node_id))?;
    Ok(SigningKey::from_bytes(&key_bytes))
}

fn signed_vote_cache_key_v1(proposal_id: u64, voter_id: u32, support: bool) -> String {
    format!(
        "{proposal_id}:{voter_id}:{}",
        if support { "1" } else { "0" }
    )
}

pub fn default_mainline_governance_store_path() -> PathBuf {
    std::env::var("NOVOVM_MAINLINE_GOVERNANCE_STORE_PATH")
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts/mainline/novovm-governance-surface.json"))
}

fn governance_store_path_from_params_or_env_v1(params: &Value) -> PathBuf {
    let from_params = match params {
        Value::Object(map) => map
            .get("governance_store_path")
            .and_then(|value| value.as_str()),
        _ => None,
    }
    .map(|raw| raw.trim().to_string())
    .filter(|raw| !raw.is_empty())
    .map(PathBuf::from);
    from_params
        .or_else(|| {
            string_env_nonempty_v1("NOVOVM_MAINLINE_GOVERNANCE_STORE_PATH").map(PathBuf::from)
        })
        .unwrap_or_else(default_mainline_governance_store_path)
}

fn value_to_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => {
            let trimmed = s.trim();
            if let Some(hex) = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                u64::from_str_radix(hex, 16).ok()
            } else {
                trimmed.parse::<u64>().ok()
            }
        }
        _ => None,
    }
}

fn value_to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn value_to_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.eq_ignore_ascii_case("true")
                || trimmed.eq_ignore_ascii_case("yes")
                || trimmed.eq_ignore_ascii_case("approve")
                || trimmed.eq_ignore_ascii_case("for")
                || trimmed == "1"
            {
                Some(true)
            } else if trimmed.eq_ignore_ascii_case("false")
                || trimmed.eq_ignore_ascii_case("no")
                || trimmed.eq_ignore_ascii_case("reject")
                || trimmed.eq_ignore_ascii_case("against")
                || trimmed == "0"
            {
                Some(false)
            } else {
                None
            }
        }
        Value::Number(n) => n.as_u64().map(|v| v != 0),
        _ => None,
    }
}

fn param_as_u64(params: &Value, key: &str) -> Option<u64> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_to_u64),
        Value::Array(items) => items.first().and_then(value_to_u64),
        _ => None,
    }
}

fn param_as_i64(params: &Value, key: &str) -> Option<i64> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_to_i64),
        _ => None,
    }
}

fn param_as_string(params: &Value, key: &str) -> Option<String> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_to_string),
        Value::Array(items) => items.first().and_then(value_to_string),
        _ => None,
    }
}

fn param_as_bool(params: &Value, key: &str) -> Option<bool> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_to_bool),
        _ => None,
    }
}

fn param_as_u64_list(params: &Value, key: &str) -> Option<Vec<u64>> {
    match params {
        Value::Object(map) => map.get(key).and_then(|value| match value {
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(value_to_u64(item)?);
                }
                Some(out)
            }
            Value::String(raw) => {
                let mut out = Vec::new();
                for token in raw.split(',') {
                    let trimmed = token.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    out.push(trimmed.parse::<u64>().ok()?);
                }
                Some(out)
            }
            _ => None,
        }),
        _ => None,
    }
}

fn parse_slash_mode_v1(raw: &str) -> Result<SlashMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "enforce" => Ok(SlashMode::Enforce),
        "observe_only" | "observe-only" | "observe" => Ok(SlashMode::ObserveOnly),
        other => bail!("unsupported slash mode: {}", other),
    }
}

fn parse_governance_signature_scheme_v1(params: &Value) -> Result<GovernanceVoteVerifierScheme> {
    match param_as_string(params, "signature_scheme")
        .or_else(|| param_as_string(params, "signature_algo"))
        .or_else(|| param_as_string(params, "scheme"))
        .or_else(|| string_env_nonempty_v1("NOVOVM_GOVERNANCE_VOTE_VERIFIER"))
    {
        Some(value) => GovernanceVoteVerifierScheme::parse(&value).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported governance signature scheme: {} (valid: ed25519|mldsa87)",
                value
            )
        }),
        None => Ok(GovernanceVoteVerifierScheme::Ed25519),
    }
}

fn governance_vote_verifier_config_from_params_or_env_v1(
    params: &Value,
) -> Result<GovernanceVoteVerifierConfigV1> {
    let scheme = match param_as_string(params, "governance_vote_verifier")
        .or_else(|| param_as_string(params, "vote_verifier"))
        .or_else(|| string_env_nonempty_v1("NOVOVM_GOVERNANCE_VOTE_VERIFIER"))
    {
        Some(raw) => GovernanceVoteVerifierScheme::parse(&raw).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported NOVOVM_GOVERNANCE_VOTE_VERIFIER: {} (valid: ed25519, mldsa87)",
                raw
            )
        })?,
        None => GovernanceVoteVerifierScheme::Ed25519,
    };
    let mldsa_mode = param_as_string(params, "governance_mldsa_mode")
        .or_else(|| param_as_string(params, "mldsa_mode"))
        .or_else(|| string_env_nonempty_v1("NOVOVM_GOVERNANCE_MLDSA_MODE"));
    let mldsa87_pubkeys = param_as_string(params, "governance_mldsa87_pubkeys")
        .or_else(|| param_as_string(params, "mldsa87_pubkeys"))
        .map(|raw| {
            let mut out = HashMap::new();
            for token in raw.split(',') {
                let entry = token.trim();
                if entry.is_empty() {
                    continue;
                }
                let mut parts = entry.splitn(2, ':');
                let id_raw = parts.next().ok_or_else(|| {
                    anyhow::anyhow!("invalid mldsa pubkey mapping entry: {}", entry)
                })?;
                let pubkey_hex = parts.next().ok_or_else(|| {
                    anyhow::anyhow!("invalid mldsa pubkey mapping entry: {}", entry)
                })?;
                let voter_id = id_raw.trim().parse::<u32>().with_context(|| {
                    format!("invalid mldsa voter id in mapping: {}", id_raw.trim())
                })?;
                let pubkey = decode_hex_bytes_v1(pubkey_hex.trim(), "mldsa_pubkey")?;
                out.insert(voter_id, pubkey);
            }
            if out.is_empty() {
                bail!("governance_mldsa87_pubkeys resolved to empty mapping");
            }
            Ok::<HashMap<u32, Vec<u8>>, anyhow::Error>(out)
        })
        .transpose()?;
    let aoem_dll_path = param_as_string(params, "governance_aoem_dll")
        .or_else(|| param_as_string(params, "aoem_dll"))
        .or_else(|| string_env_nonempty_v1("NOVOVM_AOEM_DLL"))
        .or_else(|| string_env_nonempty_v1("AOEM_DLL"))
        .or_else(|| string_env_nonempty_v1("NOVOVM_AOEM_FFI_LIB_PATH"));
    Ok(GovernanceVoteVerifierConfigV1 {
        scheme,
        mldsa_mode,
        mldsa87_pubkeys,
        aoem_dll_path,
    })
}

fn parse_council_seat_v1(raw: &str) -> Result<GovernanceCouncilSeat> {
    let seat = raw.trim().to_ascii_lowercase();
    if seat == "founder" {
        return Ok(GovernanceCouncilSeat::Founder);
    }
    if seat == "independent" {
        return Ok(GovernanceCouncilSeat::Independent);
    }
    if let Some(rest) = seat.strip_prefix("top_holder_") {
        return Ok(GovernanceCouncilSeat::TopHolder(
            rest.parse::<u8>()
                .with_context(|| format!("invalid top_holder index: {}", rest))?,
        ));
    }
    if let Some(rest) = seat.strip_prefix("team_") {
        return Ok(GovernanceCouncilSeat::Team(
            rest.parse::<u8>()
                .with_context(|| format!("invalid team index: {}", rest))?,
        ));
    }
    bail!(
        "unsupported council seat: {} (expected founder|top_holder_0..4|team_0..1|independent)",
        raw
    )
}

fn parse_governance_council_members_v1(
    params: &Value,
    field: &str,
) -> Result<Vec<GovernanceCouncilMember>> {
    let items = params
        .get(field)
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("{} must be an array", field))?;
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let seat_raw = item
            .get("seat")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("{}.{}: seat is required", field, idx))?;
        let node_id_raw = item
            .get("node_id")
            .and_then(value_to_u64)
            .ok_or_else(|| anyhow::anyhow!("{}.{}: node_id is required", field, idx))?;
        out.push(GovernanceCouncilMember {
            seat: parse_council_seat_v1(seat_raw)?,
            node_id: u32::try_from(node_id_raw)
                .map_err(|_| anyhow::anyhow!("{}.{}: node_id out of range", field, idx))?,
        });
    }
    Ok(out)
}

fn parse_governance_op_v1(params: &Value) -> Result<GovernanceOp> {
    let op = param_as_string(params, "op")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if op.is_empty() {
        bail!("op is required for governance_submitProposal");
    }
    match op.as_str() {
        "update_slash_policy" => {
            let mode = parse_slash_mode_v1(
                &param_as_string(params, "mode").unwrap_or_else(|| "observe_only".to_string()),
            )?;
            let policy = SlashPolicy {
                mode,
                equivocation_threshold: u32::try_from(
                    param_as_u64(params, "equivocation_threshold").ok_or_else(|| {
                        anyhow::anyhow!(
                            "equivocation_threshold is required for update_slash_policy"
                        )
                    })?,
                )
                .map_err(|_| anyhow::anyhow!("equivocation_threshold is out of u32 range"))?,
                min_active_validators: u32::try_from(
                    param_as_u64(params, "min_active_validators").ok_or_else(|| {
                        anyhow::anyhow!("min_active_validators is required for update_slash_policy")
                    })?,
                )
                .map_err(|_| anyhow::anyhow!("min_active_validators is out of u32 range"))?,
                cooldown_epochs: param_as_u64(params, "cooldown_epochs").unwrap_or(0),
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateSlashPolicy { policy })
        }
        "update_mempool_fee_floor" => {
            let fee_floor = param_as_u64(params, "fee_floor")
                .ok_or_else(|| anyhow::anyhow!("fee_floor is required"))?;
            if fee_floor == 0 {
                bail!("fee_floor must be > 0");
            }
            Ok(GovernanceOp::UpdateMempoolFeeFloor { fee_floor })
        }
        "update_network_dos_policy" => {
            let policy = NetworkDosPolicy {
                rpc_rate_limit_per_ip: u32::try_from(
                    param_as_u64(params, "rpc_rate_limit_per_ip").ok_or_else(|| {
                        anyhow::anyhow!(
                            "rpc_rate_limit_per_ip is required for update_network_dos_policy"
                        )
                    })?,
                )
                .map_err(|_| anyhow::anyhow!("rpc_rate_limit_per_ip is out of u32 range"))?,
                peer_ban_threshold: i32::try_from(
                    param_as_i64(params, "peer_ban_threshold").ok_or_else(|| {
                        anyhow::anyhow!(
                            "peer_ban_threshold is required for update_network_dos_policy"
                        )
                    })?,
                )
                .map_err(|_| anyhow::anyhow!("peer_ban_threshold is out of i32 range"))?,
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateNetworkDosPolicy { policy })
        }
        "update_token_economics_policy" => {
            let policy = TokenEconomicsPolicy {
                max_supply: param_as_u64(params, "max_supply")
                    .ok_or_else(|| anyhow::anyhow!("max_supply is required"))?,
                locked_supply: param_as_u64(params, "locked_supply")
                    .ok_or_else(|| anyhow::anyhow!("locked_supply is required"))?,
                fee_split: FeeSplit {
                    gas_base_burn_bp: u16::try_from(
                        param_as_u64(params, "gas_base_burn_bp")
                            .ok_or_else(|| anyhow::anyhow!("gas_base_burn_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("gas_base_burn_bp is out of u16 range"))?,
                    gas_to_node_bp: u16::try_from(
                        param_as_u64(params, "gas_to_node_bp")
                            .ok_or_else(|| anyhow::anyhow!("gas_to_node_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("gas_to_node_bp is out of u16 range"))?,
                    service_burn_bp: u16::try_from(
                        param_as_u64(params, "service_burn_bp")
                            .ok_or_else(|| anyhow::anyhow!("service_burn_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("service_burn_bp is out of u16 range"))?,
                    service_to_provider_bp: u16::try_from(
                        param_as_u64(params, "service_to_provider_bp")
                            .ok_or_else(|| anyhow::anyhow!("service_to_provider_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("service_to_provider_bp is out of u16 range"))?,
                },
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateTokenEconomicsPolicy { policy })
        }
        "update_market_governance_policy" => {
            let policy = MarketGovernancePolicy {
                amm: AmmGovernanceParams {
                    swap_fee_bp: u16::try_from(
                        param_as_u64(params, "amm_swap_fee_bp")
                            .ok_or_else(|| anyhow::anyhow!("amm_swap_fee_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("amm_swap_fee_bp is out of u16 range"))?,
                    lp_fee_share_bp: u16::try_from(
                        param_as_u64(params, "amm_lp_fee_share_bp")
                            .ok_or_else(|| anyhow::anyhow!("amm_lp_fee_share_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("amm_lp_fee_share_bp is out of u16 range"))?,
                },
                cdp: CdpGovernanceParams {
                    min_collateral_ratio_bp: u16::try_from(
                        param_as_u64(params, "cdp_min_collateral_ratio_bp").ok_or_else(|| {
                            anyhow::anyhow!("cdp_min_collateral_ratio_bp is required")
                        })?,
                    )
                    .map_err(|_| {
                        anyhow::anyhow!("cdp_min_collateral_ratio_bp is out of u16 range")
                    })?,
                    liquidation_threshold_bp: u16::try_from(
                        param_as_u64(params, "cdp_liquidation_threshold_bp").ok_or_else(|| {
                            anyhow::anyhow!("cdp_liquidation_threshold_bp is required")
                        })?,
                    )
                    .map_err(|_| {
                        anyhow::anyhow!("cdp_liquidation_threshold_bp is out of u16 range")
                    })?,
                    liquidation_penalty_bp: u16::try_from(
                        param_as_u64(params, "cdp_liquidation_penalty_bp").ok_or_else(|| {
                            anyhow::anyhow!("cdp_liquidation_penalty_bp is required")
                        })?,
                    )
                    .map_err(|_| {
                        anyhow::anyhow!("cdp_liquidation_penalty_bp is out of u16 range")
                    })?,
                    stability_fee_bp: u16::try_from(
                        param_as_u64(params, "cdp_stability_fee_bp")
                            .ok_or_else(|| anyhow::anyhow!("cdp_stability_fee_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("cdp_stability_fee_bp is out of u16 range"))?,
                    max_leverage_x100: u16::try_from(
                        param_as_u64(params, "cdp_max_leverage_x100")
                            .ok_or_else(|| anyhow::anyhow!("cdp_max_leverage_x100 is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("cdp_max_leverage_x100 is out of u16 range"))?,
                },
                bond: BondGovernanceParams {
                    coupon_rate_bp: u16::try_from(
                        param_as_u64(params, "bond_coupon_rate_bp")
                            .ok_or_else(|| anyhow::anyhow!("bond_coupon_rate_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("bond_coupon_rate_bp is out of u16 range"))?,
                    max_maturity_days: u16::try_from(
                        param_as_u64(params, "bond_max_maturity_days")
                            .ok_or_else(|| anyhow::anyhow!("bond_max_maturity_days is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("bond_max_maturity_days is out of u16 range"))?,
                    min_issue_price_bp: u16::try_from(
                        param_as_u64(params, "bond_min_issue_price_bp").ok_or_else(|| {
                            anyhow::anyhow!("bond_min_issue_price_bp is required")
                        })?,
                    )
                    .map_err(|_| anyhow::anyhow!("bond_min_issue_price_bp is out of u16 range"))?,
                },
                reserve: ReserveGovernanceParams {
                    min_reserve_ratio_bp: u16::try_from(
                        param_as_u64(params, "reserve_min_reserve_ratio_bp").ok_or_else(|| {
                            anyhow::anyhow!("reserve_min_reserve_ratio_bp is required")
                        })?,
                    )
                    .map_err(|_| {
                        anyhow::anyhow!("reserve_min_reserve_ratio_bp is out of u16 range")
                    })?,
                    redemption_fee_bp: u16::try_from(
                        param_as_u64(params, "reserve_redemption_fee_bp").ok_or_else(|| {
                            anyhow::anyhow!("reserve_redemption_fee_bp is required")
                        })?,
                    )
                    .map_err(|_| {
                        anyhow::anyhow!("reserve_redemption_fee_bp is out of u16 range")
                    })?,
                },
                nav: NavGovernanceParams {
                    settlement_delay_epochs: u16::try_from(
                        param_as_u64(params, "nav_settlement_delay_epochs").ok_or_else(|| {
                            anyhow::anyhow!("nav_settlement_delay_epochs is required")
                        })?,
                    )
                    .map_err(|_| {
                        anyhow::anyhow!("nav_settlement_delay_epochs is out of u16 range")
                    })?,
                    max_daily_redemption_bp: u16::try_from(
                        param_as_u64(params, "nav_max_daily_redemption_bp").ok_or_else(|| {
                            anyhow::anyhow!("nav_max_daily_redemption_bp is required")
                        })?,
                    )
                    .map_err(|_| {
                        anyhow::anyhow!("nav_max_daily_redemption_bp is out of u16 range")
                    })?,
                },
                buyback: BuybackGovernanceParams {
                    trigger_discount_bp: u16::try_from(
                        param_as_u64(params, "buyback_trigger_discount_bp").ok_or_else(|| {
                            anyhow::anyhow!("buyback_trigger_discount_bp is required")
                        })?,
                    )
                    .map_err(|_| {
                        anyhow::anyhow!("buyback_trigger_discount_bp is out of u16 range")
                    })?,
                    max_treasury_budget_per_epoch: param_as_u64(
                        params,
                        "buyback_max_treasury_budget_per_epoch",
                    )
                    .ok_or_else(|| {
                        anyhow::anyhow!("buyback_max_treasury_budget_per_epoch is required")
                    })?,
                    burn_share_bp: u16::try_from(
                        param_as_u64(params, "buyback_burn_share_bp")
                            .ok_or_else(|| anyhow::anyhow!("buyback_burn_share_bp is required"))?,
                    )
                    .map_err(|_| anyhow::anyhow!("buyback_burn_share_bp is out of u16 range"))?,
                },
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateMarketGovernancePolicy { policy })
        }
        "update_governance_access_policy" => {
            let proposer_committee_raw = param_as_u64_list(params, "proposer_committee")
                .ok_or_else(|| anyhow::anyhow!("proposer_committee is required"))?;
            let executor_committee_raw = param_as_u64_list(params, "executor_committee")
                .ok_or_else(|| anyhow::anyhow!("executor_committee is required"))?;
            let policy = GovernanceAccessPolicy {
                proposer_committee: proposer_committee_raw
                    .into_iter()
                    .map(|id| {
                        u32::try_from(id).map_err(|_| {
                            anyhow::anyhow!("proposer committee id out of range: {}", id)
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                proposer_threshold: u32::try_from(
                    param_as_u64(params, "proposer_threshold")
                        .ok_or_else(|| anyhow::anyhow!("proposer_threshold is required"))?,
                )
                .map_err(|_| anyhow::anyhow!("proposer_threshold is out of u32 range"))?,
                executor_committee: executor_committee_raw
                    .into_iter()
                    .map(|id| {
                        u32::try_from(id).map_err(|_| {
                            anyhow::anyhow!("executor committee id out of range: {}", id)
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                executor_threshold: u32::try_from(
                    param_as_u64(params, "executor_threshold")
                        .ok_or_else(|| anyhow::anyhow!("executor_threshold is required"))?,
                )
                .map_err(|_| anyhow::anyhow!("executor_threshold is out of u32 range"))?,
                timelock_epochs: param_as_u64(params, "timelock_epochs").unwrap_or(0),
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateGovernanceAccessPolicy { policy })
        }
        "update_governance_council_policy" => {
            let enabled = param_as_bool(params, "enabled").unwrap_or(true);
            let defaults = GovernanceCouncilPolicy::disabled();
            let policy = GovernanceCouncilPolicy {
                enabled,
                members: if enabled {
                    parse_governance_council_members_v1(params, "members")?
                } else {
                    params
                        .get("members")
                        .and_then(|value| value.as_array())
                        .map(|_| parse_governance_council_members_v1(params, "members"))
                        .transpose()?
                        .unwrap_or_default()
                },
                parameter_change_threshold_bp: match param_as_u64(
                    params,
                    "parameter_change_threshold_bp",
                ) {
                    Some(v) => u16::try_from(v).map_err(|_| {
                        anyhow::anyhow!("parameter_change_threshold_bp out of range")
                    })?,
                    None => defaults.parameter_change_threshold_bp,
                },
                treasury_spend_threshold_bp: match param_as_u64(
                    params,
                    "treasury_spend_threshold_bp",
                ) {
                    Some(v) => u16::try_from(v)
                        .map_err(|_| anyhow::anyhow!("treasury_spend_threshold_bp out of range"))?,
                    None => defaults.treasury_spend_threshold_bp,
                },
                protocol_upgrade_threshold_bp: match param_as_u64(
                    params,
                    "protocol_upgrade_threshold_bp",
                ) {
                    Some(v) => u16::try_from(v).map_err(|_| {
                        anyhow::anyhow!("protocol_upgrade_threshold_bp out of range")
                    })?,
                    None => defaults.protocol_upgrade_threshold_bp,
                },
                emergency_freeze_threshold_bp: match param_as_u64(
                    params,
                    "emergency_freeze_threshold_bp",
                ) {
                    Some(v) => u16::try_from(v).map_err(|_| {
                        anyhow::anyhow!("emergency_freeze_threshold_bp out of range")
                    })?,
                    None => defaults.emergency_freeze_threshold_bp,
                },
                emergency_min_categories: match param_as_u64(params, "emergency_min_categories") {
                    Some(v) => u8::try_from(v)
                        .map_err(|_| anyhow::anyhow!("emergency_min_categories out of range"))?,
                    None => defaults.emergency_min_categories,
                },
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateGovernanceCouncilPolicy { policy })
        }
        "treasury_spend" => {
            let to = u32::try_from(
                param_as_u64(params, "to").ok_or_else(|| anyhow::anyhow!("to is required"))?,
            )
            .map_err(|_| anyhow::anyhow!("to is out of u32 range"))?;
            let amount = param_as_u64(params, "amount")
                .ok_or_else(|| anyhow::anyhow!("amount is required"))?;
            let reason = param_as_string(params, "reason").unwrap_or_default();
            if amount == 0 {
                bail!("governance_policy_invalid: treasury spend amount must be > 0");
            }
            if reason.is_empty() {
                bail!("governance_policy_invalid: treasury spend reason cannot be empty");
            }
            if reason.len() > 128 {
                bail!("governance_policy_invalid: treasury spend reason too long (max 128)");
            }
            Ok(GovernanceOp::TreasurySpend { to, amount, reason })
        }
        other => bail!("unsupported governance op: {}", other),
    }
}

fn governance_council_policy_to_json_v1(policy: &GovernanceCouncilPolicy) -> Value {
    let members: Vec<_> = policy
        .members
        .iter()
        .map(|member| {
            let seat = match member.seat {
                GovernanceCouncilSeat::Founder => "founder".to_string(),
                GovernanceCouncilSeat::TopHolder(idx) => format!("top_holder_{}", idx),
                GovernanceCouncilSeat::Team(idx) => format!("team_{}", idx),
                GovernanceCouncilSeat::Independent => "independent".to_string(),
            };
            json!({
                "seat": seat,
                "node_id": member.node_id,
            })
        })
        .collect();
    json!({
        "enabled": policy.enabled,
        "members": members,
        "parameter_change_threshold_bp": policy.parameter_change_threshold_bp,
        "treasury_spend_threshold_bp": policy.treasury_spend_threshold_bp,
        "protocol_upgrade_threshold_bp": policy.protocol_upgrade_threshold_bp,
        "emergency_freeze_threshold_bp": policy.emergency_freeze_threshold_bp,
        "emergency_min_categories": policy.emergency_min_categories,
    })
}

fn market_governance_policy_to_json_v1(policy: &MarketGovernancePolicy) -> Value {
    json!({
        "amm": {
            "swap_fee_bp": policy.amm.swap_fee_bp,
            "lp_fee_share_bp": policy.amm.lp_fee_share_bp,
        },
        "cdp": {
            "min_collateral_ratio_bp": policy.cdp.min_collateral_ratio_bp,
            "liquidation_threshold_bp": policy.cdp.liquidation_threshold_bp,
            "liquidation_penalty_bp": policy.cdp.liquidation_penalty_bp,
            "stability_fee_bp": policy.cdp.stability_fee_bp,
            "max_leverage_x100": policy.cdp.max_leverage_x100,
        },
        "bond": {
            "coupon_rate_bp": policy.bond.coupon_rate_bp,
            "max_maturity_days": policy.bond.max_maturity_days,
            "min_issue_price_bp": policy.bond.min_issue_price_bp,
        },
        "reserve": {
            "min_reserve_ratio_bp": policy.reserve.min_reserve_ratio_bp,
            "redemption_fee_bp": policy.reserve.redemption_fee_bp,
        },
        "nav": {
            "settlement_delay_epochs": policy.nav.settlement_delay_epochs,
            "max_daily_redemption_bp": policy.nav.max_daily_redemption_bp,
        },
        "buyback": {
            "trigger_discount_bp": policy.buyback.trigger_discount_bp,
            "max_treasury_budget_per_epoch": policy.buyback.max_treasury_budget_per_epoch,
            "burn_share_bp": policy.buyback.burn_share_bp,
        },
    })
}

fn market_engine_snapshot_to_json_v1(snapshot: &Web30MarketEngineSnapshot) -> Value {
    let mut out = serde_json::Map::new();
    macro_rules! put {
        ($key:literal, $value:expr) => {
            out.insert($key.to_string(), json!($value));
        };
    }
    put!("amm_swap_fee_bp", snapshot.amm_swap_fee_bp);
    put!("amm_lp_fee_share_bp", snapshot.amm_lp_fee_share_bp);
    put!(
        "cdp_min_collateral_ratio_bp",
        snapshot.cdp_min_collateral_ratio_bp
    );
    put!(
        "cdp_liquidation_threshold_bp",
        snapshot.cdp_liquidation_threshold_bp
    );
    put!(
        "cdp_liquidation_penalty_bp",
        snapshot.cdp_liquidation_penalty_bp
    );
    put!("cdp_stability_fee_bp", snapshot.cdp_stability_fee_bp);
    put!("cdp_max_leverage_x100", snapshot.cdp_max_leverage_x100);
    put!("bond_one_year_coupon_bp", snapshot.bond_one_year_coupon_bp);
    put!(
        "bond_three_year_coupon_bp",
        snapshot.bond_three_year_coupon_bp
    );
    put!(
        "bond_five_year_coupon_bp",
        snapshot.bond_five_year_coupon_bp
    );
    put!(
        "bond_max_maturity_days_policy",
        snapshot.bond_max_maturity_days_policy
    );
    put!("bond_min_issue_price_bp", snapshot.bond_min_issue_price_bp);
    put!(
        "reserve_min_reserve_ratio_bp",
        snapshot.reserve_min_reserve_ratio_bp
    );
    put!(
        "reserve_redemption_fee_bp",
        snapshot.reserve_redemption_fee_bp
    );
    put!(
        "nav_settlement_delay_epochs",
        snapshot.nav_settlement_delay_epochs
    );
    put!(
        "nav_max_daily_redemption_bp",
        snapshot.nav_max_daily_redemption_bp
    );
    put!(
        "buyback_trigger_discount_bp",
        snapshot.buyback_trigger_discount_bp
    );
    put!(
        "buyback_max_treasury_budget_per_epoch",
        snapshot.buyback_max_treasury_budget_per_epoch
    );
    put!("buyback_burn_share_bp", snapshot.buyback_burn_share_bp);
    put!("treasury_main_balance", snapshot.treasury_main_balance);
    put!(
        "treasury_ecosystem_balance",
        snapshot.treasury_ecosystem_balance
    );
    put!(
        "treasury_risk_reserve_balance",
        snapshot.treasury_risk_reserve_balance
    );
    put!(
        "reserve_foreign_usdt_balance",
        snapshot.reserve_foreign_usdt_balance
    );
    put!("nav_soft_floor_value", snapshot.nav_soft_floor_value);
    put!(
        "buyback_last_spent_stable",
        snapshot.buyback_last_spent_stable
    );
    put!(
        "buyback_last_burned_token",
        snapshot.buyback_last_burned_token
    );
    put!("oracle_price_before", snapshot.oracle_price_before);
    put!("oracle_price_after", snapshot.oracle_price_after);
    put!(
        "cdp_liquidation_candidates",
        snapshot.cdp_liquidation_candidates
    );
    put!(
        "cdp_liquidations_executed",
        snapshot.cdp_liquidations_executed
    );
    put!(
        "cdp_liquidation_penalty_routed",
        snapshot.cdp_liquidation_penalty_routed
    );
    put!("nav_snapshot_day", snapshot.nav_snapshot_day);
    put!("nav_latest_value", snapshot.nav_latest_value);
    put!("nav_valuation_source", snapshot.nav_valuation_source);
    put!("nav_valuation_price_bp", snapshot.nav_valuation_price_bp);
    put!(
        "nav_valuation_fallback_used",
        snapshot.nav_valuation_fallback_used
    );
    put!(
        "nav_redemptions_submitted",
        snapshot.nav_redemptions_submitted
    );
    put!(
        "nav_redemptions_executed",
        snapshot.nav_redemptions_executed
    );
    put!(
        "nav_executed_stable_total",
        snapshot.nav_executed_stable_total
    );
    put!(
        "dividend_income_received",
        snapshot.dividend_income_received
    );
    put!(
        "dividend_runtime_balance_accounts",
        snapshot.dividend_runtime_balance_accounts
    );
    put!(
        "dividend_eligible_accounts",
        snapshot.dividend_eligible_accounts
    );
    put!(
        "dividend_snapshot_created",
        snapshot.dividend_snapshot_created
    );
    put!(
        "dividend_claims_executed",
        snapshot.dividend_claims_executed
    );
    put!("dividend_pool_balance", snapshot.dividend_pool_balance);
    put!(
        "foreign_payments_processed",
        snapshot.foreign_payments_processed
    );
    put!("foreign_rate_source", snapshot.foreign_rate_source);
    put!(
        "foreign_rate_quote_spec_applied",
        snapshot.foreign_rate_quote_spec_applied
    );
    put!(
        "foreign_rate_fallback_used",
        snapshot.foreign_rate_fallback_used
    );
    put!(
        "foreign_token_paid_total",
        snapshot.foreign_token_paid_total
    );
    put!("foreign_reserve_btc", snapshot.foreign_reserve_btc);
    put!("foreign_reserve_eth", snapshot.foreign_reserve_eth);
    put!(
        "foreign_payment_reserve_usdt",
        snapshot.foreign_payment_reserve_usdt
    );
    put!("foreign_swap_out_total", snapshot.foreign_swap_out_total);
    Value::Object(out)
}

fn governance_op_to_view_v1(op: &GovernanceOp) -> (String, Value) {
    match op {
        GovernanceOp::UpdateSlashPolicy { policy } => (
            "update_slash_policy".to_string(),
            json!({
                "mode": policy.mode.as_str(),
                "equivocation_threshold": policy.equivocation_threshold,
                "min_active_validators": policy.min_active_validators,
                "cooldown_epochs": policy.cooldown_epochs,
            }),
        ),
        GovernanceOp::UpdateMempoolFeeFloor { fee_floor } => (
            "update_mempool_fee_floor".to_string(),
            json!({ "fee_floor": fee_floor }),
        ),
        GovernanceOp::UpdateNetworkDosPolicy { policy } => (
            "update_network_dos_policy".to_string(),
            json!({
                "rpc_rate_limit_per_ip": policy.rpc_rate_limit_per_ip,
                "peer_ban_threshold": policy.peer_ban_threshold,
            }),
        ),
        GovernanceOp::UpdateTokenEconomicsPolicy { policy } => (
            "update_token_economics_policy".to_string(),
            json!({
                "max_supply": policy.max_supply,
                "locked_supply": policy.locked_supply,
                "fee_split": {
                    "gas_base_burn_bp": policy.fee_split.gas_base_burn_bp,
                    "gas_to_node_bp": policy.fee_split.gas_to_node_bp,
                    "service_burn_bp": policy.fee_split.service_burn_bp,
                    "service_to_provider_bp": policy.fee_split.service_to_provider_bp,
                },
            }),
        ),
        GovernanceOp::UpdateMarketGovernancePolicy { policy } => (
            "update_market_governance_policy".to_string(),
            market_governance_policy_to_json_v1(policy),
        ),
        GovernanceOp::UpdateGovernanceAccessPolicy { policy } => (
            "update_governance_access_policy".to_string(),
            json!({
                "proposer_committee": policy.proposer_committee,
                "proposer_threshold": policy.proposer_threshold,
                "executor_committee": policy.executor_committee,
                "executor_threshold": policy.executor_threshold,
                "timelock_epochs": policy.timelock_epochs,
            }),
        ),
        GovernanceOp::UpdateGovernanceCouncilPolicy { policy } => (
            "update_governance_council_policy".to_string(),
            governance_council_policy_to_json_v1(policy),
        ),
        GovernanceOp::TreasurySpend { to, amount, reason } => (
            "treasury_spend".to_string(),
            json!({
                "to": to,
                "amount": amount,
                "reason": reason,
            }),
        ),
    }
}

fn proposal_to_view_v1(
    proposal: &GovernanceProposal,
    votes_collected: usize,
) -> GovernanceSurfaceProposalViewV1 {
    let (op, payload) = governance_op_to_view_v1(&proposal.op);
    GovernanceSurfaceProposalViewV1 {
        proposal_id: proposal.proposal_id,
        proposer: proposal.proposer,
        created_height: proposal.created_height,
        proposal_digest: encode_hex_v1(&proposal.digest()),
        op,
        payload,
        votes_collected,
    }
}

impl GovernanceSurfaceRuntimeV1 {
    fn build_engine_from_store(
        store: &GovernanceSurfaceStoreV1,
        verifier_config: &GovernanceVoteVerifierConfigV1,
    ) -> Result<(BFTEngine, BTreeMap<u32, SigningKey>)> {
        let mut validator_ids = store.validator_ids.clone();
        if validator_ids.is_empty() {
            validator_ids = store.signer_secret_keys_hex.keys().copied().collect();
        }
        validator_ids.sort_unstable();
        validator_ids.dedup();
        if validator_ids.is_empty() {
            bail!("governance surface validator_ids cannot be empty");
        }
        let mut signers = BTreeMap::new();
        let mut public_keys = HashMap::new();
        for node_id in &validator_ids {
            let signer_hex = store.signer_secret_keys_hex.get(node_id).ok_or_else(|| {
                anyhow::anyhow!("missing governance signer secret for node {}", node_id)
            })?;
            let signer = decode_signing_key_v1(signer_hex, *node_id)?;
            public_keys.insert(*node_id, signer.verifying_key());
            signers.insert(*node_id, signer);
        }
        let self_id = *validator_ids
            .first()
            .ok_or_else(|| anyhow::anyhow!("governance validator set is empty"))?;
        let self_signer = signers
            .get(&self_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing governance self signer {}", self_id))?;
        let engine = BFTEngine::new(
            BFTConfig::default(),
            self_id,
            self_signer,
            ValidatorSet::new_equal_weight(validator_ids),
            public_keys,
        )
        .context("init governance consensus engine failed")?;
        apply_governance_vote_verifier_v1(&engine, verifier_config)
            .context("configure governance vote verifier failed")?;
        engine
            .restore_governance_snapshot(store.engine_snapshot.clone())
            .context("restore governance snapshot failed")?;
        Ok((engine, signers))
    }

    fn build_default_store(
        verifier_config: &GovernanceVoteVerifierConfigV1,
    ) -> Result<GovernanceSurfaceStoreV1> {
        let validator_ids = vec![0u32, 1u32, 2u32];
        let mut signer_secret_keys_hex = BTreeMap::new();
        let mut public_keys = HashMap::new();
        let mut first_signer: Option<SigningKey> = None;
        for node_id in &validator_ids {
            let signer = SigningKey::generate(&mut OsRng);
            public_keys.insert(*node_id, signer.verifying_key());
            signer_secret_keys_hex.insert(*node_id, encode_hex_v1(&signer.to_bytes()));
            if first_signer.is_none() {
                first_signer = Some(signer.clone());
            }
        }
        let engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            first_signer.expect("first signer"),
            ValidatorSet::new_equal_weight(validator_ids.clone()),
            public_keys,
        )
        .context("init default governance surface engine failed")?;
        apply_governance_vote_verifier_v1(&engine, verifier_config)
            .context("configure default governance vote verifier failed")?;
        Ok(GovernanceSurfaceStoreV1 {
            schema: GOVERNANCE_SURFACE_STORE_SCHEMA_V1.to_string(),
            generated_unix_sec: now_unix_sec(),
            validator_ids,
            signer_secret_keys_hex,
            engine_snapshot: engine.governance_snapshot(),
            votes: BTreeMap::new(),
            signed_votes: BTreeMap::new(),
            signed_vote_meta: BTreeMap::new(),
            audit_events: Vec::new(),
            next_audit_seq: 0,
        })
    }

    fn load_or_init(path: &Path, verifier_config: &GovernanceVoteVerifierConfigV1) -> Result<Self> {
        let store = if path.exists() {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("read governance store failed: {}", path.display()))?;
            serde_json::from_str::<GovernanceSurfaceStoreV1>(raw.trim_start_matches('\u{feff}'))
                .with_context(|| format!("parse governance store failed: {}", path.display()))?
        } else {
            Self::build_default_store(verifier_config)?
        };
        let (engine, signers) = Self::build_engine_from_store(&store, verifier_config)?;
        Ok(Self {
            engine,
            signers,
            store,
            store_path: path.to_path_buf(),
        })
    }

    fn persist(&mut self) -> Result<()> {
        self.store.generated_unix_sec = now_unix_sec();
        self.store.engine_snapshot = self.engine.governance_snapshot();
        if let Some(parent) = self.store_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("create governance store dir failed: {}", parent.display())
                })?;
            }
        }
        let serialized = serde_json::to_string_pretty(&self.store)
            .context("serialize governance surface store failed")?;
        fs::write(&self.store_path, serialized).with_context(|| {
            format!(
                "write governance store failed: {}",
                self.store_path.display()
            )
        })?;
        Ok(())
    }

    fn push_audit_event(
        &mut self,
        action: &str,
        proposal_id: u64,
        actor: Option<u32>,
        outcome: &str,
        detail: impl Into<String>,
    ) {
        self.store.next_audit_seq = self.store.next_audit_seq.saturating_add(1);
        self.store.audit_events.push(GovernanceSurfaceAuditEventV1 {
            seq: self.store.next_audit_seq,
            ts_sec: now_unix_sec(),
            action: action.to_string(),
            proposal_id,
            actor,
            outcome: outcome.to_string(),
            detail: detail.into(),
        });
    }
}

fn governance_get_policy_v1(runtime: &GovernanceSurfaceRuntimeV1) -> Value {
    let slash = runtime.engine.slash_policy();
    let dos = runtime.engine.governance_network_dos_policy();
    let token = runtime.engine.governance_token_economics_policy();
    let market = runtime.engine.governance_market_policy();
    let market_engine = runtime.engine.governance_market_engine_snapshot();
    let access = runtime.engine.governance_access_policy();
    let council = runtime.engine.governance_council_policy();
    let chain_audit_events = runtime.engine.governance_chain_audit_events();
    json!({
        "method": "governance_getPolicy",
        "slash_policy": {
            "mode": slash.mode.as_str(),
            "equivocation_threshold": slash.equivocation_threshold,
            "min_active_validators": slash.min_active_validators,
            "cooldown_epochs": slash.cooldown_epochs,
        },
        "mempool_fee_floor": runtime.engine.governance_mempool_fee_floor(),
        "network_dos_policy": {
            "rpc_rate_limit_per_ip": dos.rpc_rate_limit_per_ip,
            "peer_ban_threshold": dos.peer_ban_threshold,
        },
        "governance_access_policy": {
            "proposer_committee": access.proposer_committee,
            "proposer_threshold": access.proposer_threshold,
            "executor_committee": access.executor_committee,
            "executor_threshold": access.executor_threshold,
            "timelock_epochs": access.timelock_epochs,
        },
        "governance_council_policy": governance_council_policy_to_json_v1(&council),
        "token_economics_policy": {
            "max_supply": token.max_supply,
            "locked_supply": token.locked_supply,
            "fee_split": {
                "gas_base_burn_bp": token.fee_split.gas_base_burn_bp,
                "gas_to_node_bp": token.fee_split.gas_to_node_bp,
                "service_burn_bp": token.fee_split.service_burn_bp,
                "service_to_provider_bp": token.fee_split.service_to_provider_bp,
            },
        },
        "market_governance_policy": market_governance_policy_to_json_v1(&market),
        "market_engine_snapshot": market_engine_snapshot_to_json_v1(&market_engine),
        "market_runtime_snapshot": market_engine_snapshot_to_json_v1(&market_engine),
        "treasury": {
            "balance": runtime.engine.token_treasury_balance(),
            "spent_total": runtime.engine.token_treasury_spent_total(),
        },
        "governance_chain_audit": {
            "count": chain_audit_events.len(),
            "head_seq": chain_audit_events.last().map(|event| event.seq).unwrap_or(0),
            "root": encode_hex_v1(&runtime.engine.governance_chain_audit_root()),
        },
        "governance_execution_enabled": runtime.engine.governance_execution_enabled(),
    })
}

pub fn is_mainline_governance_query_method(method: &str) -> bool {
    matches!(
        method,
        "governance_getPolicy"
            | "governance_getProposal"
            | "governance_listProposals"
            | "governance_listAuditEvents"
            | "governance_listChainAuditEvents"
            | "governance_submitProposal"
            | "governance_sign"
            | "governance_vote"
            | "governance_execute"
    )
}

pub fn run_mainline_governance_query(method: &str, params: &Value) -> Result<Value> {
    let store_path = governance_store_path_from_params_or_env_v1(params);
    let verifier_config = governance_vote_verifier_config_from_params_or_env_v1(params)?;
    let mut runtime =
        GovernanceSurfaceRuntimeV1::load_or_init(store_path.as_path(), &verifier_config)?;
    let out = match method {
        "governance_getPolicy" => governance_get_policy_v1(&runtime),
        "governance_getProposal" => {
            let proposal_id = param_as_u64(params, "proposal_id")
                .ok_or_else(|| anyhow::anyhow!("proposal_id is required for governance_getProposal"))?;
            let proposal = runtime.engine.governance_pending_proposal(proposal_id);
            let votes_collected = runtime
                .store
                .votes
                .get(&proposal_id)
                .map(|votes| votes.len())
                .unwrap_or(0);
            json!({
                "method": "governance_getProposal",
                "proposal_id": proposal_id,
                "found": proposal.is_some(),
                "proposal": proposal.map(|item| proposal_to_view_v1(&item, votes_collected)),
            })
        }
        "governance_listProposals" => {
            let proposals: Vec<_> = runtime
                .engine
                .governance_pending_proposals()
                .into_iter()
                .map(|proposal| {
                    let votes_collected = runtime
                        .store
                        .votes
                        .get(&proposal.proposal_id)
                        .map(|votes| votes.len())
                        .unwrap_or(0);
                    proposal_to_view_v1(&proposal, votes_collected)
                })
                .collect();
            json!({
                "method": "governance_listProposals",
                "count": proposals.len(),
                "proposals": proposals,
            })
        }
        "governance_listAuditEvents" => {
            let proposal_id_filter = param_as_u64(params, "proposal_id");
            let limit = param_as_u64(params, "limit").unwrap_or(50).clamp(1, 200) as usize;
            let mut events: Vec<_> = runtime
                .store
                .audit_events
                .iter()
                .filter(|event| proposal_id_filter.map(|id| event.proposal_id == id).unwrap_or(true))
                .cloned()
                .collect();
            if events.len() > limit {
                let start = events.len().saturating_sub(limit);
                events = events[start..].to_vec();
            }
            json!({
                "method": "governance_listAuditEvents",
                "count": events.len(),
                "proposal_id_filter": proposal_id_filter,
                "events": events,
            })
        }
        "governance_listChainAuditEvents" => {
            let proposal_id_filter = param_as_u64(params, "proposal_id");
            let since_seq = param_as_u64(params, "since_seq").unwrap_or(0);
            let limit = param_as_u64(params, "limit").unwrap_or(50).clamp(1, 200) as usize;
            let mut events = runtime.engine.governance_chain_audit_events();
            let head_seq = events.last().map(|event| event.seq).unwrap_or(0);
            events.retain(|event| {
                event.seq > since_seq
                    && proposal_id_filter
                        .map(|proposal_id| event.proposal_id == proposal_id)
                        .unwrap_or(true)
            });
            if events.len() > limit {
                let start = events.len().saturating_sub(limit);
                events = events[start..].to_vec();
            }
            json!({
                "method": "governance_listChainAuditEvents",
                "count": events.len(),
                "proposal_id_filter": proposal_id_filter,
                "since_seq": since_seq,
                "head_seq": head_seq,
                "root": encode_hex_v1(&runtime.engine.governance_chain_audit_root()),
                "events": events,
            })
        }
        "governance_submitProposal" => {
            let proposer = u32::try_from(
                param_as_u64(params, "proposer")
                    .or_else(|| param_as_u64(params, "from"))
                    .ok_or_else(|| anyhow::anyhow!("proposer/from is required for governance_submitProposal"))?,
            )
            .map_err(|_| anyhow::anyhow!("proposer is out of u32 range"))?;
            let approvals_raw =
                param_as_u64_list(params, "proposer_approvals").unwrap_or_else(|| vec![u64::from(proposer)]);
            let proposer_approvals = approvals_raw
                .into_iter()
                .map(|id| u32::try_from(id).map_err(|_| anyhow::anyhow!("proposer_approvals id out of range: {}", id)))
                .collect::<Result<Vec<_>>>()?;
            let op = parse_governance_op_v1(params)?;
            match runtime
                .engine
                .submit_governance_proposal_with_approvals(proposer, &proposer_approvals, op)
            {
                Ok(proposal) => {
                    let view = proposal_to_view_v1(&proposal, 0);
                    runtime.push_audit_event(
                        "submit",
                        proposal.proposal_id,
                        Some(proposer),
                        "ok",
                        format!("{} proposer_approvals={}", view.op, proposer_approvals.len()),
                    );
                    runtime.persist()?;
                    json!({
                        "method": "governance_submitProposal",
                        "submitted": true,
                        "proposer_approvals": proposer_approvals,
                        "proposal": view,
                        "entrypoint": "mainline_query",
                    })
                }
                Err(err) => {
                    runtime.push_audit_event("submit", 0, Some(proposer), "reject", err.to_string());
                    runtime.persist()?;
                    return Err(anyhow::anyhow!(err.to_string()));
                }
            }
        }
        "governance_sign" => {
            let proposal_id = param_as_u64(params, "proposal_id")
                .ok_or_else(|| anyhow::anyhow!("proposal_id is required for governance_sign"))?;
            let signer_id = u32::try_from(
                param_as_u64(params, "signer_id")
                    .or_else(|| param_as_u64(params, "signer"))
                    .or_else(|| param_as_u64(params, "from"))
                    .ok_or_else(|| anyhow::anyhow!("signer_id/signer/from is required for governance_sign"))?,
            )
            .map_err(|_| anyhow::anyhow!("signer_id is out of u32 range"))?;
            let support = param_as_bool(params, "support")
                .or_else(|| param_as_bool(params, "vote"))
                .unwrap_or(true);
            let signature_scheme = parse_governance_signature_scheme_v1(params)?;
            if !runtime
                .engine
                .governance_signature_scheme_supported(signature_scheme)
            {
                runtime.push_audit_event(
                    "sign",
                    proposal_id,
                    Some(signer_id),
                    "reject",
                    format!(
                        "unsupported signature scheme {} (current enabled: {})",
                        signature_scheme.as_str(),
                        runtime.engine.governance_vote_verifier_scheme().as_str()
                    ),
                );
                runtime.persist()?;
                bail!(
                    "unsupported governance signature scheme: {} (current enabled: {})",
                    signature_scheme.as_str(),
                    runtime.engine.governance_vote_verifier_scheme().as_str()
                );
            }
            if signature_scheme == GovernanceVoteVerifierScheme::MlDsa87 {
                runtime.push_audit_event(
                    "sign",
                    proposal_id,
                    Some(signer_id),
                    "reject",
                    "mldsa87 local signing is not supported; provide external mldsa signature via governance_vote(signature,mldsa_pubkey)",
                );
                runtime.persist()?;
                bail!(
                    "governance_sign does not support local mldsa87 signing; use governance_vote with external signature and mldsa_pubkey"
                );
            }
            let signer = runtime
                .signers
                .get(&signer_id)
                .ok_or_else(|| anyhow::anyhow!("unknown signer_id: {}", signer_id))?;
            let proposal = runtime
                .engine
                .governance_pending_proposal(proposal_id)
                .ok_or_else(|| anyhow::anyhow!("proposal not found: {}", proposal_id))?;
            let vote = GovernanceVote::new(&proposal, signer_id, support, signer);
            let signature_hex = encode_hex_v1(&vote.signature);
            let cache_key = signed_vote_cache_key_v1(proposal_id, signer_id, support);
            runtime
                .store
                .signed_votes
                .insert(cache_key.clone(), signature_hex.clone());
            runtime.store.signed_vote_meta.insert(
                cache_key,
                GovernanceSurfaceSignedVoteMetaV1 {
                    signature_scheme: signature_scheme.as_str().to_string(),
                    external_pubkey_hex: None,
                },
            );
            runtime.push_audit_event(
                "sign",
                proposal_id,
                Some(signer_id),
                "ok",
                format!(
                    "support={} signature_scheme={}",
                    support,
                    signature_scheme.as_str()
                ),
            );
            runtime.persist()?;
            json!({
                "method": "governance_sign",
                "proposal_id": proposal_id,
                "signer_id": signer_id,
                "support": support,
                "signature_scheme": signature_scheme.as_str(),
                "signature": signature_hex,
                "entrypoint": "mainline_query",
            })
        }
        "governance_vote" => {
            let proposal_id = param_as_u64(params, "proposal_id")
                .ok_or_else(|| anyhow::anyhow!("proposal_id is required for governance_vote"))?;
            let voter_id = u32::try_from(
                param_as_u64(params, "voter_id")
                    .or_else(|| param_as_u64(params, "voter"))
                    .or_else(|| param_as_u64(params, "from"))
                    .ok_or_else(|| anyhow::anyhow!("voter_id/voter/from is required for governance_vote"))?,
            )
            .map_err(|_| anyhow::anyhow!("voter_id is out of u32 range"))?;
            let support = param_as_bool(params, "support")
                .or_else(|| param_as_bool(params, "vote"))
                .unwrap_or(true);
            let signature_scheme = parse_governance_signature_scheme_v1(params)?;
            if !runtime
                .engine
                .governance_signature_scheme_supported(signature_scheme)
            {
                runtime.push_audit_event(
                    "vote",
                    proposal_id,
                    Some(voter_id),
                    "reject",
                    format!(
                        "unsupported signature scheme {} (current enabled: {})",
                        signature_scheme.as_str(),
                        runtime.engine.governance_vote_verifier_scheme().as_str()
                    ),
                );
                runtime.persist()?;
                bail!(
                    "unsupported governance signature scheme: {} (current enabled: {})",
                    signature_scheme.as_str(),
                    runtime.engine.governance_vote_verifier_scheme().as_str()
                );
            }
            let proposal = runtime
                .engine
                .governance_pending_proposal(proposal_id)
                .ok_or_else(|| anyhow::anyhow!("proposal not found: {}", proposal_id))?;
            if runtime
                .store
                .votes
                .get(&proposal_id)
                .map(|votes| votes.iter().any(|vote| vote.voter_id == voter_id))
                .unwrap_or(false)
            {
                runtime.push_audit_event(
                    "vote",
                    proposal_id,
                    Some(voter_id),
                    "reject",
                    "duplicate governance vote",
                );
                runtime.persist()?;
                bail!("duplicate governance vote from voter {}", voter_id);
            }
            let cache_key = signed_vote_cache_key_v1(proposal_id, voter_id, support);
            let vote_signature = if signature_scheme == GovernanceVoteVerifierScheme::MlDsa87 {
                let signature_hex = param_as_string(params, "signature").ok_or_else(|| {
                    anyhow::anyhow!(
                        "signature is required for governance_vote when signature_scheme=mldsa87"
                    )
                })?;
                let mldsa_signature = decode_hex_bytes_v1(&signature_hex, "signature")?;
                let pubkey_hex = param_as_string(params, "mldsa_pubkey")
                    .or_else(|| param_as_string(params, "pubkey"))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "mldsa_pubkey/pubkey is required for governance_vote when signature_scheme=mldsa87"
                        )
                    })?;
                let mldsa_pubkey = decode_hex_bytes_v1(&pubkey_hex, "mldsa_pubkey")?;
                encode_mldsa87_vote_signature_envelope_v1(&mldsa_pubkey, &mldsa_signature)?
            } else if let Some(signature_hex) = param_as_string(params, "signature") {
                decode_hex_bytes_v1(&signature_hex, "signature")?
            } else if let Some(signature_hex) = runtime.store.signed_votes.remove(&cache_key) {
                if let Some(meta) = runtime.store.signed_vote_meta.remove(&cache_key) {
                    if meta.signature_scheme != signature_scheme.as_str() {
                        runtime.push_audit_event(
                            "vote",
                            proposal_id,
                            Some(voter_id),
                            "reject",
                            format!(
                                "cached signature scheme mismatch: requested={} cached={}",
                                signature_scheme.as_str(),
                                meta.signature_scheme
                            ),
                        );
                        runtime.persist()?;
                        bail!(
                            "cached governance signature scheme mismatch: requested={} cached={}",
                            signature_scheme.as_str(),
                            meta.signature_scheme
                        );
                    }
                }
                decode_hex_bytes_v1(&signature_hex, "signature")?
            } else {
                let signer = runtime
                    .signers
                    .get(&voter_id)
                    .ok_or_else(|| anyhow::anyhow!("unknown voter_id: {}", voter_id))?;
                GovernanceVote::new(&proposal, voter_id, support, signer).signature
            };
            let vote = GovernanceVote {
                proposal_id,
                proposal_height: proposal.created_height,
                proposal_digest: proposal.digest(),
                voter_id,
                support,
                signature: vote_signature,
            };
            let votes_collected = {
                let entry = runtime.store.votes.entry(proposal_id).or_default();
                entry.push(vote);
                entry.len()
            };
            runtime.push_audit_event(
                "vote",
                proposal_id,
                Some(voter_id),
                "ok",
                format!(
                    "support={} votes_collected={} signature_scheme={}",
                    support,
                    votes_collected,
                    signature_scheme.as_str()
                ),
            );
            runtime.persist()?;
            json!({
                "method": "governance_vote",
                "proposal_id": proposal_id,
                "voter_id": voter_id,
                "support": support,
                "signature_scheme": signature_scheme.as_str(),
                "votes_collected": votes_collected,
                "entrypoint": "mainline_query",
            })
        }
        "governance_execute" => {
            let proposal_id = param_as_u64(params, "proposal_id")
                .ok_or_else(|| anyhow::anyhow!("proposal_id is required for governance_execute"))?;
            let executor = u32::try_from(
                param_as_u64(params, "executor")
                    .or_else(|| param_as_u64(params, "from"))
                    .ok_or_else(|| anyhow::anyhow!("executor/from is required for governance_execute"))?,
            )
            .map_err(|_| anyhow::anyhow!("executor is out of u32 range"))?;
            let approvals_raw =
                param_as_u64_list(params, "executor_approvals").unwrap_or_else(|| vec![u64::from(executor)]);
            let executor_approvals = approvals_raw
                .into_iter()
                .map(|id| u32::try_from(id).map_err(|_| anyhow::anyhow!("executor_approvals id out of range: {}", id)))
                .collect::<Result<Vec<_>>>()?;
            let votes = runtime.store.votes.get(&proposal_id).cloned().unwrap_or_default();
            match runtime.engine.execute_governance_proposal_with_executor_approvals(
                proposal_id,
                &votes,
                &executor_approvals,
            ) {
                Ok(executed) => {
                    if executed {
                        runtime.store.votes.remove(&proposal_id);
                    }
                    runtime.push_audit_event(
                        "execute",
                        proposal_id,
                        Some(executor),
                        "ok",
                        format!("executed={} executor_approvals={}", executed, executor_approvals.len()),
                    );
                    let slash = runtime.engine.slash_policy();
                    let dos = runtime.engine.governance_network_dos_policy();
                    let token = runtime.engine.governance_token_economics_policy();
                    let market = runtime.engine.governance_market_policy();
                    let market_engine = runtime.engine.governance_market_engine_snapshot();
                    let access = runtime.engine.governance_access_policy();
                    let council = runtime.engine.governance_council_policy();
                    let vote_verifier_name = runtime.engine.governance_vote_verifier_name();
                    let vote_verifier_scheme = runtime.engine.governance_vote_verifier_scheme();
                    runtime.persist()?;
                    json!({
                        "method": "governance_execute",
                        "proposal_id": proposal_id,
                        "executor": executor,
                        "executor_approvals": executor_approvals,
                        "executed": executed,
                        "vote_verifier": {
                            "name": vote_verifier_name,
                            "signature_scheme": vote_verifier_scheme.as_str(),
                        },
                        "slash_policy": {
                            "mode": slash.mode.as_str(),
                            "equivocation_threshold": slash.equivocation_threshold,
                            "min_active_validators": slash.min_active_validators,
                            "cooldown_epochs": slash.cooldown_epochs,
                        },
                        "mempool_fee_floor": runtime.engine.governance_mempool_fee_floor(),
                        "network_dos_policy": {
                            "rpc_rate_limit_per_ip": dos.rpc_rate_limit_per_ip,
                            "peer_ban_threshold": dos.peer_ban_threshold,
                        },
                        "governance_access_policy": {
                            "proposer_committee": access.proposer_committee,
                            "proposer_threshold": access.proposer_threshold,
                            "executor_committee": access.executor_committee,
                            "executor_threshold": access.executor_threshold,
                            "timelock_epochs": access.timelock_epochs,
                        },
                        "governance_council_policy": governance_council_policy_to_json_v1(&council),
                        "token_economics_policy": {
                            "max_supply": token.max_supply,
                            "locked_supply": token.locked_supply,
                            "fee_split": {
                                "gas_base_burn_bp": token.fee_split.gas_base_burn_bp,
                                "gas_to_node_bp": token.fee_split.gas_to_node_bp,
                                "service_burn_bp": token.fee_split.service_burn_bp,
                                "service_to_provider_bp": token.fee_split.service_to_provider_bp,
                            },
                        },
                        "market_governance_policy": market_governance_policy_to_json_v1(&market),
                        "market_engine_snapshot": market_engine_snapshot_to_json_v1(&market_engine),
                        "market_runtime_snapshot": market_engine_snapshot_to_json_v1(&market_engine),
                        "treasury": {
                            "balance": runtime.engine.token_treasury_balance(),
                            "spent_total": runtime.engine.token_treasury_spent_total(),
                        },
                        "entrypoint": "mainline_query",
                    })
                }
                Err(err) => {
                    runtime.push_audit_event(
                        "execute",
                        proposal_id,
                        Some(executor),
                        "reject",
                        err.to_string(),
                    );
                    runtime.persist()?;
                    return Err(anyhow::anyhow!(err.to_string()));
                }
            }
        }
        _ => bail!(
            "unknown governance method: {}; valid: governance_submitProposal|governance_sign|governance_vote|governance_execute|governance_getProposal|governance_listProposals|governance_listAuditEvents|governance_listChainAuditEvents|governance_getPolicy",
            method
        ),
    };
    Ok(out)
}
