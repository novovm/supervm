use anyhow::{ensure, Context, Result};
use novovm_node::product_nat_runtime::{
    load_product_nat_runtime_config_v1, prepare_product_nat_runtime_v1,
};
use std::{
    env,
    io::{Read, Write},
    sync::mpsc,
    time::Duration,
};

fn read_start(mut input: impl Read) -> Result<()> {
    let mut command = [0];
    input
        .read_exact(&mut command)
        .context("NAT start input closed")?;
    ensure!(command == [1], "invalid NAT start command");
    Ok(())
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let config_path = args
        .next()
        .context("usage: novovm-product-nat <nat-config.json> [--wait-for-start]")?;
    let wait_for_start = match args.next().as_deref() {
        None => false,
        Some("--wait-for-start") => true,
        Some(_) => anyhow::bail!("unknown NAT qualification argument"),
    };
    ensure!(
        args.next().is_none(),
        "unexpected NAT qualification argument"
    );
    let prepared =
        prepare_product_nat_runtime_v1(load_product_nat_runtime_config_v1(config_path)?)?;
    if wait_for_start {
        // Explicit local orchestration only. No socket exists yet; no peer or
        // payload can trigger this. Default CLI invocations never wait on stdin.
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = sender.send(read_start(std::io::stdin().lock()));
        });
        println!("NAT_PREPARED_V1");
        std::io::stdout().flush()?;
        receiver
            .recv_timeout(Duration::from_secs(25))
            .context("NAT prepared process start deadline exceeded")??;
    }
    let report = prepared.run()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn prepared_start_requires_exact_command_and_rejects_closed_input() {
        assert!(super::read_start([1].as_slice()).is_ok());
        assert!(super::read_start([0].as_slice()).is_err());
        assert!(super::read_start([b'1'].as_slice()).is_err());
        assert!(super::read_start([].as_slice()).is_err());
    }
}
