use super::*;

fn key() -> SigningKey {
    SigningKey::from_bytes(&[42; 32])
}
fn pin() -> [u8; 32] {
    key().verifying_key().to_bytes()
}
fn trust() -> SelfOwnedTrustV1 {
    SelfOwnedTrustV1 {
        version: 1,
        node_pins: [pin()].into(),
    }
}
fn route() -> NativeMailboxEndpointV7 {
    NativeMailboxEndpointV7 {
        address: "127.0.0.1:30210".parse().unwrap(),
        max_connections: 4,
        max_bytes_per_minute: 64 * 1024 * 1024,
    }
}
fn record(
    endpoints: Vec<RelayEndpointV1>,
    sequence: u64,
    now: u64,
) -> Result<PeerSignedRelayRecordV1> {
    Ok(directory::sign_relay_record_v1(
        &key(),
        "native-node",
        endpoints,
        now,
        now + 90_000,
        sequence,
    )?)
}
fn book() -> Result<ServiceLeaseBookV1> {
    ServiceLeaseBookV1::new_self_owned(
        trust(),
        vec![record(vec![route().endpoint()?], 1, 1000)?],
        2000,
    )
}

#[test]
fn native_only_uses_original_trust_checkpoint_and_no_iroh_placeholder() -> Result<()> {
    let mut b = book()?;
    let candidates = b.native_mailbox_candidates_v7(2000)?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].public_key, pin());
    assert_eq!(candidates[0].endpoint, route());
    assert!(b.bootstrap_candidates(2000)?.is_empty());
    assert!(IrohServiceEndpointV1::from_record(&b.routes(2000)?[0]).is_err());
    assert!(b.check_continuous_peer_v4(pin(), pin(), 2000).is_err());
    let bytes = b.checkpoint()?;
    let mut restored = ServiceLeaseBookV1::restore_self_owned(trust(), &bytes, 2001)?;
    assert_eq!(bytes, restored.checkpoint()?);
    restored.check_native_mailbox_route_v7(pin(), &route(), 2001)?;
    Ok(())
}

#[test]
fn fresh_dial_and_healthy_native_session_have_distinct_lifetimes() -> Result<()> {
    let mut b = book()?;
    b.check_native_mailbox_route_v7(pin(), &route(), 2000)?;
    assert!(b.native_mailbox_candidates_v7(91_000)?.is_empty());
    assert!(b
        .check_native_mailbox_route_v7(pin(), &route(), 91_000)
        .is_err());
    b.check_continuous_native_peer_v7(pin(), 91_000)?;
    let mut changed = route();
    changed.address.set_port(30211);
    b.replace_self_owned_records(vec![record(vec![changed.endpoint()?], 2, 91_000)?], 91_000)?;
    b.check_continuous_native_peer_v7(pin(), 91_001)?;
    assert!(b
        .check_native_mailbox_route_v7(pin(), &route(), 91_001)
        .is_err());
    b.check_native_mailbox_route_v7(pin(), &changed, 91_001)?;
    b.retire_for_policy_change();
    assert!(b.check_continuous_native_peer_v7(pin(), 91_001).is_err());
    assert!(b.native_mailbox_candidates_v7(91_001).is_err());
    Ok(())
}

