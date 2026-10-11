//! Relay deployment without consensus or execution engines. Both executable
//! entrypoints share an explicit runtime selector; the historical JSON backend
//! below remains available for compatibility, never as a duplex fallback.
#[allow(dead_code)]
#[path = "../../novovm-node/src/product_relay_client.rs"]
mod product_relay_client;
#[allow(dead_code)]
#[path = "../../novovm-node/src/product_relay_daemon.rs"]
mod product_relay_daemon;
#[allow(dead_code)]
#[path = "../../novovm-node/src/product_relay_io.rs"]
mod product_relay_io;

use anyhow::Result;
use novovm_network::product_relay_launch::run_product_relay_cli_v1;

fn main() -> Result<()> {
    run_product_relay_cli_v1(std::env::args_os().skip(1), |path| {
        let config = product_relay_daemon::load_product_relay_daemon_config_v1(path)?;
        product_relay_daemon::run_product_relay_daemon_v1(config)
    })
}
