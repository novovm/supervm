//! Foreground/systemd adapter for the same self-owned UDP/TLS/mailbox owner.
use crate::rendezvous_self_owned_owner::{
    CarrierMode, Config, NativeListenerConfig, Owner, TlsListenerConfig,
};
use anyhow::{ensure, Context, Result};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub fn main() -> Result<()> {
    // Set before creating the runtime or any state file, also covering SQLite WAL.
    // SAFETY: process startup is single-threaded and this only narrows permissions.
    unsafe {
        libc::umask(0o077);
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(args.len() >= 2 && args.len() <= 3, "usage: --check|--provision-self-owned|--foreground CONFIG; --set-self-owned-* CONFIG VALUE");
    let path = PathBuf::from(&args[1]);
    let config = Config::load(&path)?;
    crate::linux_owner_files::check_private_root(&config.root)?;
    if args[0] == "--check" {
        ensure!(args.len() == 2, "check arguments");
        return Ok(());
    }
    if args[0] == "--provision-self-owned" {
        ensure!(args.len() == 2, "provision arguments");
        drop(Owner::open(config, true, None)?);
        return Ok(());
    }
    // Only explicit provisioning may create custody. The first real listener
    // activates its journal; a missing journal after activation remains fatal.
    ensure!(
        config.root.join("self-owned-policy.sealed").is_file(),
        "explicit self-owned provisioning required"
    );
    let owner = Owner::open(config, true, None)?;
    if args[0] == "--foreground" {
        ensure!(args.len() == 2, "foreground arguments");
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async move {
                let stop = Arc::new(AtomicBool::new(false));
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
                let mut interrupt =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
                let run = owner.run(stop.clone());
                tokio::pin!(run);
                tokio::select! {
                    result = &mut run => result,
                    _ = term.recv() => { stop.store(true, Ordering::Release); run.await },
                    _ = interrupt.recv() => { stop.store(true, Ordering::Release); run.await },
                }
            });
    }
    ensure!(args.len() == 3, "configuration value required");
    match args[0].to_str().context("command encoding")? {
        "--set-self-owned-storage" => owner.set_storage_limit_mib(if args[2] == "unlimited" {
            None
        } else {
            Some(
                args[2]
                    .to_str()
                    .context("storage limit encoding")?
                    .parse()?,
            )
        }),
        "--set-self-owned-carrier" => owner.set_carrier_mode(match args[2].to_str() {
            Some("automatic") => CarrierMode::Automatic,
            Some("relay-only") => CarrierMode::RelayOnly,
            Some("direct-only") => CarrierMode::DirectOnly,
            _ => anyhow::bail!("unknown carrier mode"),
        }),
        "--set-self-owned-native-carrier" => match args[2].to_str() {
            Some("off") => owner.set_native_carrier_mailbox_v7(false),
            Some("on" | "v7") => owner.set_native_carrier_mailbox_v7(true),
            Some("v8") => owner.set_native_carrier_mailbox_v8(true),
            _ => anyhow::bail!("native carrier selection must be on, off, v7 or v8"),
        },
        "--set-self-owned-native" => owner.set_native_listener(if args[2] == "off" {
            None
        } else {
            Some(NativeListenerConfig::load(&PathBuf::from(&args[2]))?)
        }),
        "--set-self-owned-tls" => owner.set_tls_listener(if args[2] == "off" {
            None
        } else {
            Some(TlsListenerConfig::load(&PathBuf::from(&args[2]))?)
        }),
        _ => anyhow::bail!("unsupported Linux owner command"),
    }
}
