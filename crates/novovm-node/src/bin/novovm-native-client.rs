//! Operator consumer of the existing native SDK, never a node or executor.
use anyhow::{ensure, Context, Result};
use novovm_node::native_pipeline::service::client::operator;
use std::{env, path::Path};

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    const USAGE: &str = "usage: novovm-native-client <client-config.json> status | query <request.nvquery> | submit <signed-batch.nuapfl> | submit-registration <signed.nuir> | submit-application <signed.nuap> | consensus-proposal <height> <round> <tx-hash> [--legacy] | activation-anchor <height> <output> | verified-receipt <trusted-parent.json> <tx-hash>\n       novovm-native-client <installed-discovery-profile.json> discover | discover-verified-receipt <tx-hash> [profile-sha256] | check-installed-profile <profile-sha256> | sync-chain | sync-state | follow-state [profile-sha256] | activation-parent <height> <profile-sha256> <output> | verify-uca-execution <profile-sha256> <archive.bin>";
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
    } else if action == "check-installed-profile" {
        ensure!(args.len() == 3, "{USAGE}");
        #[cfg(feature = "iroh-transport")]
        {
            novovm_node::native_pipeline::service::network::check_installed_profile(
                config,
                args[2].to_str().context("non-UTF8 profile pin")?,
            )?
        }
        #[cfg(not(feature = "iroh-transport"))]
        anyhow::bail!("installed discovery requires the NOVONET carrier feature");
    } else if matches!(action, "sync-chain" | "sync-state") {
        ensure!(args.len() == 2, "{USAGE}");
        sync_chain(config, action == "sync-state")?
    } else if action == "verify-uca-execution" {
        ensure!(args.len() == 4, "{USAGE}");
        #[cfg(feature = "iroh-transport")]
        {
            novovm_node::native_pipeline::service::network::verify_installed_uca_execution(
                config,
                args[2].to_str().context("non-UTF8 profile pin")?,
                Path::new(&args[3]),
            )?
        }
        #[cfg(not(feature = "iroh-transport"))]
        anyhow::bail!("UCA execution replay requires installed chain verification");
    } else if action == "discover-verified-receipt" {
        ensure!((3..=4).contains(&args.len()), "{USAGE}");
        discovered_receipt(
            config,
            args[2].to_str().context("non-UTF8 transaction hash")?,
            args.get(3)
                .map(|arg| arg.to_str().context("non-UTF8 profile pin"))
                .transpose()?,
        )?
    } else if action == "activation-parent" {
        ensure!(args.len() == 5, "{USAGE}");
        #[cfg(feature = "iroh-transport")]
        {
            novovm_node::native_pipeline::service::network::export_activation_parent(
                config,
                args[3].to_str().context("non-UTF8 profile pin")?,
                args[2]
                    .to_str()
                    .context("non-UTF8 parent height")?
                    .parse()?,
                Path::new(&args[4]),
            )?
        }
        #[cfg(not(feature = "iroh-transport"))]
        anyhow::bail!("activation parent export requires installed chain verification");
    } else if action == "activation-anchor" {
        ensure!(args.len() == 4, "{USAGE}");
        let height = args[2]
            .to_str()
            .context("non-UTF8 anchor height")?
            .parse()?;
        operator::activation_anchor(config, height, Path::new(&args[3]))?
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
fn discovered_receipt(
    config: &Path,
    hash: &str,
    profile_sha256: Option<&str>,
) -> Result<serde_json::Value> {
    novovm_node::native_pipeline::service::network::discover_verified_receipt_from_profile(
        config,
        hash,
        profile_sha256,
    )
}

#[cfg(not(feature = "iroh-transport"))]
fn discovered_receipt(
    _config: &Path,
    _hash: &str,
    _profile_sha256: Option<&str>,
) -> Result<serde_json::Value> {
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
