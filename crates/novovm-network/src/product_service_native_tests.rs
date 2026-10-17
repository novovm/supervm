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

#[test]
fn expired_native_locator_is_probe_only_and_remains_bound_to_original_pin_and_route() -> Result<()>
{
    let b = book()?;
    let mut reopened = ServiceLeaseBookV1::restore_self_owned(trust(), &b.checkpoint()?, 91_000)?;
    let probes = reopened.native_mailbox_bootstrap_v7(91_000)?;
    assert_eq!(probes.len(), 1);
    assert_eq!(probes[0].public_key, pin());
    assert_eq!(probes[0].endpoint, route());
    reopened.check_native_bootstrap_route_v7(pin(), &route(), 91_000)?;
    assert!(reopened.native_mailbox_candidates_v7(91_000)?.is_empty());
    assert!(reopened
        .check_native_mailbox_route_v7(pin(), &route(), 91_000)
        .is_err());
    let unknown = SigningKey::from_bytes(&[43; 32]).verifying_key().to_bytes();
    assert!(reopened
        .check_native_bootstrap_route_v7(unknown, &route(), 91_000)
        .is_err());
    let mut changed = route();
    changed.address.set_port(30213);
    assert!(reopened
        .check_native_bootstrap_route_v7(pin(), &changed, 91_000)
        .is_err());
    // A signed new locator, not the historical address, admits new business.
    reopened.accept_native_record_v7(
        pin(),
        record(vec![changed.endpoint()?], 2, 91_000)?,
        91_000,
    )?;
    reopened.check_native_mailbox_route_v7(pin(), &changed, 91_001)?;
    assert!(reopened
        .check_native_bootstrap_route_v7(pin(), &route(), 91_001)
        .is_err());
    reopened.retire_for_policy_change();
    assert!(reopened.native_mailbox_bootstrap_v7(91_001).is_err());
    Ok(())
}

#[test]
fn single_native_refresh_persists_without_refreshing_another_offline_node() -> Result<()> {
    let other_key = SigningKey::from_bytes(&[43; 32]);
    let other_pin = other_key.verifying_key().to_bytes();
    let mut other_route = route();
    other_route.address.set_port(30214);
    let other_record = directory::sign_relay_record_v1(
        &other_key,
        "offline-native-node",
        vec![other_route.endpoint()?],
        1000,
        91_000,
        1,
    )?;
    let both = SelfOwnedTrustV1 {
        version: 1,
        node_pins: [pin(), other_pin].into(),
    };
    let mut b = ServiceLeaseBookV1::new_self_owned(
        both.clone(),
        vec![
            record(vec![route().endpoint()?], 1, 1000)?,
            other_record.clone(),
        ],
        2000,
    )?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("two-native-pins.enc");
    let initial = b.checkpoint()?;
    let file = ServiceLeaseFileV1::create(&path, zeroize::Zeroizing::new([92; 32]), &initial)?;
    b.attach_persistence(file, &initial)?;
    assert!(b.native_mailbox_candidates_v7(91_000)?.is_empty());
    let fresh = record(vec![route().endpoint()?], 2, 91_000)?;
    b.accept_native_record_v7(pin(), fresh.clone(), 91_000)?;
    let candidates = b.native_mailbox_candidates_v7(91_000)?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].public_key, pin());
    assert_eq!(b.native_mailbox_bootstrap_v7(91_000)?.len(), 2);
    assert!(b.self_owned_records()?.contains(&other_record));
    let saved = b.durable_checkpoint()?;
    drop(b);
    let (file, disk) = ServiceLeaseFileV1::open(&path, zeroize::Zeroizing::new([92; 32]))?;
    assert_eq!(saved, disk);
    assert!(ServiceLeaseBookV1::restore_self_owned(both.clone(), &disk, 90_999).is_err());
    let mut reopened = ServiceLeaseBookV1::restore_self_owned(both, &disk, 91_001)?;
    reopened.attach_persistence(file, &disk)?;
    assert!(reopened.self_owned_records()?.contains(&fresh));
    assert!(reopened.self_owned_records()?.contains(&other_record));
    reopened.check_native_mailbox_route_v7(pin(), &route(), 91_001)?;
    reopened.check_native_bootstrap_route_v7(other_pin, &other_route, 91_001)?;
    assert!(reopened
        .check_native_mailbox_route_v7(other_pin, &other_route, 91_001)
        .is_err());
    // Neither lower sequence nor a newer sequence with older issuance can lower
    // the original per-pin high-water after a genuine encrypted-store reopen.
    assert!(reopened
        .accept_native_record_v7(pin(), record(vec![route().endpoint()?], 1, 91_001)?, 91_001)
        .is_err());
    assert!(reopened
        .accept_native_record_v7(pin(), record(vec![route().endpoint()?], 3, 90_999)?, 91_001)
        .is_err());
    assert_eq!(reopened.durable_checkpoint()?, saved);
    Ok(())
}

