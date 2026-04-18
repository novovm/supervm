use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::process::Command;

#[path = "../mainline_preflight.rs"]
mod mainline_preflight;

#[derive(Debug, Serialize)]
struct MainlineGate {
    check_novovm_network: bool,
    check_novovm_node: bool,
    test_scheduler_gate_matrix: bool,
    test_manual_route_env_lock_matrix: bool,
    test_l2_l1_export_equivalence_batch_vs_replay: bool,
    test_l2_l1_export_equivalence_batch_vs_watch: bool,
    test_l2_l1_anchor_fingerprint_stable: bool,
    test_mainline_status_freshness_gate_contract_is_frozen: bool,
    test_discovery_membership_freshness_contract_default_is_frozen: bool,
    test_discovery_membership_gossip_json_supports_single_and_vec_message: bool,
    test_discovery_source_governance_contract_is_frozen: bool,
    test_discovery_source_breakdown_contract_is_frozen: bool,
    test_discovery_membership_priority_contract_is_frozen: bool,
    test_cross_node_runtime_membership_closed_loop: bool,
    test_cross_node_runtime_membership_cross_region_is_not_admitted: bool,
    test_cross_node_runtime_membership_newer_unavailable_dominates_older_healthy: bool,
    test_cross_node_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch:
        bool,
    test_cross_node_stale_runtime_membership_prunes_discovered_relay_after_refresh: bool,
    test_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch: bool,
    test_operator_forced_still_dominates_route_selection_across_batch_replay_watch: bool,
    test_pruned_dynamic_relays_no_longer_affect_selection_across_batch_replay_watch: bool,
    test_concurrent_runtime_membership_order_keeps_selection_view_stable: bool,
    test_stale_runtime_membership_prunes_discovered_relay_after_refresh: bool,
    test_cross_node_gossip_membership_order_keeps_selection_view_stable: bool,
    test_cross_node_gossip_membership_respects_operator_forced_selection: bool,
    test_runtime_membership_unavailable_update_prunes_existing_discovered_relay: bool,
    test_v2_matrix_a_order_perturbation_consistency: bool,
    test_v2_matrix_b_multi_source_conflict_consistency: bool,
    test_v2_matrix_c_weak_network_disturbance_consistency: bool,
    test_v2_matrix_d_multi_region_view_consistency: bool,
    test_v2_stage2a_large_scale_distributed_adjudication_consistency: bool,
    test_v2_stage2b_weak_network_robustness_consistency: bool,
    test_v2_stage2c_multi_region_real_route_consistency: bool,
    test_v2_stage3a_convergence_recovery_consistency: bool,
    test_v2_stage3b_convergence_time_recovery_budget_consistency: bool,
    test_relay_path_tests: bool,
    test_queue_replay_smoke: bool,
}

#[derive(Debug, Serialize)]
struct MainlineLayers {
    l1: u8,
    l2: u8,
    l3: u8,
    l4: u8,
    overall: u8,
}

