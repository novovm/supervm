//! Offline full-mesh configuration preflight for the node-owned Product Overlay.
//!
//! This validates deployment intent and peer symmetry. It deliberately does not turn local
//! configuration validation into evidence that any external network path was executed.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::product_mainline_overlay::{
    load_product_mainline_overlay_config_v1, product_mainline_overlay_local_peer_id_v1,
    validate_product_mainline_overlay_config_v1, ProductMainlineOverlayConfigV1,
    ProductMainlineOverlayRoleV1,
};

pub const PRODUCT_MAINLINE_TOPOLOGY_SCOPE_V1: &str = "novovm_product_mainline_topology_plan_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductMainlineTopologyNodeV1 {
    pub name: String,
    pub peer_id: String,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductMainlineTopologyPlanV1 {
    pub scope: String,
    pub chain_id: u64,
    #[serde(default)]
    pub require_identity_files: bool,
    pub nodes: Vec<ProductMainlineTopologyNodeV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductMainlineTopologyPreflightV1 {
    pub accepted: bool,
    pub scope: String,
    pub payload_treated_opaque: bool,
    pub novorudp_wire_changed: bool,
    pub chain_id: u64,
    pub node_count: usize,
    pub directed_peer_edge_count: usize,
    pub full_mesh_symmetric: bool,
    pub identity_file_verification_required: bool,
    pub identity_file_verified_count: usize,
    pub relative_config_path_count: usize,
    pub external_network_executed: bool,
    pub real_public_topology_proven: bool,
    pub real_cross_nat_proven: bool,
    pub reason: Option<String>,
}

pub fn verify_product_mainline_topology_plan_v1(
    plan_path: impl AsRef<Path>,
) -> ProductMainlineTopologyPreflightV1 {
    match verify_product_mainline_topology_plan_inner_v1(plan_path.as_ref()) {
        Ok(report) => report,
        Err(error) => ProductMainlineTopologyPreflightV1 {
            accepted: false,
            scope: PRODUCT_MAINLINE_TOPOLOGY_SCOPE_V1.into(),
            payload_treated_opaque: true,
            novorudp_wire_changed: false,
            chain_id: 0,
            node_count: 0,
            directed_peer_edge_count: 0,
            full_mesh_symmetric: false,
            identity_file_verification_required: false,
            identity_file_verified_count: 0,
            relative_config_path_count: 0,
            external_network_executed: false,
            real_public_topology_proven: false,
            real_cross_nat_proven: false,
            reason: Some(error.to_string()),
        },
    }
}

fn verify_product_mainline_topology_plan_inner_v1(
    plan_path: &Path,
) -> Result<ProductMainlineTopologyPreflightV1> {
    let absolute_plan = fs::canonicalize(plan_path)
        .with_context(|| format!("resolve product topology plan: {}", plan_path.display()))?;
    let plan_bytes = fs::read(&absolute_plan)
        .with_context(|| format!("read product topology plan: {}", absolute_plan.display()))?;
    let plan: ProductMainlineTopologyPlanV1 =
        serde_json::from_slice(&plan_bytes).context("decode product topology plan")?;
    if plan.scope != PRODUCT_MAINLINE_TOPOLOGY_SCOPE_V1 {
        bail!("unsupported product topology plan scope");
    }
    if plan.chain_id == 0 {
        bail!("product topology plan chain_id must be positive");
    }
    if !(2..=64).contains(&plan.nodes.len()) {
        bail!("product topology plan requires between 2 and 64 nodes");
    }
    let plan_dir = absolute_plan
        .parent()
        .context("product topology plan has no parent directory")?;
    let mut names = BTreeSet::new();
    let mut peer_ids = BTreeSet::new();
    for node in &plan.nodes {
        if node.name.is_empty() || node.peer_id.is_empty() {
            bail!("product topology node name and peer_id must not be empty");
        }
        if !names.insert(node.name.as_str()) {
            bail!("product topology node names must be unique");
        }
        if !peer_ids.insert(node.peer_id.as_str()) {
            bail!("product topology node peer IDs must be unique");
        }
    }

    let mut configs = BTreeMap::<String, ProductMainlineOverlayConfigV1>::new();
    let mut identity_file_verified_count = 0usize;
    let mut relative_config_path_count = 0usize;
    for node in &plan.nodes {
        if node.config_path.is_relative() {
            relative_config_path_count = relative_config_path_count.saturating_add(1);
        }
        let config_path = if node.config_path.is_absolute() {
            node.config_path.clone()
        } else {
            plan_dir.join(&node.config_path)
        };
        let config = load_product_mainline_overlay_config_v1(&config_path)
            .with_context(|| format!("load topology config for node {}", node.name))?;
        validate_product_mainline_overlay_config_v1(&config)
            .with_context(|| format!("validate topology config for node {}", node.name))?;
        if config.chain_id != plan.chain_id {
            bail!(
                "topology node {} chain_id {} does not match plan chain_id {}",
                node.name,
                config.chain_id,
                plan.chain_id
            );
        }
        if config.role != ProductMainlineOverlayRoleV1::Duplex || config.peers.is_empty() {
            bail!(
                "topology node {} must use an explicit multi-peer duplex config",
                node.name
            );
        }
        if plan.require_identity_files {
            let actual_peer_id = product_mainline_overlay_local_peer_id_v1(&config)
                .with_context(|| format!("verify topology identity for node {}", node.name))?;
            if actual_peer_id != node.peer_id {
                bail!(
                    "topology node {} identity does not match peer_id",
                    node.name
                );
            }
            identity_file_verified_count = identity_file_verified_count.saturating_add(1);
        }
        configs.insert(node.peer_id.clone(), config);
    }

    let mut directed_peer_edge_count = 0usize;
    for node in &plan.nodes {
        let expected = peer_ids
            .iter()
            .filter(|peer_id| **peer_id != node.peer_id)
            .copied()
            .collect::<BTreeSet<_>>();
        let config = configs
            .get(&node.peer_id)
            .context("topology node config disappeared")?;
        let actual = config
            .peers
            .iter()
            .map(|peer| peer.peer_id.as_str())
            .collect::<BTreeSet<_>>();
        if actual != expected {
            bail!(
                "topology node {} peer set is not the complete symmetric mesh",
                node.name
            );
        }
        directed_peer_edge_count = directed_peer_edge_count.saturating_add(actual.len());
    }

    Ok(ProductMainlineTopologyPreflightV1 {
        accepted: true,
        scope: PRODUCT_MAINLINE_TOPOLOGY_SCOPE_V1.into(),
        payload_treated_opaque: true,
        novorudp_wire_changed: false,
        chain_id: plan.chain_id,
        node_count: plan.nodes.len(),
        directed_peer_edge_count,
        full_mesh_symmetric: true,
        identity_file_verification_required: plan.require_identity_files,
        identity_file_verified_count,
        relative_config_path_count,
        external_network_executed: false,
        real_public_topology_proven: false,
        real_cross_nat_proven: false,
        reason: Some(
            "configuration_full_mesh_verified; external topology remains not executed".into(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn three_node_full_mesh_preflight_is_accepted_without_external_claims() {
        let root = temp_root_v1("accepted");
        fs::create_dir_all(&root).unwrap();
        let peer_ids = ["peer-a", "peer-b", "peer-c"];
        for (index, peer_id) in peer_ids.iter().enumerate() {
            write_node_config_v1(&root, index, peer_id, &peer_ids, false);
        }
        let plan_path = write_plan_v1(&root, &peer_ids);
        let report = verify_product_mainline_topology_plan_v1(&plan_path);
        assert!(report.accepted, "{:?}", report.reason);
        assert_eq!(report.node_count, 3);
        assert_eq!(report.directed_peer_edge_count, 6);
        assert!(report.full_mesh_symmetric);
        assert!(report.payload_treated_opaque);
        assert!(!report.novorudp_wire_changed);
        assert!(!report.external_network_executed);
        assert!(!report.real_public_topology_proven);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn asymmetric_peer_set_is_rejected() {
        let root = temp_root_v1("asymmetric");
        fs::create_dir_all(&root).unwrap();
        let peer_ids = ["peer-a", "peer-b", "peer-c"];
        for (index, peer_id) in peer_ids.iter().enumerate() {
            write_node_config_v1(&root, index, peer_id, &peer_ids, index == 1);
        }
        let plan_path = write_plan_v1(&root, &peer_ids);
        let report = verify_product_mainline_topology_plan_v1(&plan_path);
        assert!(!report.accepted);
        assert!(report
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("complete symmetric mesh")));
        let _ = fs::remove_dir_all(root);
    }

    fn write_plan_v1(root: &Path, peer_ids: &[&str]) -> PathBuf {
        let plan_path = root.join("topology.json");
        let nodes = peer_ids
            .iter()
            .enumerate()
            .map(|(index, peer_id)| {
                json!({
                    "name": format!("node-{index}"),
                    "peer_id": peer_id,
                    "config_path": format!("node-{index}.json")
                })
            })
            .collect::<Vec<_>>();
        fs::write(
            &plan_path,
            serde_json::to_vec_pretty(&json!({
                "scope": PRODUCT_MAINLINE_TOPOLOGY_SCOPE_V1,
                "chain_id": 77,
                "require_identity_files": false,
                "nodes": nodes
            }))
            .unwrap(),
        )
        .unwrap();
        plan_path
    }

    fn write_node_config_v1(
        root: &Path,
        index: usize,
        local_peer_id: &str,
        peer_ids: &[&str],
        omit_last_peer: bool,
    ) {
        let mut peers = peer_ids
            .iter()
            .enumerate()
            .filter(|(_, peer_id)| **peer_id != local_peer_id)
            .map(|(peer_index, peer_id)| {
                json!({
                    "peer_id": peer_id,
                    "metric_peer_id": 10_000 + peer_index as u64
                })
            })
            .collect::<Vec<_>>();
        if omit_last_peer {
            peers.pop();
        }
        fs::write(
            root.join(format!("node-{index}.json")),
            serde_json::to_vec_pretty(&json!({
                "chain_id": 77,
                "role": "duplex",
                "identity_key_path": format!("node-{index}.hex"),
                "peers": peers,
                "overlay": {
                    "cache_path": format!("node-{index}-cache.json"),
                    "trusted_signer_public_keys": [],
                    "embedded_sources": []
                }
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn temp_root_v1(label: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("novovm-product-topology-{label}-{now}"))
    }
}
