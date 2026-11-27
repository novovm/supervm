//! Explicit read-only serving using the installed Client identity.
mod local_locator;
mod publication;
use super::{
    bind_carrier, client_identity, fingerprint, installed_state_base, profile_base,
    read_installed_profile, state_path, wall_ms,
};
use crate::native_pipeline::service::network::{
    query_carrier::{self, Ingress, QuerySlot, Read},
    CARRIER_CONNECTION_CEILING,
};
use anyhow::{ensure, Context, Result};
use local_locator::LocalLocator;
use novovm_network::{
    carrier_stream::CarrierStreamV1,
    discovery_control::native_locator_id_v1,
    iroh_transport::{IrohConfigV1, IrohScopeV1},
    product_directory::{sign_relay_record_v1, validate_relay_record_v1, PeerSignedRelayRecordV1},
    product_overlay::TransportIdentity,
    product_service_lease::IrohServiceEndpointV1,
    service_concurrency::ServiceConcurrencyV1,
};
use std::{
    future::{poll_fn, Future},
    io::{self, Write},
    net::Ipv4Addr,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{mpsc, Arc},
    task::Poll,
    time::{Duration, Instant},
};

pub fn serve_installed_read_snapshot(
    profile_path: &Path,
    profile_sha256: &str,
    height: u64,
    max_state_nodes: NonZeroUsize,
    seconds: u64,
    verify_seconds: u64,
) -> Result<()> {
    serve_snapshot(
        profile_path,
        profile_sha256,
        height,
        max_state_nodes,
        SnapshotService::Loopback(seconds),
        verify_seconds,
    )
}

pub fn serve_installed_public_read_snapshot(
    profile_path: &Path,
    profile_sha256: &str,
    height: u64,
    max_state_nodes: NonZeroUsize,
    seconds: u64,
    verify_seconds: u64,
) -> Result<()> {
    serve_snapshot(
        profile_path,
        profile_sha256,
        height,
        max_state_nodes,
        SnapshotService::TimedPublic(seconds),
        verify_seconds,
    )
}

/// Serve an explicitly selected verified cache until stopped or its checkpoint expires.
pub fn run_installed_public_read_snapshot(
    profile_path: &Path,
    profile_sha256: &str,
    height: u64,
    max_state_nodes: NonZeroUsize,
    stop_file: Option<&Path>,
) -> Result<()> {
    serve_snapshot(
        profile_path,
        profile_sha256,
        height,
        max_state_nodes,
        SnapshotService::ResidentPublic { stop_file },
        300,
    )
}

enum SnapshotService<'a> {
    Loopback(u64),
    TimedPublic(u64),
    ResidentPublic { stop_file: Option<&'a Path> },
}

fn serve_snapshot(
    profile_path: &Path,
    profile_sha256: &str,
    height: u64,
    max_state_nodes: NonZeroUsize,
    service: SnapshotService<'_>,
    verify_seconds: u64,
) -> Result<()> {
    let (seconds, public, stop_file) = match service {
        SnapshotService::Loopback(seconds) => (Some(seconds), false, None),
        SnapshotService::TimedPublic(seconds) => (Some(seconds), true, None),
        SnapshotService::ResidentPublic { stop_file } => (None, true, stop_file),
    };
    ensure!(
        seconds.is_none_or(|seconds| (1..=120).contains(&seconds)),
        "local snapshot duration must be 1–120 seconds"
    );
    ensure!(
        (1..=86_400).contains(&verify_seconds),
        "snapshot verification duration must be 1-86400 seconds"
    );
    let verify_deadline = Instant::now() + Duration::from_secs(verify_seconds);
    let duration = seconds.map(Duration::from_secs);
    if let Some(path) = stop_file {
        ensure!(
            path.is_absolute(),
            "snapshot stop marker must use an absolute path"
        );
        if stop_requested(path)? {
            return Ok(());
        }
    }
    let data = installed_state_base()?;
    let profile = read_installed_profile(profile_path, Some(profile_sha256))?;
    ensure!(
        !public || profile.public_read_referrals(),
        "public read serving requires verification pins and approved relays"
    );
    let pins = profile.pins();
    let identity = client_identity(&data, profile_base(profile_path)?, &profile, false)?;
    let mut snapshot = super::chain_sync::read_snapshot::ReadSnapshot::open(
        &data,
        profile_path,
        profile_sha256,
        height,
        max_state_nodes,
        verify_deadline,
    )?;
    let locator_path = state_path(&data, &profile).join("read-snapshot.local.json");
    let _public_owner = if public {
        Some(local_locator::lock(&locator_path)?)
    } else {
        None
    };
    let publication = if public {
        Some(publication::Publication::new(
            &profile,
            profile_base(profile_path)?,
            &identity,
            duration.is_none(),
        )?)
    } else {
        None
    };
    let carrier = if public {
        profile.carrier_config()?.with_relay_only()?
    } else {
        IrohConfigV1::new(&[], Ipv4Addr::LOCALHOST.into())?
            .with_bind_address((Ipv4Addr::LOCALHOST, 0).into())?
    }
    .with_service_concurrency(ServiceConcurrencyV1::from_host())
    .with_connection_ceiling(CARRIER_CONNECTION_CEILING)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let mut stop = if duration.is_none() {
                Some(ResidentStop::new(stop_file)?)
            } else {
                None
            };
            let owner = bind_carrier(carrier).await?;
            let (requests, reads) = mpsc::sync_channel::<Read>(1);
            let worker = tokio::task::spawn_blocking(move || {
                while let Ok(read) = reads.recv() {
                    let _ = read.answer.send(snapshot.answer(&read.request.query));
                }
            });
            let ingress = Ingress {
                requests,
                slot: Some(Arc::new(QuerySlot::default())),
                peers: None,
            };
            let served =
                if let Some(publication) = publication {
                    owner.with_relay_only_service_v1(async |scope| {
                    let deadline = duration.map(|duration| Instant::now() + duration);
                    let publish = publication.run(scope, &identity, pins, height, deadline);
                    tokio::select! {
                        biased;
                        result = wait_for_stop(stop.as_mut()) => result,
                        result = publish => result,
                        result = serve(scope, identity.clone(), pins, ingress, None) => result,
                    }
                }).await
                } else {
                    owner
                .with_continuous_service_v4(
                    || Ok(()),
                    |scope| {
                        Box::pin(async move {
                            let duration = duration.context("loopback snapshot duration required")?;
                            let deadline = Instant::now() + duration;
                            let _locator =
                                local_locator(scope, &identity, pins, locator_path, duration)?;
                            writeln!(
                                io::stdout().lock(),
                                "{}",
                                serde_json::json!({
                                    "event": "read_snapshot_ready", "height": height,
                                    "transport_identity_sha256": fingerprint(identity.public_key()),
                                    "publication": "disabled", "scope": "loopback"
                                })
                            )?;
                            serve(scope, identity, pins, ingress, Some(deadline)).await
                        })
                    },
                )
                .await
                };
            let closed = owner.close().await;
            worker.await.context("snapshot query worker failed")?;
            served.and(closed)?;
            if duration.is_none() {
                println!(
                    "{}",
                    serde_json::json!({"event":"public_read_stopped", "height":height})
                );
            }
            Ok(())
        })
}

