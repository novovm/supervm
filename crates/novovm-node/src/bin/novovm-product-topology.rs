use anyhow::{bail, Context, Result};
use novovm_node::product_mainline_topology::verify_product_mainline_topology_plan_v1;
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let plan_path = PathBuf::from(
        args.next()
            .context("usage: novovm-product-topology <topology-plan.json>")?,
    );
    if args.next().is_some() {
        bail!("too many arguments for product topology preflight");
    }
    let report = verify_product_mainline_topology_plan_v1(plan_path);
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.accepted {
        bail!("product topology preflight failed");
    }
    Ok(())
}
