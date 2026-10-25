//! Operator consumer of the existing native SDK, never a node or executor.
use anyhow::{ensure, Context, Result};
use novovm_node::native_pipeline::service::client::operator;
use std::{env, path::Path};

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    ensure!((2..=3).contains(&args.len()),
        "usage: novovm-native-client <client-config.json> status | <query|submit> <request.nvquery|signed-batch.nuapfl>");
    let action = args[1].to_str().context("non-UTF8 action")?;
    let report = operator::run(Path::new(&args[0]), action, args.get(2).map(Path::new))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
