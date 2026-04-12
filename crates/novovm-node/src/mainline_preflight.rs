use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use std::path::Path;

pub const MAINLINE_GATE_LOCKSET_V1: &str = "check_novovm_network+check_novovm_node+test_scheduler_gate_matrix+test_manual_route_env_lock_matrix+test_l2_l1_export_equivalence_batch_vs_replay+test_l2_l1_export_equivalence_batch_vs_watch+test_l2_l1_anchor_fingerprint_stable+test_mainline_status_freshness_gate_contract_is_frozen+test_discovery_membership_freshness_contract_default_is_frozen+test_discovery_membership_gossip_json_supports_single_and_vec_message+test_discovery_source_governance_contract_is_frozen+test_discovery_source_breakdown_contract_is_frozen+test_discovery_membership_priority_contract_is_frozen+test_cross_node_runtime_membership_closed_loop+test_cross_node_runtime_membership_cross_region_is_not_admitted+test_cross_node_runtime_membership_newer_unavailable_dominates_older_healthy+test_cross_node_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch+test_cross_node_stale_runtime_membership_prunes_discovered_relay_after_refresh+test_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch+test_operator_forced_still_dominates_route_selection_across_batch_replay_watch+test_pruned_dynamic_relays_no_longer_affect_selection_across_batch_replay_watch+test_concurrent_runtime_membership_order_keeps_selection_view_stable+test_stale_runtime_membership_prunes_discovered_relay_after_refresh+test_cross_node_gossip_membership_order_keeps_selection_view_stable+test_cross_node_gossip_membership_respects_operator_forced_selection+test_runtime_membership_unavailable_update_prunes_existing_discovered_relay+test_v2_matrix_a_order_perturbation_consistency+test_v2_matrix_b_multi_source_conflict_consistency+test_v2_matrix_c_weak_network_disturbance_consistency+test_v2_matrix_d_multi_region_view_consistency+test_v2_stage2a_large_scale_distributed_adjudication_consistency+test_v2_stage2b_weak_network_robustness_consistency+test_v2_stage2c_multi_region_real_route_consistency+test_v2_stage3a_convergence_recovery_consistency+test_v2_stage3b_convergence_time_recovery_budget_consistency+test_relay_path_tests+test_queue_replay_smoke";
pub const REQUIRED_GATE_KEYS_IN_ORDER: [&str; 37] = [
    "check_novovm_network",
    "check_novovm_node",
    "test_scheduler_gate_matrix",
    "test_manual_route_env_lock_matrix",
    "test_l2_l1_export_equivalence_batch_vs_replay",
    "test_l2_l1_export_equivalence_batch_vs_watch",
    "test_l2_l1_anchor_fingerprint_stable",
    "test_mainline_status_freshness_gate_contract_is_frozen",
    "test_discovery_membership_freshness_contract_default_is_frozen",
    "test_discovery_membership_gossip_json_supports_single_and_vec_message",
    "test_discovery_source_governance_contract_is_frozen",
    "test_discovery_source_breakdown_contract_is_frozen",
    "test_discovery_membership_priority_contract_is_frozen",
    "test_cross_node_runtime_membership_closed_loop",
    "test_cross_node_runtime_membership_cross_region_is_not_admitted",
    "test_cross_node_runtime_membership_newer_unavailable_dominates_older_healthy",
    "test_cross_node_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch",
    "test_cross_node_stale_runtime_membership_prunes_discovered_relay_after_refresh",
    "test_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch",
    "test_operator_forced_still_dominates_route_selection_across_batch_replay_watch",
    "test_pruned_dynamic_relays_no_longer_affect_selection_across_batch_replay_watch",
    "test_concurrent_runtime_membership_order_keeps_selection_view_stable",
    "test_stale_runtime_membership_prunes_discovered_relay_after_refresh",
    "test_cross_node_gossip_membership_order_keeps_selection_view_stable",
    "test_cross_node_gossip_membership_respects_operator_forced_selection",
    "test_runtime_membership_unavailable_update_prunes_existing_discovered_relay",
    "test_v2_matrix_a_order_perturbation_consistency",
    "test_v2_matrix_b_multi_source_conflict_consistency",
    "test_v2_matrix_c_weak_network_disturbance_consistency",
    "test_v2_matrix_d_multi_region_view_consistency",
    "test_v2_stage2a_large_scale_distributed_adjudication_consistency",
    "test_v2_stage2b_weak_network_robustness_consistency",
    "test_v2_stage2c_multi_region_real_route_consistency",
    "test_v2_stage3a_convergence_recovery_consistency",
    "test_v2_stage3b_convergence_time_recovery_budget_consistency",
    "test_relay_path_tests",
    "test_queue_replay_smoke",
];

