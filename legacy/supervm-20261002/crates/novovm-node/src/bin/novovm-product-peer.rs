use anyhow::{Context, Result};
use novovm_node::product_peer_runtime::{
    load_product_peer_runtime_config_v1, run_product_peer_runtime_v1,
};
use std::env;

fn main() -> Result<()> {
    let path = env::args()
        .nth(1)
        .context("usage: novovm-product-peer <peer-config.json>")?;
    let report = run_product_peer_runtime_v1(load_product_peer_runtime_config_v1(path)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