#[test]
fn native_refresh_rejects_forgery_wrong_pin_equivocation_and_durable_revoked_reentry() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("native-refresh.enc");
    let mut b = book()?;
    let initial = b.checkpoint()?;
    let file = ServiceLeaseFileV1::create(&path, zeroize::Zeroizing::new([93; 32]), &initial)?;
    b.attach_persistence(file, &initial)?;
    let good = record(vec![route().endpoint()?], 2, 91_000)?;
    let mut forged = good.clone();
    forged.signature[0] ^= 1;
    assert!(b.accept_native_record_v7(pin(), forged, 91_000).is_err());
    let unknown_key = SigningKey::from_bytes(&[44; 32]);
    let unknown = unknown_key.verifying_key().to_bytes();
    let alien = directory::sign_relay_record_v1(
        &unknown_key,
        "unknown-native",
        vec![route().endpoint()?],
        91_000,
        181_000,
        2,
    )?;
    assert!(b
        .accept_native_record_v7(pin(), alien.clone(), 91_000)
        .is_err());
    assert!(b.accept_native_record_v7(unknown, alien, 91_000).is_err());
    assert_eq!(b.durable_checkpoint()?, initial);
    b.accept_native_record_v7(pin(), good.clone(), 91_000)?;
    let accepted = b.durable_checkpoint()?;
    b.accept_native_record_v7(pin(), good, 91_000)?;
    assert_eq!(b.durable_checkpoint()?, accepted);
    let mut changed = route();
    changed.address.set_port(30215);
    assert!(b
        .accept_native_record_v7(pin(), record(vec![changed.endpoint()?], 2, 91_000)?, 91_000)
        .is_err());
    assert!(b
        .accept_native_record_v7(pin(), record(vec![route().endpoint()?], 1, 91_000)?, 91_000)
        .is_err());
    assert_eq!(b.durable_checkpoint()?, accepted);
    b.revoke_self_owned(pin(), 91_001)?;
    let revoked = b.durable_checkpoint()?;
    drop(b);
    let (file, disk) = ServiceLeaseFileV1::open(&path, zeroize::Zeroizing::new([93; 32]))?;
    assert_eq!(disk, revoked);
    let mut reopened = ServiceLeaseBookV1::restore_self_owned(trust(), &disk, 91_002)?;
    reopened.attach_persistence(file, &disk)?;
    assert!(reopened.native_mailbox_bootstrap_v7(91_002)?.is_empty());
    assert!(reopened.native_mailbox_candidates_v7(91_002)?.is_empty());
    assert!(reopened
        .check_native_bootstrap_route_v7(pin(), &route(), 91_002)
        .is_err());
    assert!(reopened
        .check_continuous_native_peer_v7(pin(), 91_002)
        .is_err());
    assert!(reopened
        .accept_native_record_v7(pin(), record(vec![route().endpoint()?], 3, 91_002)?, 91_002)
        .is_err());
    assert_eq!(reopened.durable_checkpoint()?, revoked);
    Ok(())
}

