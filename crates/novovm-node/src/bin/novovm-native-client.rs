//! Operator consumer of the existing native SDK, never a node or executor.
use anyhow::{ensure, Context, Result};
use novovm_node::native_pipeline::service::client::operator;
use std::{env, path::Path};

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    const USAGE: &str = "usage: novovm-native-client <client-config.json> status | query <request.nvquery> | submit <signed-batch.nuapfl> | submit-registration <signed.nuir> | submit-application <signed.nuap> | consensus-proposal <height> <round> <tx-hash> [--legacy] | activation-anchor <height> <output> | verified-receipt <trusted-parent.json> <tx-hash>\n       novovm-native-client <installed-discovery-profile.json> discover [profile-sha256] | discover-verified-receipt <tx-hash> [profile-sha256 [--existing-identity]] [--height N | --lookup-height] | check-installed-profile <profile-sha256> | sync-chain | sync-state | follow-state [profile-sha256] | follow-state <profile-sha256> --public-read-max-state-nodes <limit> [absolute-stop-file] | activation-parent <height> <profile-sha256> <output> | verify-uca-execution <profile-sha256> <archive.bin> | serve-read-snapshot or serve-public-read-snapshot <profile-sha256> <height> <max-state-nodes> <serve-seconds> [verify-seconds=300] | run-public-read-snapshot <profile-sha256> <height> <max-state-nodes> [absolute-stop-file]\n       novovm-native-client <bootstrap-config.json> run-discovery-bootstrap <config-sha256>";
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
    if action == "run-discovery-bootstrap" {
        ensure!(args.len() == 3, "bootstrap config and SHA-256 required");
        #[cfg(all(any(windows, target_os = "linux"), feature = "iroh-transport"))]
        return novovm_node::native_pipeline::service::network::run_discovery_bootstrap(
            config,
            args[2].to_str().context("bootstrap configuration SHA-256")?,
        );
        #[cfg(not(all(any(windows, target_os = "linux"), feature = "iroh-transport")))]
        anyhow::bail!("discovery bootstrap requires protected Windows or Linux custody and the NOVONET carrier");
    }
    if action == "run-public-read-snapshot" {
        ensure!((5..=6).contains(&args.len()), "{USAGE}");
        #[cfg(feature = "iroh-transport")]
        return novovm_node::native_pipeline::service::network::run_installed_public_read_snapshot(
            config,
            args[2].to_str().context("profile SHA-256")?,
            args[3].to_str().context("snapshot height")?.parse()?,
            args[4].to_str().context("snapshot node budget")?.parse()?,
            args.get(5).map(Path::new),
        );
        #[cfg(not(feature = "iroh-transport"))]
        anyhow::bail!("read snapshot serving requires the NOVONET carrier feature");
    }
    if matches!(action, "serve-read-snapshot" | "serve-public-read-snapshot") {
        ensure!((6..=7).contains(&args.len()), "{USAGE}");
        #[cfg(feature = "iroh-transport")]
        {
            let pin = args[2].to_str().context("profile SHA-256")?;
            let height = args[3].to_str().context("snapshot height")?.parse()?;
            let max_nodes = args[4].to_str().context("snapshot node budget")?.parse()?;
            let seconds = args[5]
                .to_str()
                .context("snapshot serving duration")?
                .parse()?;
            let verify_seconds = args
                .get(6)
                .map(|arg| {
                    arg.to_str()
                        .context("snapshot verification duration")?
                        .parse::<u64>()
                        .context("snapshot verification duration")
                })
                .transpose()?
                .unwrap_or(300);
            use novovm_node::native_pipeline::service::network::{
                serve_installed_public_read_snapshot, serve_installed_read_snapshot,
            };
            let serve = if action == "serve-public-read-snapshot" {
                serve_installed_public_read_snapshot
            } else {
                serve_installed_read_snapshot
            };
            return serve(config, pin, height, max_nodes, seconds, verify_seconds);
        }
        #[cfg(not(feature = "iroh-transport"))]
        anyhow::bail!("read snapshot serving requires the NOVONET carrier feature");
    }
    if action == "follow-state" {
        if matches!(args.len(), 5 | 6) && args[3] == "--public-read-max-state-nodes" {
            #[cfg(feature = "iroh-transport")]
            return novovm_node::native_pipeline::service::network::follow_and_serve_installed_state(
                config,
                args[2].to_str().context("profile SHA-256")?,
                args[4].to_str().context("snapshot node budget")?.parse()?,
                args.get(5).map(Path::new),
            );
            #[cfg(not(feature = "iroh-transport"))]
            anyhow::bail!("public read serving requires the NOVONET carrier feature");
        }
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
        ensure!(args.len() <= 3, "{USAGE}");
        discover(
            config,
            args.get(2)
                .map(|arg| arg.to_str().context("non-UTF8 profile pin"))
                .transpose()?,
        )?
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
        ensure!((3..=7).contains(&args.len()), "{USAGE}");
        discovered_receipt(
            config,
            args[2].to_str().context("non-UTF8 transaction hash")?,
            &args[3..],
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
fn discover(config: &Path, pin: Option<&str>) -> Result<serde_json::Value> {
    novovm_node::native_pipeline::service::network::discover_installed_from_profile(config, pin)
}

#[cfg(not(feature = "iroh-transport"))]
fn discover(_config: &Path, _pin: Option<&str>) -> Result<serde_json::Value> {
    anyhow::bail!("installed discovery requires the NOVONET carrier feature")
}

#[cfg(feature = "iroh-transport")]
fn discovered_receipt(
    config: &Path,
    hash: &str,
    options: &[std::ffi::OsString],
) -> Result<serde_json::Value> {
    use novovm_node::native_pipeline::service::network::{
        discover_existing_verified_receipt_at, discover_verified_receipt_at, ReceiptLocation,
    };
    let options = options
        .iter()
        .map(|arg| arg.to_str().context("non-UTF8 receipt option"))
        .collect::<Result<Vec<_>>>()?;
    let (pin, selection) = match options.as_slice() {
        [pin, remaining @ ..] if !pin.starts_with('-') => (Some(*pin), remaining),
        remaining => (None, remaining),
    };
    let existing = selection.strip_prefix(&["--existing-identity"]);
    let location = match existing.unwrap_or(selection) {
        [] => ReceiptLocation::AnchorSuccessor,
        ["--lookup-height"] => ReceiptLocation::QueryByHash,
        ["--height", value] => {
            let height = value
                .parse::<std::num::NonZeroU64>()
                .context("receipt height must be a positive integer")?;
            ReceiptLocation::Height(height.get())
        }
        _ => anyhow::bail!(
            "expected [profile-sha256 [--existing-identity]] [--height N | --lookup-height]"
        ),
    };
    if existing.is_some() {
        discover_existing_verified_receipt_at(
            config,
            hash,
            pin.context("existing identity requires a pinned profile")?,
            location,
        )
    } else {
        discover_verified_receipt_at(config, hash, pin, location)
    }
}

#[cfg(not(feature = "iroh-transport"))]
fn discovered_receipt(
    _config: &Path,
    _hash: &str,
    _options: &[std::ffi::OsString],
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
