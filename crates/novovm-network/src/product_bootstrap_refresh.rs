//! Bounded source refresh over the existing V2 prelude and approved Iroh
//! transport. A mirror is a distribution location, never a new trust anchor.
use crate::{
    carrier_stream::{is_carrier_io_failure, CarrierStreamV1},
    iroh_transport::{IrohAddressV1, IrohConnectUnavailableV1, IrohScopeV1},
    product_service_lease::{request_service_offer_v2, IrohServiceEndpointV1, ServiceLeaseBookV1},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Explicit local invitation permission, separate from short service grants.
/// A platform persists it with its original trust policy, never learns it from
/// a mirror response. Expiration requires a new explicit invitation workflow.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapRefreshPeerV1 {
    pub address: IrohAddressV1,
    pub expires_at_ms: u64,
}

pub fn validate_bootstrap_refresh_peers_v1(
    peers: &[BootstrapRefreshPeerV1],
    relays: &[String],
    now: u64,
    restoring: bool,
) -> Result<()> {
    ensure!(peers.len() <= 2, "bootstrap mirror budget");
    let mut keys = BTreeSet::new();
    for peer in peers {
        ensure!(
            keys.insert(peer.address.endpoint_key),
            "duplicate bootstrap mirror"
        );
        // Reuse the carrier's canonical strong-key/address/resource checks.
        IrohServiceEndpointV1 {
            instance: [1; 32],
            address: peer.address.clone(),
        }
        .endpoint()?;
        ensure!(
            peer.address.relays.iter().all(|r| relays.contains(r)),
            "unapproved bootstrap relay"
        );
        ensure!(peer.expires_at_ms > 0, "empty bootstrap permission");
        if !restoring {
            ensure!(
                peer.expires_at_ms > now && peer.expires_at_ms - now <= 86_400_000,
                "bootstrap permission must expire within 24 hours"
            );
        }
    }
    Ok(())
}

/// Fixed local diagnostics; never an address, identity, source body or raw error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapRefreshEventV1 {
    Connecting,
    Reading,
    Validating,
    Finishing,
    Retry,
    ConnectTimeout,
    ConnectUnavailable,
    ReadTimeout,
    ReadUnavailable,
    PermissionExpired,
    Rejected,
    Complete,
}

/// Compatibility entry point; callers needing diagnostics use the same owner.
pub async fn refresh_bootstrap_sources_v1(
    scope: &IrohScopeV1,
    peers: &[BootstrapRefreshPeerV1],
    book: &Mutex<ServiceLeaseBookV1>,
    now: impl Fn() -> Result<u64>,
) -> Result<()> {
    refresh_bootstrap_sources_observed_v1(scope, peers, book, now, |_| {}).await
}

