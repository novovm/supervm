use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use novovm_node::product_node_overlay::{
    load_product_node_overlay_config_v1, ProductNodeOverlayRuntimeV1,
};
use std::{
    env, fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() -> Result<()> {
    let mut arguments = env::args().skip(1);
    let config_path = arguments.next().context(
        "usage: novovm-product-node-overlay <config.json> <node-ed25519.hex> <target-peer-id>",
    )?;
    let identity_path = arguments
        .next()
        .context("missing node Ed25519 identity key path")?;
    let target_peer_id = arguments.next().context("missing target peer id")?;
    if arguments.next().is_some() {
        anyhow::bail!("too many arguments for novovm-product-node-overlay");
    }
    let config = load_product_node_overlay_config_v1(&config_path)?;
    let identity = load_ed25519_identity(&identity_path)?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let runtime = ProductNodeOverlayRuntimeV1::bootstrap(&config, now_ms)?;
    let plan = runtime.select_relay_route(
        &identity,
        format!("node-overlay-{}", now_ms),
        &target_peer_id,
        now_ms,
        None,
        false,
    );
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

fn load_ed25519_identity(path: &str) -> Result<SigningKey> {
    let text =
        fs::read_to_string(path).with_context(|| format!("read node identity key: {path}"))?;
    let text = text.trim();
    if text.len() != 64 {
        anyhow::bail!("node identity key must be 64 hexadecimal characters");
    }
    let mut secret = [0u8; 32];
    for (index, output) in secret.iter_mut().enumerate() {
        *output = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .context("decode node identity key hex")?;
    }
    Ok(SigningKey::from_bytes(&secret))
}