#[test]
fn native_records_reject_forgery_replay_equivocation_and_revoked_reentry() -> Result<()> {
    let mut b = book()?;
    let good = record(vec![route().endpoint()?], 2, 3000)?;
    let mut forged = good.clone();
    forged.signature[0] ^= 1;
    assert!(b.replace_self_owned_records(vec![forged], 3000).is_err());
    let other = directory::sign_relay_record_v1(
        &SigningKey::from_bytes(&[43; 32]),
        "native-node",
        vec![route().endpoint()?],
        3000,
        93_000,
        2,
    )?;
    assert!(b.replace_self_owned_records(vec![other], 3000).is_err());
    b.replace_self_owned_records(vec![good], 3000)?;
    assert!(b
        .replace_self_owned_records(vec![record(vec![route().endpoint()?], 1, 3000)?], 3000)
        .is_err());
    let mut changed = route();
    changed.address.set_port(30212);
    assert!(b
        .replace_self_owned_records(vec![record(vec![changed.endpoint()?], 2, 3000)?], 3000)
        .is_err());
    b.revoke_self_owned(pin(), 3001)?;
    let mut reopened = ServiceLeaseBookV1::restore_self_owned(trust(), &b.checkpoint()?, 3002)?;
    assert!(reopened.native_mailbox_candidates_v7(3002)?.is_empty());
    assert!(reopened
        .check_continuous_native_peer_v7(pin(), 3002)
        .is_err());
    assert!(reopened
        .replace_self_owned_records(vec![record(vec![route().endpoint()?], 3, 3002)?], 3002)
        .is_err());
    Ok(())
}

#[test]
fn udp_label_does_not_authorize_unknown_mailbox_protocol_or_malformed_alternative() -> Result<()> {
    let endpoint = route().endpoint()?;
    for uri in [
        "udp://127.0.0.1:30210",
        "udp://127.0.0.1:30210/novonet-mailbox/v6",
        "udp://127.0.0.1:030210/novonet-mailbox/v7",
        "udp://0.0.0.0:30210/novonet-mailbox/v7",
        "udp://255.255.255.255:30210/novonet-mailbox/v7",
        "udp://[ff02::1]:30210/novonet-mailbox/v7",
    ] {
        let mut wrong = endpoint.clone();
        wrong.uri = uri.into();
        let r = record(vec![wrong], 1, 1000)?;
        assert!(
            ServiceLeaseBookV1::new_self_owned(trust(), vec![r], 2000).is_err(),
            "{uri}"
        );
    }
    for capacity in [0, 65, u32::MAX] {
        let mut wrong = endpoint.clone();
        wrong.max_sessions = capacity;
        if let Ok(r) = record(vec![wrong], 1, 1000) {
            assert!(ServiceLeaseBookV1::new_self_owned(trust(), vec![r], 2000).is_err());
        }
    }
    let r = record(vec![endpoint.clone(), endpoint], 1, 1000)?;
    assert!(ServiceLeaseBookV1::new_self_owned(trust(), vec![r], 2000).is_err());
    let ipv6 = NativeMailboxEndpointV7 {
        address: "[2001:db8::1]:30210".parse()?,
        max_connections: 2,
        max_bytes_per_minute: 64 * 1024 * 1024,
    };
    let r = record(vec![ipv6.endpoint()?], 1, 1000)?;
    assert_eq!(NativeMailboxEndpointV7::from_record(&r)?, Some(ipv6));
    Ok(())
}

fn durable_host(mut b: ServiceLeaseBookV1, path: &std::path::Path) -> Result<ServiceLeaseHostV1> {
    let bytes = b.checkpoint()?;
    let file = ServiceLeaseFileV1::create(path, zeroize::Zeroizing::new([91; 32]), &bytes)?;
    b.attach_persistence(file, &bytes)?;
    ServiceLeaseHostV1::new(key(), b)
}