#[derive(Debug, Serialize)]
struct MainlineLockset {
    gate: &'static str,
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct MainlineDeliveryLockset {
    contract_version: &'static str,
    fieldset_version: &'static str,
    fieldset_fingerprint: &'static str,
}

#[derive(Debug, Serialize)]
struct MainlineDeliveryContract {
    schema: &'static str,
    generated_utc: String,
    milestone: &'static str,
    gate_entry: &'static str,
    status_source: &'static str,
    overall_gate: u8,
    debt: u8,
    lockset: MainlineDeliveryLockset,
}

#[derive(Debug, Serialize)]
struct MainlineStatus {
    schema: &'static str,
    generated_utc: String,
    gate: MainlineGate,
    layers: MainlineLayers,
    lockset: MainlineLockset,
}

fn run_step(name: &str, program: &str, args: &[&str]) -> Result<()> {
    println!("==> {name}");
    let mut cmd = Command::new(program);
    cmd.args(args);
    if program.eq_ignore_ascii_case("cargo") {
        cmd.env("CARGO_TARGET_DIR", resolve_gate_target_dir_v1());
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to run step: {name}"))?;

    if !status.success() {
        match status.code() {
            Some(code) => bail!("Step failed: {name} (exit={code})"),
            None => bail!("Step failed: {name} (terminated by signal)"),
        }
    }

    Ok(())
}

fn resolve_gate_target_dir_v1() -> String {
    if let Ok(raw) = std::env::var("CARGO_TARGET_DIR") {
        let trimmed = raw.trim();
        // On non-Windows runners, a Windows-style value like `D:\...`
        // propagates `:` into LD_LIBRARY_PATH join and breaks cargo.
        let linux_safe = cfg!(windows) || !trimmed.contains(':');
        if !trimmed.is_empty() && linux_safe {
            // Child cargo invocations must use an isolated target dir; otherwise
            // on Windows the running gate binary can be locked while cargo tries
            // to rebuild/remove it during subsequent test steps.
            return format!("{trimmed}-gate");
        }
    }
    if cfg!(windows) {
        "D:\\cargo-target-supervm-gate".to_string()
    } else {
        "target/cargo-target-supervm-gate".to_string()
    }
}

fn main() -> Result<()> {
    let lockset_from_order = mainline_preflight::REQUIRED_GATE_KEYS_IN_ORDER.join("+");
    if lockset_from_order != mainline_preflight::MAINLINE_GATE_LOCKSET_V1 {
        bail!(
            "mainline gate lockset contract drift: key_order='{}' lockset='{}'",
            lockset_from_order,
            mainline_preflight::MAINLINE_GATE_LOCKSET_V1
        );
    }

    let mut gate = MainlineGate {
        check_novovm_network: false,
        check_novovm_node: false,
        test_scheduler_gate_matrix: false,
        test_manual_route_env_lock_matrix: false,
        test_l2_l1_export_equivalence_batch_vs_replay: false,
        test_l2_l1_export_equivalence_batch_vs_watch: false,
        test_l2_l1_anchor_fingerprint_stable: false,
        test_mainline_status_freshness_gate_contract_is_frozen: false,
        test_discovery_membership_freshness_contract_default_is_frozen: false,
        test_discovery_membership_gossip_json_supports_single_and_vec_message: false,
        test_discovery_source_governance_contract_is_frozen: false,
        test_discovery_source_breakdown_contract_is_frozen: false,
        test_discovery_membership_priority_contract_is_frozen: false,
        test_cross_node_runtime_membership_closed_loop: false,
        test_cross_node_runtime_membership_cross_region_is_not_admitted: false,
        test_cross_node_runtime_membership_newer_unavailable_dominates_older_healthy: false,
        test_cross_node_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch: false,
        test_cross_node_stale_runtime_membership_prunes_discovered_relay_after_refresh: false,
        test_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch: false,
        test_operator_forced_still_dominates_route_selection_across_batch_replay_watch: false,
        test_pruned_dynamic_relays_no_longer_affect_selection_across_batch_replay_watch: false,
        test_concurrent_runtime_membership_order_keeps_selection_view_stable: false,
        test_stale_runtime_membership_prunes_discovered_relay_after_refresh: false,
        test_cross_node_gossip_membership_order_keeps_selection_view_stable: false,
        test_cross_node_gossip_membership_respects_operator_forced_selection: false,
        test_runtime_membership_unavailable_update_prunes_existing_discovered_relay: false,
        test_v2_matrix_a_order_perturbation_consistency: false,
        test_v2_matrix_b_multi_source_conflict_consistency: false,
        test_v2_matrix_c_weak_network_disturbance_consistency: false,
        test_v2_matrix_d_multi_region_view_consistency: false,
        test_v2_stage2a_large_scale_distributed_adjudication_consistency: false,
        test_v2_stage2b_weak_network_robustness_consistency: false,
        test_v2_stage2c_multi_region_real_route_consistency: false,
        test_v2_stage3a_convergence_recovery_consistency: false,
        test_v2_stage3b_convergence_time_recovery_budget_consistency: false,
        test_relay_path_tests: false,
        test_queue_replay_smoke: false,
    };

    run_step(
        "check novovm-network",
        "cargo",
        &["check", "-p", "novovm-network"],
    )?;
    gate.check_novovm_network = true;

    run_step(
        "check novovm-node",
        "cargo",
        &["check", "-p", "novovm-node"],
    )?;
    gate.check_novovm_node = true;

    run_step(
        "test scheduler_gate_matrix",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "scheduler_gate_matrix",
        ],
    )?;
    gate.test_scheduler_gate_matrix = true;

    run_step(
        "test manual_route_env_lock_matrix",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "manual_route_env_lock_matrix",
        ],
    )?;
    gate.test_manual_route_env_lock_matrix = true;

