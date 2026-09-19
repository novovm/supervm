//! Deployment target for the existing relay, without consensus or execution engines.
//! Keep one implementation: both binaries compile the same source modules.
#[allow(dead_code)]
#[path = "../../novovm-node/src/product_relay_client.rs"]
mod product_relay_client;
#[allow(dead_code)]
#[path = "../../novovm-node/src/product_relay_daemon.rs"]
mod product_relay_daemon;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: supervm-relay <relay-config.json>")?;
    let config = product_relay_daemon::load_product_relay_daemon_config_v1(&path)?;
    product_relay_daemon::run_product_relay_daemon_v1(config)
}
