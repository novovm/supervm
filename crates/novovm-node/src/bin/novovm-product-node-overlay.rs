use anyhow::{Context, Result};
use novovm_node::product_node_overlay::{
    load_product_node_overlay_config_v1, ProductNodeOverlayRuntimeV1,
};
use std::{
    env,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() -> Result<()> {
    let mut arguments = env::args().skip(1);
    let config_path = arguments.next().context(
        "usage: novovm-product-node-overlay <config.json> <chain-id> <protected-transport-key> <library> <sha256> <target-peer-id>",
    )?;
    let chain = arguments.next().context("missing chain id")?.parse()?;
    let identity_path = arguments
        .next()
        .context("missing protected transport identity path")?;
    let library = arguments.next().context("missing approved AOEM library")?;
    let sha256 = arguments.next().context("missing approved AOEM SHA256")?;
    let target_peer_id = arguments.next().context("missing target peer id")?;
    if arguments.next().is_some() {
        anyhow::bail!("too many arguments for novovm-product-node-overlay");
    }
    let config = load_product_node_overlay_config_v1(&config_path)?;
    let identity = novovm_relay::transport_identity::load_transport_identity(
        Path::new(&identity_path),
        chain,
        Path::new(&library),
        &sha256,
    )?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let runtime = ProductNodeOverlayRuntimeV1::bootstrap(&config, now_ms, &identity)?;
    let plan = runtime.select_relay_route(
        &identity,
        format!("node-overlay-{}", now_ms),
        &target_peer_id,
        now_ms,
        None,
        false,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
        "accepted": true,
        "scope": "novovm_product_node_overlay_runtime_v1",
        "bootstrap": runtime.bootstrap_status(),
        "route_plan": plan,
            "network_only": true,
            "payload_treated_opaque": true,
            "relay_is_trusted_authority": false,
            "centralized_control_plane_required": false,
            "novorudp_wire_changed": false
        }))?
    );
    Ok(())
}