fn carrier() -> NativeCarrierMailboxEndpointV7 {
    NativeCarrierMailboxEndpointV7 {
        iroh: IrohServiceEndpointV1 {
            instance: [4; 32],
            address: IrohAddressV1 {
                endpoint_key: SigningKey::from_bytes(&[43; 32]).verifying_key().to_bytes(),
                direct: vec!["127.0.0.1:40210".parse().unwrap()],
                relays: vec![],
            },
        },
    }
}

fn carrier_book() -> Result<ServiceLeaseBookV1> {
    ServiceLeaseBookV1::new_self_owned(
        trust(),
        vec![record(vec![carrier().endpoint()?], 1, 1000)?],
        2000,
    )
}

#[test]
fn native_carrier_marker_is_explicit_signed_and_not_a_fourth_physical_route() -> Result<()> {
    let endpoint = carrier();
    let legacy = record(vec![endpoint.iroh.endpoint()?], 1, 1000)?;
    assert!(NativeCarrierMailboxEndpointV7::from_record(&legacy)?.is_none());
    let mut legacy_book = ServiceLeaseBookV1::new_self_owned(trust(), vec![legacy.clone()], 2000)?;
    assert!(legacy_book
        .native_carrier_mailbox_candidates_v7(2000)?
        .is_empty());
    assert!(legacy_book
        .check_continuous_native_carrier_v7(pin(), endpoint.iroh.address.endpoint_key, 2000)
        .is_err());
    assert!(NativeCarrierMailboxEndpointV7::from_record(&record(
        vec![route().endpoint()?],
        1,
        1000
    )?)?
    .is_none());
    let tls = TlsServiceEndpointV1 {
        address: "127.0.0.1:40211".parse()?,
        endpoint_key: pin(),
        max_connections: 4,
    };
    let explicit = record(
        vec![endpoint.endpoint()?, tls.endpoint()?, route().endpoint()?],
        1,
        1000,
    )?;
    let mut b = ServiceLeaseBookV1::new_self_owned(trust(), vec![explicit.clone()], 2000)?;
    assert_eq!(b.routes(2000)?[0].endpoints.len(), 3);
    let candidate = b.native_carrier_mailbox_candidates_v7(2000)?.remove(0);
    assert_eq!(candidate.public_key, pin());
    assert_eq!(candidate.record_expires_at_ms, 91_000);
    assert_eq!(candidate.endpoint.endpoint()?, endpoint.endpoint()?);
    assert_eq!(b.native_mailbox_candidates_v7(2000)?[0].endpoint, route());
    assert_eq!(
        IrohServiceEndpointV1::from_record(&explicit)?.endpoint()?,
        endpoint.iroh.endpoint()?
    );
    let decoded = ServiceEndpoints::from_record(&explicit)?;
    assert!(decoded.iroh.is_some() && decoded.native_carrier.is_some());
    // Old Iroh parsers decode the entire suffix after PREFIX as base64; the
    // slash deliberately causes rejection rather than silent codec fallback.
    assert!(URL_SAFE_NO_PAD
        .decode(explicit.endpoints[0].uri.strip_prefix(PREFIX).unwrap())
        .is_err());
    let mut forged_upgrade = legacy;
    forged_upgrade.endpoints[0].uri.push_str(SUFFIX);
    assert!(ServiceLeaseBookV1::new_self_owned(trust(), vec![forged_upgrade], 2000).is_err());
    let mut forged_downgrade = explicit;
    forged_downgrade.endpoints[0] = endpoint.iroh.endpoint()?;
    assert!(ServiceLeaseBookV1::new_self_owned(trust(), vec![forged_downgrade], 2000).is_err());
    Ok(())
}