fn stop_requested(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file(),
                "snapshot stop marker must be a regular file"
            );
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("checking snapshot stop marker"),
    }
}

struct ResidentStop<'a> {
    file: Option<&'a Path>,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(windows)]
    interrupt: tokio::signal::windows::CtrlC,
    #[cfg(windows)]
    terminate: tokio::signal::windows::CtrlBreak,
}

impl<'a> ResidentStop<'a> {
    fn new(file: Option<&'a Path>) -> Result<Self> {
        #[cfg(unix)]
        use tokio::signal::unix::{signal, SignalKind};
        Ok(Self {
            file,
            #[cfg(unix)]
            interrupt: signal(SignalKind::interrupt())?,
            #[cfg(unix)]
            terminate: signal(SignalKind::terminate())?,
            #[cfg(windows)]
            interrupt: tokio::signal::windows::ctrl_c()?,
            #[cfg(windows)]
            terminate: tokio::signal::windows::ctrl_break()?,
        })
    }
}

async fn wait_for_stop(stop: Option<&mut ResidentStop<'_>>) -> Result<()> {
    let Some(stop) = stop else {
        return std::future::pending().await;
    };
    let marker = async {
        let Some(path) = stop.file else {
            return std::future::pending().await;
        };
        while !stop_requested(path)? {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Ok(())
    };
    tokio::select! {
        _ = stop.interrupt.recv() => Ok(()),
        _ = stop.terminate.recv() => Ok(()),
        result = marker => result,
    }
}

fn local_locator(
    scope: &IrohScopeV1,
    identity: &TransportIdentity,
    pins: (u64, [u8; 32], [u8; 32]),
    path: PathBuf,
    duration: Duration,
) -> Result<LocalLocator> {
    let address = scope.address()?;
    ensure!(
        !address.direct.is_empty()
            && address.direct.iter().all(|entry| entry.ip().is_loopback())
            && address.relays.is_empty(),
        "local snapshot has no loopback-only route"
    );
    let now = wall_ms()?;
    let record = sign_relay_record_v1(
        identity,
        native_locator_id_v1(pins),
        vec![IrohServiceEndpointV1 {
            instance: rand::random(),
            address,
        }
        .endpoint()?],
        now,
        now.checked_add(duration.as_millis().try_into()?)
            .context("snapshot expiry overflow")?,
        now,
    )?;
    LocalLocator::replace(path, &serde_json::to_vec(&record)?, |bytes| {
        // Before the lock file existed, a live owner only left a signed locator.
        serde_json::from_slice::<PeerSignedRelayRecordV1>(bytes).is_ok_and(|prior| {
            prior.record_id == record.record_id
                && prior.relay_public_key == identity.public_key()
                && validate_relay_record_v1(&prior, now, identity).is_ok()
        })
    })
}

async fn serve(
    scope: &IrohScopeV1,
    identity: TransportIdentity,
    pins: (u64, [u8; 32], [u8; 32]),
    ingress: Ingress,
    deadline: Option<Instant>,
) -> Result<()> {
    type Session<'scope> = Pin<Box<dyn Future<Output = ()> + 'scope>>;
    let mut sessions: Vec<Session<'_>> = Vec::new();
    loop {
        tokio::select! {
            biased;
            _ = async {
                if let Some(deadline) = deadline {
                    tokio::time::sleep_until(deadline.into()).await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => return Ok(()),
            _ = poll_fn(|cx| {
                let before = sessions.len();
                sessions.retain_mut(|session| session.as_mut().poll(cx).is_pending());
                if before == sessions.len() { Poll::Pending } else { Poll::Ready(()) }
            }) => {},
            stream = scope.accept_inbound(), if sessions.len() < scope.connection_capacity() => {
                let Ok(stream) = stream else {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    continue;
                };
                let mut stream: CarrierStreamV1<'_> = stream.into();
                let identity = identity.clone();
                let ingress = ingress.clone();
                sessions.push(Box::pin(async move {
                    if matches!(tokio::time::timeout(Duration::from_secs(5), stream.read_frame()).await,
                        Ok(Ok(entry)) if entry == query_carrier::ENTRY)
                    {
                        let _ = query_carrier::accept(stream, identity, pins, ingress).await;
                    }
                }));
            }
        }
    }
}