pub struct PreflightOutcome {
    pub status_path: std::path::PathBuf,
    pub delivery_path: std::path::PathBuf,
    pub age_seconds: i64,
    pub max_age_seconds: i64,
}

fn contract_bail<T>(code: &str, detail: impl AsRef<str>) -> Result<T> {
    bail!("mainline gate contract failed [{}]: {}", code, detail.as_ref());
}

fn read_json(path: &Path, missing_code: &str, empty_code: &str, parse_code: &str) -> Result<Value> {
    if !path.exists() {
        return contract_bail(missing_code, format!("missing: {}", path.display()));
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read json: {}", path.display()))?;
    if raw.trim().is_empty() {
        return contract_bail(empty_code, format!("empty: {}", path.display()));
    }
    serde_json::from_str(&raw).map_err(|_| {
        anyhow::anyhow!(
            "mainline gate contract failed [{}]: parse failed: {}",
            parse_code,
            path.display()
        )
    })
}

fn require_object<'a>(
    root: &'a Value,
    key: &str,
    code: &str,
    detail: &str,
) -> Result<&'a Map<String, Value>> {
    root.get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("mainline gate contract failed [{}]: {}", code, detail))
}

fn require_non_empty_string<'a>(
    root: &'a Value,
    key: &str,
    code: &str,
    detail: &str,
) -> Result<&'a str> {
    let s = root
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if s.is_empty() {
        return contract_bail(code, detail);
    }
    Ok(s)
}