#[test]
fn native_host_publication_is_durable_and_keeps_legacy_alternatives_on_rotation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("host.enc");
    let iroh = IrohServiceEndpointV1 {
        instance: [4; 32],
        address: IrohAddressV1 {
            endpoint_key: SigningKey::from_bytes(&[43; 32]).verifying_key().to_bytes(),
            direct: vec!["127.0.0.1:40210".parse()?],
            relays: vec![],
        },
    };
    let tls = TlsServiceEndpointV1 {
        address: "127.0.0.1:40211".parse()?,
        endpoint_key: pin(),
        max_connections: 4,
    };
    let b = ServiceLeaseBookV1::new_self_owned(
        trust(),
        vec![record(vec![iroh.endpoint()?, tls.endpoint()?], 1, 1000)?],
        2000,
    )?;
    let host = durable_host(b, &path)?;
    let r = host.rotate_native_mailbox_v7(Some(&route()), 3000, 93_000)?;
    assert_eq!(r.endpoints.len(), 3);
    assert_eq!(NativeMailboxEndpointV7::from_record(&r)?, Some(route()));
    host.check_continuous_native_peer_v7(3000)?;
    let r = host.rotate_self_owned(&iroh, 3001, 93_001)?;
    assert_eq!(NativeMailboxEndpointV7::from_record(&r)?, Some(route()));
    assert_eq!(TlsServiceEndpointV1::from_record(&r)?, Some(tls.clone()));
    let r = host.rotate_self_owned_with_tls_v1(&iroh, Some(&tls), 3002, 93_002)?;
    assert_eq!(NativeMailboxEndpointV7::from_record(&r)?, Some(route()));
    drop(host);
    let (file, saved) = ServiceLeaseFileV1::open(&path, zeroize::Zeroizing::new([91; 32]))?;
    let mut reopened = ServiceLeaseBookV1::restore_self_owned(trust(), &saved, 3003)?;
    reopened.attach_persistence(file, &saved)?;
    reopened.check_native_mailbox_route_v7(pin(), &route(), 3003)?;
    let host = ServiceLeaseHostV1::new(key(), reopened)?;
    let r = host.rotate_native_mailbox_v7(None, 3004, 93_004)?;
    assert_eq!(r.endpoints.len(), 2);
    assert!(host.check_continuous_native_peer_v7(3004).is_err());
    host.check_continuous_peer_v4(iroh.address.endpoint_key, 3004)?;
    Ok(())
}

#[test]
fn native_only_host_renews_without_legacy_lease_protocol() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let host = durable_host(book()?, &directory.path().join("native.enc"))?;
    let r = host.rotate_native_mailbox_v7(Some(&route()), 200_000, 290_000)?;
    assert_eq!(r.sequence, 2);
    assert_eq!(r.endpoints.len(), 1);
    host.check_continuous_native_peer_v7(200_001)?;
    assert!(host.offer(200_001).is_err());
    assert!(host
        .rotate_native_mailbox_v7(None, 200_001, 290_001)
        .is_err());
    host.check_continuous_native_peer_v7(200_002)?;
    Ok(())
}

#[test]
fn native_wire_budget_is_signed_preserved_and_bound_to_fresh_dials() -> Result<()> {
    let mut b = book()?;
    let original = route();
    let mut limited = original.clone();
    limited.max_bytes_per_minute = 32 * 1024 * 1024;
    let record = record(vec![limited.endpoint()?], 2, 3000)?;
    assert_eq!(
        NativeMailboxEndpointV7::from_record(&record)?,
        Some(limited.clone())
    );
    let mut forged = record.clone();
    forged.endpoints[0].max_bytes_per_minute += 1;
    assert!(b.replace_self_owned_records(vec![forged], 3000).is_err());
    b.replace_self_owned_records(vec![record], 3000)?;
    assert!(b
        .check_native_mailbox_route_v7(pin(), &original, 3001)
        .is_err());
    b.check_native_mailbox_route_v7(pin(), &limited, 3001)?;
    let saved = b.checkpoint()?;
    let mut restored = ServiceLeaseBookV1::restore_self_owned(trust(), &saved, 3002)?;
    assert_eq!(
        restored.native_mailbox_candidates_v7(3002)?[0].endpoint,
        limited
    );
    // This does not reset or expand the already-open socket's budget.
    restored.check_continuous_native_peer_v7(pin(), 3002)?;
    for limit in [0, 2399, 1024 * 1024 * 1024 + 1, u64::MAX] {
        let mut wrong = original.clone();
        wrong.max_bytes_per_minute = limit;
        assert!(wrong.endpoint().is_err());
    }
    Ok(())
}
