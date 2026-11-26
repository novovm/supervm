//! Operator consumer of the existing native SDK, never a node or executor.
use anyhow::{ensure, Context, Result};
use novovm_node::native_pipeline::service::client::operator;
use std::{env, path::Path};

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    const USAGE: &str = "usage: novovm-native-client <client-config.json> status | query <request.nvquery> | submit <signed-batch.nuapfl> | consensus-proposal <height> <round> <tx-hash> [--legacy] | verified-receipt <trusted-parent.json> <tx-hash>\n       novovm-native-client <installed-discovery-profile.json> discover | discover-verified-receipt <tx-hash> | sync-chain | sync-state | follow-state [profile-sha256]";
    ensure!(args.len() >= 2, "{USAGE}");
    let action = args[1].to_str().context("non-UTF8 action")?;
    if action == "--asas-authority" {
        ensure!(args.len() == 3, "authority profile and digest required");
        return operator::serve_authority(
            Path::new(&args[0]),
            args[2]
                .to_str()
                .context("authority profile digest required")?,
        );
    }
    let config = Path::new(&args[0]);
    if action == "follow-state" {
        ensure!(args.len() <= 3, "{USAGE}");
        #[cfg(feature = "iroh-transport")]
        return novovm_node::native_pipeline::service::network::follow_installed_state(
            config,
            args.get(2)
                .map(|arg| arg.to_str().context("non-UTF8 profile pin"))
                .transpose()?,
        );
        #[cfg(not(feature = "iroh-transport"))]
        anyhow::bail!("installed discovery requires the NOVONET carrier feature");
    }
    let report = if action == "discover" {
        ensure!(args.len() == 2, "{USAGE}");
        discover(config)?
    } else if matches!(action, "sync-chain" | "sync-state") {
        ensure!(args.len() == 2, "{USAGE}");
        sync_chain(config, action == "sync-state")?
    } else if action == "discover-verified-receipt" {
        ensure!(args.len() == 3, "{USAGE}");
        discovered_receipt(
            config,
            args[2].to_str().context("non-UTF8 transaction hash")?,
        )?
    } else if action == "verified-receipt" {
        ensure!(args.len() == 4, "{USAGE}");
        operator::verified_receipt(
            config,
            Path::new(&args[2]),
            args[3].to_str().context("non-UTF8 transaction hash")?,
        )?
    } else if action == "consensus-proposal" {
        ensure!(
            args.len() == 5 || args.len() == 6 && args[5] == "--legacy",
            "{USAGE}"
        );
        let height = args[2].to_str().context("non-UTF8 height")?.parse()?;
        let round = args[3].to_str().context("non-UTF8 round")?.parse()?;
        let hash = args[4].to_str().context("non-UTF8 transaction hash")?;
        if args.len() == 6 {
            operator::consensus_proposal_legacy(config, height, round, hash)?
        } else {
            operator::consensus_proposal(config, height, round, hash)?
        }
    } else {
        ensure!((2..=3).contains(&args.len()), "{USAGE}");
        operator::run(config, action, args.get(2).map(Path::new))?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(feature = "iroh-transport")]
fn discover(config: &Path) -> Result<serde_json::Value> {
    novovm_node::native_pipeline::service::network::discover_installed(config)
}

#[cfg(not(feature = "iroh-transport"))]
fn discover(_config: &Path) -> Result<serde_json::Value> {
    anyhow::bail!("installed discovery requires the NOVONET carrier feature")
}

#[cfg(feature = "iroh-transport")]
fn discovered_receipt(config: &Path, hash: &str) -> Result<serde_json::Value> {
    novovm_node::native_pipeline::service::network::discover_verified_receipt(config, hash)
}

#[cfg(not(feature = "iroh-transport"))]
fn discovered_receipt(_config: &Path, _hash: &str) -> Result<serde_json::Value> {
    anyhow::bail!("installed discovery requires the NOVONET carrier feature")
}

#[cfg(feature = "iroh-transport")]
fn sync_chain(config: &Path, with_state: bool) -> Result<serde_json::Value> {
    use novovm_node::native_pipeline::service::network::{
        sync_installed_chain, sync_installed_state,
    };
    if with_state {
        sync_installed_state(config)
    } else {
        sync_installed_chain(config)
    }
}

#[cfg(not(feature = "iroh-transport"))]
fn sync_chain(_config: &Path, _with_state: bool) -> Result<serde_json::Value> {
    anyhow::bail!("installed discovery requires the NOVONET carrier feature")
}