pub fn run_preflight(repo_root: &Path) -> Result<PreflightOutcome> {
    let status_path = repo_root.join("artifacts").join("mainline-status.json");
    let delivery_path = repo_root.join("artifacts").join("mainline-delivery-contract.json");

    let status = read_json(
        &status_path,
        "status.missing",
        "status.empty",
        "status.parse_failed",
    )?;
    let status_schema = status
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if status_schema != "supervm-mainline-status/v2" {
        return contract_bail(
            "status.schema_mismatch",
            format!(
                "expected supervm-mainline-status/v2, got '{}'",
                status_schema
            ),
        );
    }

    let generated_utc = require_non_empty_string(
        &status,
        "generated_utc",
        "status.generated_utc_missing",
        "mainline gate payload missing generated_utc",
    )?;
    let generated_time = DateTime::parse_from_rfc3339(generated_utc).map_err(|_| {
        anyhow::anyhow!(
            "mainline gate contract failed [status.generated_utc_missing]: invalid generated_utc format: '{}'",
            generated_utc
        )
    })?;

    let mut max_age_seconds: i64 = 3600;
    if let Ok(v) = std::env::var("NOVOVM_SUPERVM_MAINLINE_STATUS_MAX_AGE_SECONDS") {
        let parsed = v.parse::<i64>().map_err(|_| {
            anyhow::anyhow!(
                "mainline gate contract failed [freshness.invalid_max_age_env]: invalid NOVOVM_SUPERVM_MAINLINE_STATUS_MAX_AGE_SECONDS: '{}'",
                v
            )
        })?;
        if parsed < 0 {
            return contract_bail(
                "freshness.invalid_max_age_range",
                "NOVOVM_SUPERVM_MAINLINE_STATUS_MAX_AGE_SECONDS must be >= 0",
            );
        }
        max_age_seconds = parsed;
    }

    let age_seconds = (Utc::now() - generated_time.with_timezone(&Utc))
        .num_seconds()
        .max(0);
    if age_seconds > max_age_seconds {
        return contract_bail(
            "freshness.expired",
            format!(
                "mainline gate status expired: age={}s, max={}s",
                age_seconds, max_age_seconds
            ),
        );
    }

    let gate = require_object(
        &status,
        "gate",
        "gate.object_missing",
        "mainline gate payload missing 'gate' object",
    )?;
    for key in REQUIRED_GATE_KEYS_IN_ORDER {
        let gate_val = gate.get(key).ok_or_else(|| {
            anyhow::anyhow!(
                "mainline gate contract failed [gate.key_missing]: key missing: {}",
                key
            )
        })?;
        if gate_val.as_bool() != Some(true) {
            return contract_bail("gate.key_not_passed", format!("key not passed: {}", key));
        }
    }

    let keys_in_order: Vec<&str> = gate.keys().map(String::as_str).collect();
    if keys_in_order != REQUIRED_GATE_KEYS_IN_ORDER {
        return contract_bail(
            "gate.key_order_mismatch",
            format!(
                "expected '{}', got '{}'",
                REQUIRED_GATE_KEYS_IN_ORDER.join(","),
                keys_in_order.join(",")
            ),
        );
    }

    let mut keys_sorted: Vec<&str> = keys_in_order.clone();
    keys_sorted.sort_unstable();
    let mut required_sorted: Vec<&str> = REQUIRED_GATE_KEYS_IN_ORDER.to_vec();
    required_sorted.sort_unstable();
    if keys_sorted != required_sorted {
        return contract_bail(
            "gate.keyset_drift",
            format!(
                "expected '{}', got '{}'",
                required_sorted.join(","),
                keys_sorted.join(",")
            ),
        );
    }

    let gate_lockset = status
        .get("lockset")
        .and_then(Value::as_object)
        .and_then(|v| v.get("gate"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if gate_lockset.trim().is_empty() {
        return contract_bail("gate.lockset_missing", "mainline gate payload missing lockset.gate");
    }

    let lockset_tokens: Vec<&str> = gate_lockset.split('+').collect();
    if lockset_tokens != REQUIRED_GATE_KEYS_IN_ORDER {
        return contract_bail(
            "gate.lockset_token_order_mismatch",
            format!(
                "expected '{}', got '{}'",
                REQUIRED_GATE_KEYS_IN_ORDER.join(","),
                lockset_tokens.join(",")
            ),
        );
    }
    if gate_lockset != MAINLINE_GATE_LOCKSET_V1 {
        return contract_bail(
            "gate.lockset_mismatch",
            format!(
                "expected '{}', got '{}'",
                MAINLINE_GATE_LOCKSET_V1, gate_lockset
            ),
        );
    }

    let overall_gate = status
        .get("layers")
        .and_then(Value::as_object)
        .and_then(|v| v.get("overall"))
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let delivery = read_json(
        &delivery_path,
        "delivery.missing",
        "delivery.empty",
        "delivery.parse_failed",
    )?;
    let delivery_schema = delivery
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if delivery_schema != "supervm-mainline-delivery/v1" {
        return contract_bail(
            "delivery.schema_mismatch",
            format!(
                "expected supervm-mainline-delivery/v1, got '{}'",
                delivery_schema
            ),
        );
    }

    let delivery_status_source = delivery
        .get("status_source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if delivery_status_source != "artifacts/mainline-status.json" {
        return contract_bail(
            "delivery.status_source_mismatch",
            format!(
                "expected 'artifacts/mainline-status.json', got '{}'",
                delivery_status_source
            ),
        );
    }

    let delivery_gate_entry = delivery
        .get("gate_entry")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if delivery_gate_entry != "cargo run -p novovm-node --bin supervm-mainline-gate" {
        return contract_bail(
            "delivery.gate_entry_mismatch",
            format!(
                "expected 'cargo run -p novovm-node --bin supervm-mainline-gate', got '{}'",
                delivery_gate_entry
            ),
        );
    }

    let delivery_overall_gate = delivery
        .get("overall_gate")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    if delivery_overall_gate != overall_gate {
        return contract_bail(
            "delivery.overall_gate_mismatch",
            format!(
                "expected '{}', got '{}'",
                overall_gate, delivery_overall_gate
            ),
        );
    }

    let delivery_lockset = require_object(
        &delivery,
        "lockset",
        "delivery.lockset_missing",
        "mainline delivery lockset missing",
    )?;
    let fieldset_version = delivery_lockset
        .get("fieldset_version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if fieldset_version != "milestone+gate+debt:v1" {
        return contract_bail(
            "delivery.lockset_fieldset_version_mismatch",
            format!(
                "expected 'milestone+gate+debt:v1', got '{}'",
                fieldset_version
            ),
        );
    }

    let mut status_for_write = status.clone();
    let checked_utc = Utc::now().to_rfc3339();
    let preflight_block = json!({
        "schema": "supervm-mainline-preflight/v1",
        "pass": true,
        "checked_utc": checked_utc,
        "status_source": "artifacts/mainline-status.json",
        "delivery_source": "artifacts/mainline-delivery-contract.json",
        "age_seconds": age_seconds,
        "max_age_seconds": max_age_seconds,
        "gate_lockset_ok": true,
        "delivery_lockset_ok": true,
        "codes": []
    });
    status_for_write
        .as_object_mut()
        .expect("status payload must be object")
        .insert("preflight".to_string(), preflight_block);
    let encoded = serde_json::to_string_pretty(&status_for_write)
        .context("encode status with preflight block")?;
    std::fs::write(&status_path, format!("{encoded}\n"))
        .with_context(|| format!("rewrite status with preflight block: {}", status_path.display()))?;

    Ok(PreflightOutcome {
        status_path,
        delivery_path,
        age_seconds,
        max_age_seconds,
    })
}
