use anyhow::{Context, Result};
use novovm_node::product_nat_runtime::{
    load_product_nat_runtime_config_v1, run_product_nat_runtime_v1,
};
use std::env;

fn main() -> Result<()> {
    let config_path = env::args()
        .nth(1)
        .context("usage: novovm-product-nat <nat-config.json>")?;
    let report = run_product_nat_runtime_v1(load_product_nat_runtime_config_v1(config_path)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
