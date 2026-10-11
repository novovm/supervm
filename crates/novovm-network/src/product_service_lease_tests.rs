use super::*;
use directory::{BootstrapSourceKindV1, SignedBootstrapManifestV1};
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn address(n: u8) -> IrohServiceEndpointV1 {
    IrohServiceEndpointV1 {
        instance: [4; 32],
        address: IrohAddressV1 {
            endpoint_key: key(n).verifying_key().to_bytes(),
            direct: vec!["127.0.0.1:12345".parse().unwrap()],
            relays: vec![],
        },
    }
}
fn record(n: u8, sequence: u64, issued: u64, expires: u64) -> PeerSignedRelayRecordV1 {
    directory::sign_relay_record_v1(
        &key(n),
        "service",
        vec![address(3).endpoint().unwrap()],
        issued,
        expires,
        sequence,
    )
    .unwrap()
}
fn source(records: Vec<PeerSignedRelayRecordV1>, issued: u64) -> BootstrapSourceV1 {
    let mut manifest = SignedBootstrapManifestV1 {
        version: 1,
        manifest_id: "installation-source".into(),
        issued_at_ms: issued,
        expires_at_ms: 500_000,
        candidate_limit: 2,
        full_raw_ip_directory_embedded: false,
        requires_single_official_relay: false,
        requires_single_official_domain: false,
        relay_records: records,
        signatures: vec![],
    };
    directory::sign_bootstrap_manifest_v1(&mut manifest, &key(1)).unwrap();
    BootstrapSourceV1 {
        source_kind: BootstrapSourceKindV1::FriendInvite,
        priority: 0,
        manifest,
    }
}
fn book() -> ServiceLeaseBookV1 {
    ServiceLeaseBookV1::new(
        BootstrapTrustPolicyV1::single_signer(key(1).verifying_key().to_bytes()),
        vec![source(vec![record(2, 1, 1000, 400_000)], 1000)],
        2000,
    )
    .unwrap()
}
fn accept(book: &mut ServiceLeaseBookV1, value: PeerSignedRelayRecordV1, now: u64) -> Result<()> {
    book.accept_renewal(
        value,
        key(2).verifying_key().to_bytes(),
        key(3).verifying_key().to_bytes(),
        [4; 32],
        now,
    )
}
#[test]
fn signed_invite_admits_without_official_source_or_ip_input() {
    let mut b = book();
    let routes = b.routes(2000).unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(
        IrohServiceEndpointV1::from_record(&routes[0])
            .unwrap()
            .address
            .direct,
        address(3).address.direct
    );
}
#[test]
fn renewal_has_original_authority_boundary_and_monotone_sequence() {
    let mut b = book();
    let issuer = ServiceLeaseIssuerV1::new(key(2), record(2, 1, 1000, 400_000), 2000).unwrap();
    let one = issuer.issue(3000).unwrap();
    let two = issuer.issue(4000).unwrap();
    accept(&mut b, one.clone(), 3000).unwrap();
    accept(&mut b, two.clone(), 4000).unwrap();
    assert!(accept(&mut b, one, 4000).is_err());
    accept(&mut b, two, 4000).unwrap();
    assert!(accept(&mut b, record(2, 9, 390_000, 410_000), 390_001).is_err());
    assert!(issuer.issue(400_000).is_err());
}
#[test]
fn same_sequence_different_lease_is_not_renewal() {
    let mut b = book();
    accept(&mut b, record(2, 2, 2000, 3000), 2000).unwrap();
    assert!(accept(&mut b, record(2, 2, 2000, 3100), 2000).is_err());
}
#[test]
fn invalid_expired_forged_and_wrong_node_are_rejected() {
    let mut b = book();
    for mut bad in [
        record(2, 2, 2000, 3000),
        record(2, 2, 2001, 3000),
        record(2, 2, 1000, 1500),
        record(5, 2, 2000, 3000),
    ] {
        if bad.issued_at_ms == 2000 && bad.relay_public_key == key(2).verifying_key().to_bytes() {
            bad.signature[0] ^= 1;
        }
        assert!(accept(&mut b, bad, 2000).is_err());
    }
    assert_eq!(b.routes(2000).unwrap()[0].sequence, 1);
}
#[test]
fn cannot_swap_endpoint_or_instance_or_transport() {
    let mut b = book();
    for mut addr in [address(5), address(3)] {
        if addr.address.endpoint_key == key(3).verifying_key().to_bytes() {
            addr.instance = [6; 32];
        }
        let bad = directory::sign_relay_record_v1(
            &key(2),
            "service",
            vec![addr.endpoint().unwrap()],
            2000,
            3000,
            2,
        )
        .unwrap();
        assert!(accept(&mut b, bad, 2000).is_err());
    }
    let mut r = record(2, 2, 2000, 3000);
    r.endpoints[0].transport = RelayTransportV1::Quic443;
    assert!(IrohServiceEndpointV1::from_record(&r).is_err());
}
#[test]
fn clock_rollback_and_excessive_ttl_rejected() {
    let mut b = book();
    assert!(b.routes(1999).is_err());
    assert!(accept(
        &mut b,
        record(2, 2, 2000, 2000 + MAX_SERVICE_LEASE_MS + 1),
        2000
    )
    .is_err());
}
#[test]
fn expired_renewal_can_only_bootstrap_again_with_unexpired_authority() {
    let mut b = book();
    accept(&mut b, record(2, 2, 2000, 3000), 2000).unwrap();
    assert_eq!(b.routes(3100).unwrap()[0].sequence, 1);
    accept(&mut b, record(2, 3, 3100, 4100), 3100).unwrap();
    assert!(b.routes(400_000).unwrap().is_empty());
}
#[test]
fn signed_revocation_interrupts_routes_and_cannot_be_replayed_away() {
    let mut b = book();
    b.replace_sources(vec![source(vec![], 3000)], 3000).unwrap();
    assert!(b.routes(3000).unwrap().is_empty());
    assert!(b
        .check_route(
            key(2).verifying_key().to_bytes(),
            key(3).verifying_key().to_bytes(),
            [4; 32],
            3000
        )
        .is_err());
    assert!(b
        .replace_sources(vec![source(vec![record(2, 1, 1000, 400_000)], 1000)], 3000)
        .is_err());
    assert!(b
        .replace_sources(vec![source(vec![record(2, 4, 3000, 400_000)], 3100)], 3100)
        .is_err());
}
#[test]
fn wrong_anchor_and_aggregate_disclosure_are_rejected() {
    let policy = BootstrapTrustPolicyV1::single_signer(key(1).verifying_key().to_bytes());
    let wrong = BootstrapTrustPolicyV1::single_signer(key(8).verifying_key().to_bytes());
    assert!(ServiceLeaseBookV1::new(wrong, book().sources(), 2000).is_err());
    let a = source(
        vec![record(2, 1, 1000, 400_000), record(5, 1, 1000, 400_000)],
        1000,
    );
    let mut b = source(vec![record(6, 1, 1000, 400_000)], 1000);
    b.manifest.manifest_id = "community".into();
    directory::sign_bootstrap_manifest_v1(&mut b.manifest, &key(1)).unwrap();
    assert!(ServiceLeaseBookV1::new(policy, vec![a, b], 2000).is_err());
}
#[test]
fn source_rollback_equivocation_and_bad_update_preserve_previous_route() {
    let mut b = book();
    let changed = source(vec![record(2, 2, 1000, 400_000)], 1000);
    assert!(b.replace_sources(vec![changed], 2000).is_err());
    let mut forged = source(vec![], 3000);
    forged.manifest.signatures[0].signature[0] ^= 1;
    assert!(b.replace_sources(vec![forged], 3000).is_err());
    assert_eq!(b.routes(3000).unwrap().len(), 1);
}
#[test]
fn service_uri_is_canonical_and_bounded() {
    let mut r = record(2, 1, 1000, 400_000);
    r.endpoints[0].uri.push('=');
    assert!(IrohServiceEndpointV1::from_record(&r).is_err());
    let mut a = address(3);
    a.address.direct = vec!["0.0.0.0:0".parse().unwrap()];
    assert!(a.endpoint().is_err());
    a = address(3);
    a.address.relays = vec!["http://untrusted.invalid/".into()];
    assert!(a.endpoint().is_err());
}
#[tokio::test]
async fn real_carrier_renews_three_times_without_reauthorizing_node() -> Result<()> {
    use crate::iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1};
    use std::time::{Duration, Instant};
    let config =
        IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let end = Instant::now() + Duration::from_secs(20);
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let server_barrier = barrier.clone();
    let server = with_iroh_scope_v1(
        IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)?,
        IrohControlV1::new(end, || Ok(()))?,
        move |scope| {
            Box::pin(async move {
                let endpoint = IrohServiceEndpointV1 {
                    instance: [4; 32],
                    address: scope.address()?,
                };
                let initial = directory::sign_relay_record_v1(
                    &key(2),
                    "real-service",
                    vec![endpoint.endpoint()?],
                    1000,
                    400_000,
                    1,
                )?;
                tx.send(source(vec![initial.clone()], 1000))
                    .map_err(|_| anyhow::anyhow!("fixture receiver ended"))?;
                let issuer = ServiceLeaseIssuerV1::new(key(2), initial, 2000)?;
                for _ in 0..3 {
                    let mut stream = scope.accept_inbound().await?.into();
                    serve_service_lease_v1(&mut stream, &issuer, || Ok(3000)).await?;
                    // Receive an explicit fixture ACK before closing the connection.
                    ensure!(stream.read_frame().await? == b"fixture-ack", "fixture ACK");
                    server_barrier.wait().await;
                }
                Ok(())
            })
        },
    );
    let client = async {
        let source = rx.await?;
        let mut b = ServiceLeaseBookV1::new(
            BootstrapTrustPolicyV1::single_signer(key(1).verifying_key().to_bytes()),
            vec![source],
            2000,
        )?;
        let route = IrohServiceEndpointV1::from_record(&b.routes(2000)?[0])?;
        with_iroh_scope_v1(config, IrohControlV1::new(end, || Ok(()))?, move |scope| {
            Box::pin(async move {
                for seq in 2..=4 {
                    let mut stream = scope.connect(&route.address).await?.into();
                    let r = request_service_lease_v1(&mut stream).await?;
                    assert_eq!(r.sequence, seq);
                    b.accept_renewal(
                        r,
                        key(2).verifying_key().to_bytes(),
                        route.address.endpoint_key,
                        [4; 32],
                        3000,
                    )?;
                    stream.write_frame(b"fixture-ack").await?;
                    barrier.wait().await;
                }
                Ok(())
            })
        })
        .await
    };
    let (a, b) = tokio::join!(server, client);
    a?;
    b?;
    Ok(())
}