#[test]
fn native_carrier_parser_keeps_canonical_version_key_and_endpoint_bounds() -> Result<()> {
    let endpoint = carrier();
    let base = endpoint.iroh.endpoint()?;
    for uri in [
        format!("{}/novonet-mailbox/v6", base.uri),
        format!("{}/novonet-mailbox/v8", base.uri),
        format!("{}{SUFFIX}/", base.uri),
        format!("{}{SUFFIX}{SUFFIX}", base.uri),
        format!("{}%2Fnovonet-mailbox%2Fv7", base.uri),
        format!("{}={SUFFIX}", base.uri),
        format!(
            "{PREFIX}{}{SUFFIX}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec_pretty(&endpoint.iroh)?)
        ),
    ] {
        let mut wrong = base.clone();
        wrong.uri = uri.clone();
        let record = record(vec![wrong], 1, 1000)?;
        assert!(
            ServiceLeaseBookV1::new_self_owned(trust(), vec![record], 2000).is_err(),
            "{uri}"
        );
    }
    // Duplicate legacy+native views of one physical carrier are not accepted.
    let duplicate = record(vec![base, endpoint.endpoint()?], 1, 1000)?;
    assert!(ServiceLeaseBookV1::new_self_owned(trust(), vec![duplicate], 2000).is_err());
    let mut invalid_key = endpoint.clone();
    invalid_key.iroh.address.endpoint_key = [0; 32];
    assert!(invalid_key.endpoint().is_err());
    let mut invalid_instance = endpoint.clone();
    invalid_instance.iroh.instance = [0; 32];
    assert!(invalid_instance.endpoint().is_err());
    let mut invalid_route = endpoint;
    invalid_route.iroh.address.direct.clear();
    assert!(invalid_route.endpoint().is_err());
    Ok(())
}

#[test]
fn native_carrier_live_session_survives_locator_expiry_but_new_dial_requires_exact_fresh_route(
) -> Result<()> {
    let endpoint = carrier();
    let transport = endpoint.iroh.address.endpoint_key;
    let mut b = carrier_book()?;
    b.check_native_carrier_route_v7(pin(), &endpoint, 2000)?;
    assert!(b
        .check_continuous_native_carrier_v7([99; 32], transport, 2000)
        .is_err());
    assert!(b
        .check_continuous_native_carrier_v7(pin(), pin(), 2000)
        .is_err());
    // Carrier capability alone does not authorize the independent UDP entry.
    assert!(b.check_continuous_native_peer_v7(pin(), 2000).is_err());
    assert!(b.native_carrier_mailbox_candidates_v7(91_000)?.is_empty());
    assert!(b
        .check_native_carrier_route_v7(pin(), &endpoint, 91_000)
        .is_err());
    b.check_continuous_native_carrier_v7(pin(), transport, 91_000)?;
    let mut fresh = endpoint.clone();
    fresh.iroh.instance = [5; 32];
    fresh.iroh.address.direct[0].set_port(40212);
    b.replace_self_owned_records(vec![record(vec![fresh.endpoint()?], 2, 91_000)?], 91_000)?;
    b.check_continuous_native_carrier_v7(pin(), transport, 91_001)?;
    assert!(b
        .check_native_carrier_route_v7(pin(), &endpoint, 91_001)
        .is_err());
    b.check_native_carrier_route_v7(pin(), &fresh, 91_001)?;
    let mut wrong_address = fresh.clone();
    wrong_address.iroh.address.direct[0].set_port(40213);
    assert!(b
        .check_native_carrier_route_v7(pin(), &wrong_address, 91_001)
        .is_err());
    b.replace_self_owned_records(
        vec![record(
            vec![fresh.iroh.endpoint()?, route().endpoint()?],
            3,
            91_002,
        )?],
        91_002,
    )?;
    assert!(b
        .check_continuous_native_carrier_v7(pin(), transport, 91_002)
        .is_err());
    assert!(b
        .check_native_carrier_route_v7(pin(), &fresh, 91_002)
        .is_err());
    assert!(b.native_carrier_mailbox_candidates_v7(91_002)?.is_empty());
    b.check_continuous_native_peer_v7(pin(), 91_002)?;
    b.check_continuous_peer_v4(pin(), transport, 91_002)?;
    Ok(())
}

