use anyhow::Result;
use novovm_network::product_relay_launch::run_product_relay_cli_v1;
use novovm_node::product_relay_daemon::{
    load_product_relay_daemon_config_v1, run_product_relay_daemon_v1,
};

fn main() -> Result<()> {
    run_product_relay_cli_v1(std::env::args_os().skip(1), |path| {
        let config = load_product_relay_daemon_config_v1(path)?;
        run_product_relay_daemon_v1(config)
    })
}
