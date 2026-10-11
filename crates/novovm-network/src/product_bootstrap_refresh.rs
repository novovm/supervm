//! Bounded source refresh over the existing V2 prelude and approved Iroh
//! transport. A mirror is a distribution location, never a new trust anchor.
use crate::{
    carrier_stream::{is_carrier_io_failure, CarrierStreamV1},
    iroh_transport::{IrohAddressV1, IrohScopeV1},
    product_service_lease::{request_service_offer_v2, IrohServiceEndpointV1, ServiceLeaseBookV1},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Mutex, time::Duration};

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

/// Caller authorizes the scope BEFORE bind/DNS, and keeps checking identity,
/// contact policy, network generation, cancellation and the original deadline.
/// Only constant V2 requests leave the client: no contacts/account identifiers.
/// An unavailable mirror may select another approved peer. Invalid supplied
/// protocol/signatures/rollback/revocation never fall back to cached routes.
pub async fn refresh_bootstrap_sources_v1(
    scope: &IrohScopeV1,
    peers: &[BootstrapRefreshPeerV1],
    book: &Mutex<ServiceLeaseBookV1>,
    now: impl Fn() -> Result<u64>,
) -> Result<()> {
    ensure!((1..=2).contains(&peers.len()), "bootstrap mirror budget");
    for peer in peers {
        if peer.expires_at_ms <= now()? {
            continue;
        }
        let response = tokio::time::timeout(Duration::from_secs(3), async {
            // A connect failure is unavailable, never a successful admission.
            let connected = match scope.connect(&peer.address).await {
                Ok(value) => value,
                Err(_) => return Ok(None),
            };
            let mut stream: CarrierStreamV1<'_> = connected.into();
            let offer = match request_service_offer_v2(&mut stream).await {
                Ok(offer) => offer,
                Err(error) if is_carrier_io_failure(&error) => return Ok(None),
                Err(error) => return Err(error),
            };
            ensure!(
                peer.expires_at_ms > now()?,
                "bootstrap permission expired in flight"
            );
            ensure!(
                offer.lease.is_none(),
                "source mirror cannot issue online service leases"
            );
            // Persist before any consumer sees refreshed addresses. Preserve a
            // valid revocation even when the refreshed bundle has zero routes.
            // Authorization callbacks may read this same book. Never invoke
            // caller code while holding its state lock.
            let observed = now()?;
            book.lock()
                .map_err(|_| anyhow::anyhow!("bootstrap owner unavailable"))?
                .accept_bootstrap_sources(offer.sources, observed)?;
            // The shared carrier requires BOTH ends to finish. Dropping early
            // would reset a distributor that is flushing its response.
            stream.finish().await?;
            Ok(Some(()))
        })
        .await;
        match response {
            Ok(Ok(Some(()))) => return Ok(()),
            Ok(Err(error)) => return Err(error).context("bootstrap refresh rejected"),
            Ok(Ok(None)) | Err(_) => {}
        }
    }
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