    run_step(
        "test l2_l1_export_equivalence_batch_vs_replay",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "l2_l1_export_equivalence_batch_vs_replay",
        ],
    )?;
    gate.test_l2_l1_export_equivalence_batch_vs_replay = true;

    run_step(
        "test l2_l1_export_equivalence_batch_vs_watch",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "l2_l1_export_equivalence_batch_vs_watch",
        ],
    )?;
    gate.test_l2_l1_export_equivalence_batch_vs_watch = true;

    run_step(
        "test l2_l1_anchor_fingerprint_stable",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "l2_l1_anchor_fingerprint_stable",
        ],
    )?;
    gate.test_l2_l1_anchor_fingerprint_stable = true;

    run_step(
        "test mainline_status_freshness_gate_contract_is_frozen",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "mainline_status_freshness_gate_contract_is_frozen",
        ],
    )?;
    gate.test_mainline_status_freshness_gate_contract_is_frozen = true;

    run_step(
        "test discovery_membership_freshness_contract_default_is_frozen",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "discovery_membership_freshness_contract_default_is_frozen",
        ],
    )?;
    gate.test_discovery_membership_freshness_contract_default_is_frozen = true;

    run_step(
        "test discovery_membership_gossip_json_supports_single_and_vec_message",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "discovery_membership_gossip_json_supports_single_and_vec_message",
        ],
    )?;
    gate.test_discovery_membership_gossip_json_supports_single_and_vec_message = true;

    run_step(
        "test discovery_source_governance_contract_is_frozen",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "discovery_source_governance_contract_is_frozen",
        ],
    )?;
    gate.test_discovery_source_governance_contract_is_frozen = true;

    run_step(
        "test discovery_source_breakdown_contract_is_frozen",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "discovery_source_breakdown_contract_is_frozen",
        ],
    )?;
    gate.test_discovery_source_breakdown_contract_is_frozen = true;

    run_step(
        "test discovery_membership_priority_contract_is_frozen",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "discovery_membership_priority_contract_is_frozen",
        ],
    )?;
    gate.test_discovery_membership_priority_contract_is_frozen = true;

    run_step(
        "test cross_node_runtime_membership_closed_loop",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "cross_node_runtime_membership_closed_loop",
        ],
    )?;
    gate.test_cross_node_runtime_membership_closed_loop = true;

    run_step(
        "test cross_node_runtime_membership_cross_region_is_not_admitted",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "cross_node_runtime_membership_cross_region_is_not_admitted",
        ],
    )?;
    gate.test_cross_node_runtime_membership_cross_region_is_not_admitted = true;

    run_step(
        "test cross_node_runtime_membership_newer_unavailable_dominates_older_healthy",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "cross_node_runtime_membership_newer_unavailable_dominates_older_healthy",
        ],
    )?;
    gate.test_cross_node_runtime_membership_newer_unavailable_dominates_older_healthy = true;

    run_step(
        "test cross_node_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "cross_node_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch",
        ],
    )?;
    gate.test_cross_node_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch =
        true;

    run_step(
        "test cross_node_stale_runtime_membership_prunes_discovered_relay_after_refresh",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "cross_node_stale_runtime_membership_prunes_discovered_relay_after_refresh",
        ],
    )?;
    gate.test_cross_node_stale_runtime_membership_prunes_discovered_relay_after_refresh = true;

    run_step(
        "test runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch",
        ],
    )?;
    gate.test_runtime_membership_can_affect_l3_route_selection_across_batch_replay_watch = true;

    run_step(
        "test operator_forced_still_dominates_route_selection_across_batch_replay_watch",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "operator_forced_still_dominates_route_selection_across_batch_replay_watch",
        ],
    )?;
    gate.test_operator_forced_still_dominates_route_selection_across_batch_replay_watch = true;

    run_step(
        "test pruned_dynamic_relays_no_longer_affect_selection_across_batch_replay_watch",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "pruned_dynamic_relays_no_longer_affect_selection_across_batch_replay_watch",
        ],
    )?;
    gate.test_pruned_dynamic_relays_no_longer_affect_selection_across_batch_replay_watch = true;

    run_step(
        "test concurrent_runtime_membership_order_keeps_selection_view_stable",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "concurrent_runtime_membership_order_keeps_selection_view_stable",
        ],
    )?;
    gate.test_concurrent_runtime_membership_order_keeps_selection_view_stable = true;

    run_step(
        "test stale_runtime_membership_prunes_discovered_relay_after_refresh",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "stale_runtime_membership_prunes_discovered_relay_after_refresh",
        ],
    )?;
    gate.test_stale_runtime_membership_prunes_discovered_relay_after_refresh = true;

    run_step(
        "test cross_node_gossip_membership_order_keeps_selection_view_stable",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "cross_node_gossip_membership_order_keeps_selection_view_stable",
        ],
    )?;
    gate.test_cross_node_gossip_membership_order_keeps_selection_view_stable = true;

    run_step(
        "test cross_node_gossip_membership_respects_operator_forced_selection",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "cross_node_gossip_membership_respects_operator_forced_selection",
        ],
    )?;
    gate.test_cross_node_gossip_membership_respects_operator_forced_selection = true;

    run_step(
        "test runtime_membership_unavailable_update_prunes_existing_discovered_relay",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "runtime_membership_unavailable_update_prunes_existing_discovered_relay",
        ],
    )?;
    gate.test_runtime_membership_unavailable_update_prunes_existing_discovered_relay = true;

    run_step(
        "test v2_matrix_a_order_perturbation_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_matrix_a_order_perturbation_consistency",
        ],
    )?;
    gate.test_v2_matrix_a_order_perturbation_consistency = true;

    run_step(
        "test v2_matrix_b_multi_source_conflict_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_matrix_b_multi_source_conflict_consistency",
        ],
    )?;
    gate.test_v2_matrix_b_multi_source_conflict_consistency = true;

    run_step(
        "test v2_matrix_c_weak_network_disturbance_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_matrix_c_weak_network_disturbance_consistency",
        ],
    )?;
    gate.test_v2_matrix_c_weak_network_disturbance_consistency = true;

    run_step(
        "test v2_matrix_d_multi_region_view_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_matrix_d_multi_region_view_consistency",
        ],
    )?;
    gate.test_v2_matrix_d_multi_region_view_consistency = true;

    run_step(
        "test v2_stage2a_large_scale_distributed_adjudication_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_stage2a_large_scale_distributed_adjudication_consistency",
        ],
    )?;
    gate.test_v2_stage2a_large_scale_distributed_adjudication_consistency = true;

    run_step(
        "test v2_stage2b_weak_network_robustness_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_stage2b_weak_network_robustness_consistency",
        ],
    )?;
    gate.test_v2_stage2b_weak_network_robustness_consistency = true;

    run_step(
        "test v2_stage2c_multi_region_real_route_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_stage2c_multi_region_real_route_consistency",
        ],
    )?;
    gate.test_v2_stage2c_multi_region_real_route_consistency = true;

    run_step(
        "test v2_stage3a_convergence_recovery_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_stage3a_convergence_recovery_consistency",
        ],
    )?;
    gate.test_v2_stage3a_convergence_recovery_consistency = true;

    run_step(
        "test v2_stage3b_convergence_time_recovery_budget_consistency",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "v2_stage3b_convergence_time_recovery_budget_consistency",
        ],
    )?;
    gate.test_v2_stage3b_convergence_time_recovery_budget_consistency = true;

    run_step(
        "test relay_path_tests",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--bin",
            "novovm-node",
            "relay_path_tests",
        ],
    )?;
    gate.test_relay_path_tests = true;

    run_step(
        "test queue_replay_smoke",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "--test",
            "queue_replay_smoke",
            "queue_replay_smoke",
        ],
    )?;
    gate.test_queue_replay_smoke = true;

    run_step(
        "test eth_end_to_end_geth_sample_batch_parity_report_from_files_v1",
        "cargo",
        &[
            "test",
            "-p",
            "novovm-node",
            "mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1",
        ],
    )?;

    let status = MainlineStatus {
        schema: "supervm-mainline-status/v2",
        generated_utc: Utc::now().to_rfc3339(),
        gate,
        layers: MainlineLayers {
            l1: 100,
            l2: 100,
            l3: 100,
            l4: 100,
            overall: 100,
        },
        lockset: MainlineLockset {
            gate: mainline_preflight::MAINLINE_GATE_LOCKSET_V1,
            source: "bin/supervm-mainline-gate",
        },
    };

    let artifacts_dir = Path::new("artifacts");
    fs::create_dir_all(artifacts_dir).context("failed to create artifacts directory")?;
    let status_path = artifacts_dir.join("mainline-status.json");
    let payload = serde_json::to_string_pretty(&status).context("failed to encode status json")?;
    fs::write(&status_path, format!("{payload}\n")).context("failed to write status json")?;

    let delivery = MainlineDeliveryContract {
        schema: "supervm-mainline-delivery/v1",
        generated_utc: Utc::now().to_rfc3339(),
        milestone: "M23",
        gate_entry: "cargo run -p novovm-node --bin supervm-mainline-gate",
        status_source: "artifacts/mainline-status.json",
        overall_gate: status.layers.overall,
        debt: 0,
        lockset: MainlineDeliveryLockset {
            contract_version: "m23-delivery-contract/v1",
            fieldset_version: "milestone+gate+debt:v1",
            fieldset_fingerprint: "m23:milestone-gate-debt-canonical",
        },
    };
    let delivery_path = artifacts_dir.join("mainline-delivery-contract.json");
    let delivery_payload =
        serde_json::to_string_pretty(&delivery).context("failed to encode delivery contract")?;
    fs::write(&delivery_path, format!("{delivery_payload}\n"))
        .context("failed to write delivery contract json")?;

    println!("==> preflight supervm-mainline-preflight");
    let preflight = mainline_preflight::run_preflight(Path::new("."))?;
    println!(
        "==> mainline preflight passed: status={} delivery={} age={}s max_age={}s",
        preflight.status_path.display(),
        preflight.delivery_path.display(),
        preflight.age_seconds,
        preflight.max_age_seconds
    );

    println!("==> supervm mainline gate passed");
    println!("==> status: {}", status_path.display());
    println!("==> delivery: {}", delivery_path.display());
    println!("==> progress: L1=100% L2=100% L3=100% L4=100% Overall=100%");

    Ok(())
}