/// Cold connect and V2 response have separate budgets, within one 12s refresh
/// window AND the original carrier deadline. At most two passes/two peers,
/// one connection at a time. Only transport unavailability permits another
/// attempt. Invalid protocol, permission, signature or storage is terminal;
/// a retry never consumes old routes or changes the original trust policy.
pub async fn refresh_bootstrap_sources_observed_v1(
    scope: &IrohScopeV1,
    peers: &[BootstrapRefreshPeerV1],
    book: &Mutex<ServiceLeaseBookV1>,
    now: impl Fn() -> Result<u64>,
    observe: impl Fn(BootstrapRefreshEventV1),
) -> Result<()> {
    use BootstrapRefreshEventV1::*;
    ensure!((1..=2).contains(&peers.len()), "bootstrap mirror budget");
    let until = Instant::now() + Duration::from_secs(12);
    let mut last = PermissionExpired;
    for pass in 0..2 {
        if pass > 0 {
            scope.check_authority()?;
            if Instant::now() >= until {
                break;
            }
            observe(Retry);
            // Small bounded jitter; no new scope, endpoint, task or deadline.
            tokio::time::sleep_until(
                until
                    .min(
                        Instant::now()
                            + Duration::from_millis(150 + u64::from(rand::random::<u8>())),
                    )
                    .into(),
            )
            .await;
        }
        for peer in peers {
            scope.check_authority()?;
            if Instant::now() >= until {
                break;
            }
            if peer.expires_at_ms <= now()? {
                last = PermissionExpired;
                continue;
            }
            observe(Connecting);
            let connect_until = until.min(Instant::now() + Duration::from_secs(5));
            let connected =
                match tokio::time::timeout_at(connect_until.into(), scope.connect(&peer.address))
                    .await
                {
                    Err(_) => {
                        last = ConnectTimeout;
                        observe(last);
                        continue;
                    }
                    Ok(Err(error))
                        if error.downcast_ref::<IrohConnectUnavailableV1>().is_some() =>
                    {
                        last = ConnectUnavailable;
                        observe(last);
                        continue;
                    }
                    Ok(Err(error)) => {
                        observe(Rejected);
                        return Err(error).context("bootstrap connect rejected");
                    }
                    Ok(Ok(stream)) => stream,
                };
            let mut stream: CarrierStreamV1<'_> = connected.into();
            observe(Reading);
            let read_until = until.min(Instant::now() + Duration::from_secs(3));
            let offer = match tokio::time::timeout_at(
                read_until.into(),
                request_service_offer_v2(&mut stream),
            )
            .await
            {
                Err(_) => {
                    last = ReadTimeout;
                    observe(last);
                    continue;
                }
                Ok(Err(error)) if is_carrier_io_failure(&error) => {
                    last = ReadUnavailable;
                    observe(last);
                    continue;
                }
                Ok(Err(error)) => {
                    observe(Rejected);
                    return Err(error).context("bootstrap protocol rejected");
                }
                Ok(Ok(offer)) => offer,
            };
            scope.check_authority()?;
            if peer.expires_at_ms <= now()? {
                observe(PermissionExpired);
                anyhow::bail!("bootstrap permission expired in flight");
            }
            observe(Validating);
            // Caller code can read this book: call it BEFORE taking the lock.
            let observed = now()?;
            let accepted = (|| {
                ensure!(
                    offer.lease.is_none(),
                    "source mirror cannot issue online service leases"
                );
                book.lock()
                    .map_err(|_| anyhow::anyhow!("bootstrap owner unavailable"))?
                    .accept_bootstrap_sources(offer.sources, observed)
            })();
            if let Err(error) = accepted {
                observe(Rejected);
                return Err(error).context("bootstrap source rejected");
            }
            // Revocations are already durable. Never redo validation or fall
            // back after this commit, even if the final transport ACK is lost.
            observe(Finishing);
            tokio::time::timeout_at(
                until.min(Instant::now() + Duration::from_secs(3)).into(),
                stream.finish(),
            )
            .await
            .context("bootstrap finish timeout")??;
            scope.check_authority()?;
            ensure!(
                peer.expires_at_ms > now()?,
                "bootstrap permission expired at completion"
            );
            observe(Complete);
            return Ok(());
        }
    }
    observe(last);
    anyhow::bail!("approved bootstrap mirrors unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bootstrap_mirror_permission_is_bounded_pinned_and_not_implicitly_renewed() -> Result<()> {
        let peer = BootstrapRefreshPeerV1 {
            address: IrohAddressV1 {
                endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[11; 32])
                    .verifying_key()
                    .to_bytes(),
                direct: vec!["127.0.0.1:9".parse()?],
                relays: vec![],
            },
            expires_at_ms: 5000,
        };
        validate_bootstrap_refresh_peers_v1(std::slice::from_ref(&peer), &[], 1000, false)?;
        assert!(validate_bootstrap_refresh_peers_v1(
            &[peer.clone(), peer.clone()],
            &[],
            1000,
            false
        )
        .is_err());
        assert!(
            validate_bootstrap_refresh_peers_v1(std::slice::from_ref(&peer), &[], 6000, false)
                .is_err()
        );
        validate_bootstrap_refresh_peers_v1(std::slice::from_ref(&peer), &[], 6000, true)?;
        let mut bad = peer.clone();
        bad.expires_at_ms = 86_402_000;
        assert!(validate_bootstrap_refresh_peers_v1(&[bad], &[], 1000, false).is_err());
        let mut bad = peer.clone();
        bad.address.endpoint_key = [0; 32];
        assert!(validate_bootstrap_refresh_peers_v1(&[bad], &[], 1000, true).is_err());
        let mut bad = peer;
        bad.address
            .relays
            .push("https://unapproved.example.invalid/".into());
        assert!(validate_bootstrap_refresh_peers_v1(&[bad], &[], 1000, true).is_err());
        Ok(())
    }
}
