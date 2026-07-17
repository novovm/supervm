use anyhow::{Context, Result};
use novovm_node::product_relay_daemon::{
    load_product_relay_daemon_config_v1, run_product_relay_daemon_v1,
};
use std::env;

fn main() -> Result<()> {
    let config_path = env::args()
        .nth(1)
        .context("usage: novovm-product-relay <relay-config.json>")?;
    let config = load_product_relay_daemon_config_v1(&config_path)?;
    run_product_relay_daemon_v1(config)
}