#[test]
fn native_carrier_replay_key_change_revocation_and_checkpoint_do_not_restore_old_capability(
) -> Result<()> {
    let endpoint = carrier();
    let transport = endpoint.iroh.address.endpoint_key;
    let mut b = carrier_book()?;
    let good = record(vec![endpoint.endpoint()?], 2, 3000)?;
    let mut forged = good.clone();
    forged.signature[0] ^= 1;
    assert!(b.replace_self_owned_records(vec![forged], 3000).is_err());
    b.replace_self_owned_records(vec![good], 3000)?;
    let saved = b.checkpoint()?;
    let mut b = ServiceLeaseBookV1::restore_self_owned(trust(), &saved, 3001)?;
    assert_eq!(b.checkpoint()?, saved);
    b.check_native_carrier_route_v7(pin(), &endpoint, 3001)?;
    assert!(b
        .replace_self_owned_records(vec![record(vec![endpoint.endpoint()?], 1, 3001)?], 3001)
        .is_err());
    assert!(b
        .replace_self_owned_records(
            vec![record(vec![endpoint.iroh.endpoint()?], 2, 3000)?],
            3001
        )
        .is_err());
    assert!(b
        .check_continuous_native_carrier_v7(pin(), transport, 3000)
        .is_err());
    let mut replacement = endpoint.clone();
    replacement.iroh.address.endpoint_key =
        SigningKey::from_bytes(&[44; 32]).verifying_key().to_bytes();
    b.replace_self_owned_records(vec![record(vec![replacement.endpoint()?], 3, 3002)?], 3002)?;
    assert!(b
        .check_continuous_native_carrier_v7(pin(), transport, 3002)
        .is_err());
    b.check_continuous_native_carrier_v7(pin(), replacement.iroh.address.endpoint_key, 3002)?;
    b.revoke_self_owned(pin(), 3003)?;
    let mut revoked = ServiceLeaseBookV1::restore_self_owned(trust(), &b.checkpoint()?, 3004)?;
    assert!(revoked
        .native_carrier_mailbox_candidates_v7(3004)?
        .is_empty());
    assert!(revoked
        .check_continuous_native_carrier_v7(pin(), replacement.iroh.address.endpoint_key, 3004)
        .is_err());
    assert!(revoked
        .replace_self_owned_records(vec![record(vec![replacement.endpoint()?], 4, 3004)?], 3004)
        .is_err());
    let mut retired = carrier_book()?;
    retired.retire_for_policy_change();
    assert!(retired
        .check_continuous_native_carrier_v7(pin(), transport, 2001)
        .is_err());
    assert!(retired.native_carrier_mailbox_candidates_v7(2001).is_err());
    Ok(())
}

#[test]
fn native_carrier_host_rotation_preserves_marker_and_udp_until_explicit_disable() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("carrier.enc");
    let endpoint = carrier();
    let transport = endpoint.iroh.address.endpoint_key;
    let tls = TlsServiceEndpointV1 {
        address: "127.0.0.1:40211".parse()?,
        endpoint_key: pin(),
        max_connections: 4,
    };
    let b = ServiceLeaseBookV1::new_self_owned(
        trust(),
        vec![record(
            vec![
                endpoint.iroh.endpoint()?,
                tls.endpoint()?,
                route().endpoint()?,
            ],
            1,
            1000,
        )?],
        2000,
    )?;
    let host = durable_host(b, &path)?;
    assert!(host
        .check_continuous_native_carrier_v7(transport, 2000)
        .is_err());
    let enabled = host.rotate_native_carrier_mailbox_v7(true, 3000, 93_000)?;
    assert_eq!(enabled.endpoints.len(), 3);
    assert_eq!(
        NativeCarrierMailboxEndpointV7::from_record(&enabled)?
            .unwrap()
            .endpoint()?,
        endpoint.endpoint()?
    );
    host.check_continuous_native_carrier_v7(transport, 3000)?;
    let mut rotated = endpoint.clone();
    rotated.iroh.instance = [5; 32];
    let next = host.rotate_self_owned(&rotated.iroh, 3001, 93_001)?;
    assert_eq!(
        NativeCarrierMailboxEndpointV7::from_record(&next)?
            .unwrap()
            .endpoint()?,
        rotated.endpoint()?
    );
    assert_eq!(NativeMailboxEndpointV7::from_record(&next)?, Some(route()));
    let next = host.rotate_self_owned_with_tls_v1(&rotated.iroh, Some(&tls), 3002, 93_002)?;
    assert!(NativeCarrierMailboxEndpointV7::from_record(&next)?.is_some());
    let mut udp = route();
    udp.address.set_port(30216);
    let next = host.rotate_native_mailbox_v7(Some(&udp), 3003, 93_003)?;
    assert!(NativeCarrierMailboxEndpointV7::from_record(&next)?.is_some());
    assert_eq!(
        NativeMailboxEndpointV7::from_record(&next)?,
        Some(udp.clone())
    );
    drop(host);
    let (file, saved) = ServiceLeaseFileV1::open(&path, zeroize::Zeroizing::new([91; 32]))?;
    let mut b = ServiceLeaseBookV1::restore_self_owned(trust(), &saved, 3004)?;
    b.attach_persistence(file, &saved)?;
    b.check_native_carrier_route_v7(pin(), &rotated, 3004)?;
    let host = ServiceLeaseHostV1::new(key(), b)?;
    let disabled = host.rotate_native_carrier_mailbox_v7(false, 3005, 93_005)?;
    assert_eq!(disabled.endpoints.len(), 3);
    assert!(NativeCarrierMailboxEndpointV7::from_record(&disabled)?.is_none());
    assert!(host
        .check_continuous_native_carrier_v7(transport, 3005)
        .is_err());
    assert_eq!(NativeMailboxEndpointV7::from_record(&disabled)?, Some(udp));
    assert_eq!(TlsServiceEndpointV1::from_record(&disabled)?, Some(tls));
    host.check_continuous_native_peer_v7(3005)?;
    host.check_continuous_peer_v4(transport, 3005)?;
    Ok(())
}

#[test]
fn native_carrier_record_refresh_reuses_pin_and_high_water_without_accepting_legacy() -> Result<()>
{
    let mut b = carrier_book()?;
    let mut endpoint = carrier();
    endpoint.iroh.instance = [9; 32];
    let fresh = record(vec![endpoint.endpoint()?], 2, 91_000)?;
    b.accept_native_record_v7(pin(), fresh.clone(), 91_000)?;
    b.check_native_carrier_route_v7(pin(), &endpoint, 91_000)?;
    let saved = b.checkpoint()?;
    b.accept_native_record_v7(pin(), fresh.clone(), 91_000)?;
    assert_eq!(b.checkpoint()?, saved);
    for rejected in [
        record(vec![endpoint.iroh.endpoint()?], 3, 91_000)?,
        record(vec![endpoint.endpoint()?], 1, 91_000)?,
        record(vec![carrier().endpoint()?], 2, 91_000)?,
        {
            let mut forged = fresh;
            forged.signature[0] ^= 1;
            forged
        },
    ] {
        assert!(b.accept_native_record_v7(pin(), rejected, 91_000).is_err());
        assert_eq!(b.checkpoint()?, saved);
    }
    assert!(b.check_continuous_native_peer_v7(pin(), 91_000).is_err());
    b.revoke_self_owned(pin(), 91_001)?;
    assert!(b
        .accept_native_record_v7(
            pin(),
            record(vec![endpoint.endpoint()?], 3, 91_001)?,
            91_001,
        )
        .is_err());
    Ok(())
}
