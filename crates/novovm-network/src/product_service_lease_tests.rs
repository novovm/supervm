use super::*;
use directory::{BootstrapSourceKindV1, SignedBootstrapManifestV1};

fn own_trust() -> SelfOwnedTrustV1 {
    SelfOwnedTrustV1 {
        version: 1,
        node_pins: [key(2).verifying_key().to_bytes()].into(),
    }
}
fn own_book() -> Result<ServiceLeaseBookV1> {
    ServiceLeaseBookV1::new_self_owned(own_trust(), vec![record(2, 1, 1000, 400_000)], 2000)
}

#[test]
fn app_quota_refusal_preserves_renewal_but_failed_revocation_stays_closed() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let budget = novovm_storage_budget::DiskBudget::configure(dir.path())?;
    let path = dir.path().join("service.checkpoint");
    let mut book = own_book()?;
    let saved = book.checkpoint()?;
    let file = ServiceLeaseFileV1::create(&path, zeroize::Zeroizing::new([91; 32]), &saved)?;
    book.attach_persistence(file, &saved)?;
    budget.set_limit(Some(novovm_storage_budget::MIN_LIMIT_BYTES))?;
    // Sparse test pressure, never production custody or fabricated receipts.
    let pressure = std::fs::File::create(dir.path().join("pressure"))?;
    pressure.set_len(novovm_storage_budget::MIN_LIMIT_BYTES)?;
    budget.snapshot()?;
    let before = std::fs::metadata(&path)?.len();
    let error = accept(&mut book, record(2, 2, 3000, 40_000), 3000).unwrap_err();
    assert!(error.chain().any(|cause| matches!(
        cause.downcast_ref::<novovm_storage_budget::Error>(),
        Some(novovm_storage_budget::Error::Io(io))
            if io.kind() == std::io::ErrorKind::StorageFull
    )));
    assert!(!book.failed);
    assert_eq!(std::fs::metadata(&path)?.len(), before);
    assert_eq!(book.persisted, saved);
    budget.set_limit(None)?;
    accept(&mut book, record(2, 2, 3000, 40_000), 3000)?;
    assert_eq!(
        book.entries[&key(2).verifying_key().to_bytes()]
            .current
            .sequence,
        2
    );

    budget.set_limit(Some(novovm_storage_budget::MIN_LIMIT_BYTES))?;
    assert!(book
        .revoke_self_owned(key(2).verifying_key().to_bytes(), 3001)
        .is_err());
    assert!(book.failed);
    budget.set_limit(None)?;
    assert!(book.routes(3002).is_err());
    Ok(())
}

#[test]
fn self_owned_policy_update_keeps_floors_revocations_and_prior_history() -> Result<()> {
    let mut old = own_book()?;
    old.replace_self_owned_records(vec![record(2, 9, 3000, 40_000)], 3000)?;
    let original = old.checkpoint()?;
    let mut trust = own_trust();
    trust.node_pins.insert(key(5).verifying_key().to_bytes());
    let next = old.prepare_self_owned_policy_update(
        trust.clone(),
        vec![record(5, 1, 3000, 40_000)],
        3000,
    )?;
    assert_eq!(old.checkpoint()?, original);
    assert_eq!(
        next.entries[&key(2).verifying_key().to_bytes()]
            .current
            .sequence,
        9
    );
    assert!(next
        .prepare_self_owned_policy_update(own_trust(), vec![], 3001)
        .is_err());
    assert!(next
        .prepare_self_owned_policy_update(trust.clone(), vec![record(2, 8, 3001, 40_000)], 3001,)
        .is_err());
    let mut revoked =
        ServiceLeaseBookV1::restore_self_owned(trust.clone(), &next.checkpoint()?, 3001)?;
    revoked.revoke_self_owned(key(2).verifying_key().to_bytes(), 3002)?;
    let updated = revoked.prepare_self_owned_policy_update(trust.clone(), vec![], 3003)?;
    assert_eq!(updated.tombstones, revoked.tombstones);
    assert!(updated
        .prepare_self_owned_policy_update(trust.clone(), vec![record(2, 20, 3003, 40_000)], 3003,)
        .is_err());
    ServiceLeaseBookV1::restore_self_owned(trust, &updated.checkpoint()?, 3003)?;
    let old = book();
    let migrated =
        old.prepare_self_owned_migration(own_trust(), vec![record(2, 10, 3000, 40_000)], 3000)?;
    let updated = migrated.prepare_self_owned_policy_update(own_trust(), vec![], 3001)?;
    assert_eq!(updated.prior_endorsement, migrated.prior_endorsement);
    Ok(())
}

#[test]
fn continuous_peer_keeps_local_trust_not_expired_locator_authority() -> Result<()> {
    let pin = key(2).verifying_key().to_bytes();
    let endpoint = address(3).address.endpoint_key;
    let mut b = own_book()?;
    // A historical locator remains only a way to find the pinned transport.
    b.check_continuous_peer_v4(pin, endpoint, 500_000)?;
    b.check_self_owned_owner()?;
    assert!(b.routes(500_000)?.is_empty());
    assert!(accept(&mut b, record(2, 2, 3000, 40_000), 500_000).is_err());
    assert!(b.check_continuous_peer_v4(pin, [99; 32], 500_000).is_err());
    assert!(b
        .check_continuous_peer_v4([99; 32], endpoint, 500_000)
        .is_err());
    // Publisher-bound authority has no implicit conversion to local trust.
    assert!(book()
        .check_continuous_peer_v4(pin, endpoint, 3000)
        .is_err());
    b.revoke_self_owned(pin, 500_001)?;
    let saved = b.checkpoint()?;
    let mut reopened = ServiceLeaseBookV1::restore_self_owned(own_trust(), &saved, 500_002)?;
    assert!(reopened
        .check_continuous_peer_v4(pin, endpoint, 500_002)
        .is_err());
    assert!(reopened
        .replace_self_owned_records(vec![record(2, 5, 500_002, 530_000)], 500_002)
        .is_err());
    let mut retired = own_book()?;
    retired.retire_for_policy_change();
    assert!(retired.check_self_owned_owner().is_err());
    assert!(retired
        .check_continuous_peer_v4(pin, endpoint, 3000)
        .is_err());
    Ok(())
}

#[test]
fn self_owned_store_rotation_requires_fresh_signature_on_unchanged_transport() -> Result<()> {
    let mut client = own_book()?;
    let mut endpoint = address(3);
    endpoint.instance = [8; 32];
    let fresh = directory::sign_relay_record_v1(
        &key(2),
        "service",
        vec![endpoint.endpoint()?],
        500_000,
        530_000,
        2,
    )?;
    let pin = key(2).verifying_key().to_bytes();
    let transport = endpoint.address.endpoint_key;
    client.check_bootstrap_route(pin, transport, [4; 32], 500_000)?;
    assert!(client
        .check_route(pin, transport, endpoint.instance, 500_000)
        .is_err());
    let mut forged = fresh.clone();
    forged.signature[0] ^= 1;
    assert!(client
        .accept_offer(
            ServiceLeaseOfferV2 {
                version: 2,
                sources: vec![],
                lease: Some(forged)
            },
            pin,
            transport,
            endpoint.instance,
            500_000
        )
        .is_err());
    client.accept_offer(
        ServiceLeaseOfferV2 {
            version: 2,
            sources: vec![],
            lease: Some(fresh.clone()),
        },
        pin,
        transport,
        endpoint.instance,
        500_000,
    )?;
    client.check_route(pin, transport, endpoint.instance, 500_000)?;
    let mut reopened =
        ServiceLeaseBookV1::restore_self_owned(own_trust(), &client.checkpoint()?, 500_001)?;
    assert_eq!(reopened.routes(500_001)?, vec![fresh]);
    assert!(accept(&mut reopened, record(2, 1, 500_001, 510_000), 500_001).is_err());
    reopened.revoke_self_owned(pin, 500_001)?;
    assert!(reopened
        .check_bootstrap_route(pin, transport, [8; 32], 500_001)
        .is_err());
    Ok(())
}

#[test]
fn compacted_self_owned_journal_keeps_revocation_and_sequence_floors() -> Result<()> {
    let dir = HostDirectory::new();
    let mut b = own_book()?;
    let bytes = b.checkpoint()?;
    let file = ServiceLeaseFileV1::create(&dir.path(), zeroize::Zeroizing::new([91; 32]), &bytes)?;
    b.attach_persistence(file.clone(), &bytes)?;
    // Multiple compactions using signed test state, not production records.
    for sequence in 2..=700 {
        let now = 2000 + sequence;
        accept(&mut b, record(2, sequence, now, now + 50_000), now)?;
    }
    let pin = key(2).verifying_key().to_bytes();
    b.revoke_self_owned(pin, 4000)?;
    let exact = b.checkpoint()?;
    drop(b);
    drop(file);
    let raw = std::fs::read(dir.path())?;
    assert!(u64::from_le_bytes(raw[8..16].try_into()?) >= 2);
    let (file, saved) = ServiceLeaseFileV1::open(&dir.path(), zeroize::Zeroizing::new([91; 32]))?;
    assert_eq!(saved, exact);
    assert!(std::fs::metadata(dir.path())?.len() < 700_000);
    let mut restored = ServiceLeaseBookV1::restore_self_owned(own_trust(), &saved, 5000)?;
    restored.attach_persistence(file, &saved)?;
    assert!(restored.routes(5000)?.is_empty());
    assert!(restored.bootstrap_candidates(5000)?.is_empty());
    assert!(restored
        .replace_self_owned_records(vec![record(2, 999, 5000, 60_000)], 5000)
        .is_err());
    Ok(())
}

#[test]
fn self_owned_does_not_need_publisher_but_does_need_confirmed_key() -> Result<()> {
    let mut b = own_book()?;
    assert!(b.sources().is_empty());
    assert_eq!(b.bootstrap_candidates(2000)?.len(), 1);
    let mut bad = record(2, 1, 1000, 400_000);
    bad.signature[0] ^= 1;
    assert!(ServiceLeaseBookV1::new_self_owned(own_trust(), vec![bad], 2000).is_err());
    assert!(ServiceLeaseBookV1::new_self_owned(
        own_trust(),
        vec![record(4, 1, 1000, 400_000)],
        2000
    )
    .is_err());
    assert!(ServiceLeaseBookV1::new_self_owned(
        own_trust(),
        vec![record(2, 1, 1000, 400_000)],
        400_000
    )
    .is_err());
    assert!(ServiceLeaseBookV1::new(
        BootstrapTrustPolicyV1 {
            allowed_signer_peer_ids: BTreeSet::new(),
            minimum_valid_signatures: 0
        },
        Vec::new(),
        2000
    )
    .is_err());
    assert!(b.replace_sources(book().sources(), 2000).is_err());
    Ok(())
}

#[test]
fn self_owned_expired_claim_is_only_locator_and_new_online_offer_is_required() -> Result<()> {
    let dir = HostDirectory::new();
    let mut b = own_book()?;
    let bytes = b.checkpoint()?;
    let file = ServiceLeaseFileV1::create(&dir.path(), zeroize::Zeroizing::new([91; 32]), &bytes)?;
    b.attach_persistence(file, &bytes)?;
    let host = ServiceLeaseHostV1::new(key(2), b)?;
    let mut client = ServiceLeaseBookV1::restore_self_owned(own_trust(), &bytes, 500_000)?;
    assert!(client.routes(500_000)?.is_empty());
    assert_eq!(client.bootstrap_candidates(500_000)?.len(), 1);
    assert!(accept(&mut client, record(2, 1, 1000, 100_000), 500_000).is_err());
    let offer = host.offer(500_000)?;
    assert!(offer.sources.is_empty());
    assert_eq!(accept_offer(&mut client, offer, 500_000)?, 620_000);
    let current = client.routes(500_000)?;
    let mut restarted =
        ServiceLeaseBookV1::restore_self_owned(own_trust(), &client.checkpoint()?, 500_001)?;
    assert_eq!(restarted.routes(500_001)?, current);
    assert!(accept(&mut restarted, record(2, 1, 500_001, 510_000), 500_001).is_err());
    Ok(())
}

#[test]
fn self_owned_live_locator_update_preserves_peer_and_persists_new_candidates() -> Result<()> {
    let dir = HostDirectory::new();
    let mut book = own_book()?;
    let bytes = book.checkpoint()?;
    let file = ServiceLeaseFileV1::create(&dir.path(), zeroize::Zeroizing::new([91; 32]), &bytes)?;
    book.attach_persistence(file, &bytes)?;
    let host = ServiceLeaseHostV1::new(key(2), book)?;
    let mut endpoint = address(3);
    let transport = endpoint.address.endpoint_key;
    host.check_continuous_peer_v4(transport, 3000)?;
    endpoint.address.direct = vec!["192.0.2.44:54321".parse()?];
    endpoint.instance = [8; 32];
    let fresh = host.rotate_self_owned(&endpoint, 3001, 93_001)?;
    directory::validate_relay_record_v1(&fresh, 3001)?;
    assert_eq!(
        IrohServiceEndpointV1::from_record(&fresh)?.address.direct,
        endpoint.address.direct
    );
    // A live peer remains authenticated; its transport identity is not replaced.
    host.check_continuous_peer_v4(transport, 3002)?;
    let offer = host.offer(3003)?;
    assert_eq!(
        IrohServiceEndpointV1::from_record(offer.lease.as_ref().unwrap())?
            .address
            .direct,
        endpoint.address.direct
    );
    endpoint.address.endpoint_key = key(9).verifying_key().to_bytes();
    assert!(host.rotate_self_owned(&endpoint, 3004, 93_004).is_err());
    drop(host);
    let (_file, saved) = ServiceLeaseFileV1::open(&dir.path(), zeroize::Zeroizing::new([91; 32]))?;
    let mut reopened = ServiceLeaseBookV1::restore_self_owned(own_trust(), &saved, 3005)?;
    let routes = reopened.routes(3005)?;
    let restored = IrohServiceEndpointV1::from_record(&routes[0])?;
    assert_eq!(restored.address.direct, endpoint.address.direct);
    assert_eq!(restored.address.endpoint_key, transport);
    assert_eq!(restored.instance, [8; 32]);
    let pin = key(2).verifying_key().to_bytes();
    reopened.revoke_self_owned(pin, 3006)?;
    assert!(reopened
        .replace_self_owned_records(vec![fresh], 3007)
        .is_err());
    assert!(reopened
        .check_continuous_peer_v4(pin, transport, 3007)
        .is_err());
    Ok(())
}

#[test]
fn self_owned_offers_reject_forgery_wrong_endpoint_and_endorsement_fallback() -> Result<()> {
    for case in 0..5 {
        let mut b = own_book()?;
        let mut lease = record(2, 2, 2000, 30_000);
        match case {
            0 => lease.signature[0] ^= 1,
            1 => lease = record(4, 2, 2000, 30_000),
            2 => lease = prepared_epoch(2, 2000, [5; 32], 6),
            3 => lease = record(2, 1, 2000, 30_000),
            _ => {}
        }
        let offer = ServiceLeaseOfferV2 {
            version: 2,
            sources: if case == 4 { book().sources() } else { vec![] },
            lease: Some(lease),
        };
        assert!(accept_offer(&mut b, offer, 2000).is_err());
        assert_eq!(b.routes(2000)?[0].sequence, 1);
    }
    let mut endorsed = book();
    assert!(accept_offer(
        &mut endorsed,
        ServiceLeaseOfferV2 {
            version: 2,
            sources: vec![],
            lease: Some(record(2, 2, 2000, 30_000))
        },
        2000
    )
    .is_err());
    Ok(())
}

#[test]
fn self_owned_revocation_survives_restart_and_rejects_new_signed_record() -> Result<()> {
    let mut b = own_book()?;
    b.revoke_self_owned(key(2).verifying_key().to_bytes(), 2000)?;
    let mut b = ServiceLeaseBookV1::restore_self_owned(own_trust(), &b.checkpoint()?, 3000)?;
    assert!(b.bootstrap_candidates(3000)?.is_empty());
    assert!(b
        .replace_self_owned_records(vec![record(2, 3, 3000, 40_000)], 3000)
        .is_err());
    assert!(accept(&mut b, record(2, 3, 3000, 40_000), 3000).is_err());
    Ok(())
}

#[test]
fn self_owned_migration_preserves_old_history_and_rejects_revoked_or_rollback() -> Result<()> {
    let mut old = book();
    accept(&mut old, record(2, 10, 2000, 30_000), 2000)?;
    assert!(old
        .prepare_self_owned_migration(own_trust(), vec![record(2, 9, 3000, 40_000)], 3000)
        .is_err());
    let next =
        old.prepare_self_owned_migration(own_trust(), vec![record(2, 11, 3000, 40_000)], 3000)?;
    assert_eq!(
        next.prior_endorsement.as_deref(),
        Some(std::str::from_utf8(&old.checkpoint()?)?)
    );
    ServiceLeaseBookV1::restore_self_owned(own_trust(), &next.checkpoint()?, 3000)?;
    old.replace_sources(vec![source(vec![], 3000)], 3000)?;
    assert!(old
        .prepare_self_owned_migration(own_trust(), vec![record(2, 11, 3000, 40_000)], 3000)
        .is_err());
    Ok(())
}
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

fn prepared_epoch(
    sequence: u64,
    issued: u64,
    instance: [u8; 32],
    carrier: u8,
) -> PeerSignedRelayRecordV1 {
    let mut endpoint = address(carrier);
    endpoint.instance = instance;
    directory::sign_relay_record_v1(
        &key(2),
        "service",
        vec![endpoint.endpoint().unwrap()],
        issued,
        400_000,
        sequence,
    )
    .unwrap()
}

#[test]
fn prepared_epoch_survives_old_online_renewal_and_durable_restore() -> Result<()> {
    let mut b = book();
    let next = prepared_epoch(66, 2500, [5; 32], 6);
    accept(&mut b, record(2, 2, 3000, 100_000), 3000)?;
    let mut recovered = restore(&b.checkpoint()?, 3100)?;
    recovered.replace_sources(vec![source(vec![next.clone()], 2600)], 3100)?;
    assert_eq!(recovered.routes(3100)?, vec![next.clone()]);
    let mut recovered = restore(&recovered.checkpoint()?, 3200)?;
    assert_eq!(recovered.routes(3200)?, vec![next]);
    assert!(accept(&mut recovered, record(2, 3, 3300, 100_000), 3300).is_err());
    Ok(())
}

#[test]
fn prepared_epoch_never_waives_rollback_signature_or_revocation() -> Result<()> {
    for bad in [
        prepared_epoch(2, 2500, [5; 32], 6), // no fresh sequence reservation
        prepared_epoch(66, 999, [5; 32], 6), // before previous independent grant
        prepared_epoch(66, 2500, [4; 32], 6), // same instance
        prepared_epoch(66, 2500, [5; 32], 3), // same transport identity
    ] {
        let mut b = book();
        accept(&mut b, record(2, 2, 3000, 100_000), 3000)?;
        let old = b.routes(3000)?;
        assert!(b
            .replace_sources(vec![source(vec![bad], 2600)], 3100)
            .is_err());
        assert_eq!(b.routes(3100)?, old);
    }
    let next = prepared_epoch(66, 2500, [5; 32], 6);
    let mut b = book();
    accept(&mut b, record(2, 2, 3000, 100_000), 3000)?;
    let mut forged = source(vec![next.clone()], 2600);
    forged.manifest.signatures[0].signature[0] ^= 1;
    assert!(b.replace_sources(vec![forged], 3100).is_err());
    b.replace_sources(vec![source(vec![], 3200)], 3200)?;
    assert!(b
        .replace_sources(vec![source(vec![next], 3300)], 3300)
        .is_err());
    assert!(b.routes(3300)?.is_empty());
    Ok(())
}

struct HostDirectory(std::path::PathBuf);
impl HostDirectory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "novovm-service-host-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("lease.journal")
    }
}
impl Drop for HostDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn durable_host(dir: &HostDirectory) -> Result<ServiceLeaseHostV1> {
    let mut b = book();
    let checkpoint = b.checkpoint()?;
    let file =
        ServiceLeaseFileV1::create(&dir.path(), zeroize::Zeroizing::new([91; 32]), &checkpoint)?;
    b.attach_persistence(file, &checkpoint)?;
    ServiceLeaseHostV1::new(key(2), b)
}
fn reopen_host(dir: &HostDirectory, now: u64) -> Result<ServiceLeaseHostV1> {
    let (file, checkpoint) =
        ServiceLeaseFileV1::open(&dir.path(), zeroize::Zeroizing::new([91; 32]))?;
    let mut b = restore(&checkpoint, now)?;
    b.attach_persistence(file, &checkpoint)?;
    ServiceLeaseHostV1::new(key(2), b)
}
fn accept_offer(b: &mut ServiceLeaseBookV1, offer: ServiceLeaseOfferV2, now: u64) -> Result<u64> {
    b.accept_offer(
        offer,
        key(2).verifying_key().to_bytes(),
        key(3).verifying_key().to_bytes(),
        [4; 32],
        now,
    )
}

fn activation_offer(record: PeerSignedRelayRecordV1) -> ServiceLeaseOfferV2 {
    ServiceLeaseOfferV2 {
        version: 2,
        sources: book().sources(),
        lease: Some(record),
    }
}

#[tokio::test]
async fn activation_wait_keeps_old_authority_until_real_local_time() -> Result<()> {
    let owner = Mutex::new(book());
    let started = std::time::Instant::now();
    let now = || Ok(2000 + started.elapsed().as_millis() as u64);
    let endpoint = address(3);
    let offer = activation_offer(record(2, 2, 2120, 100_000));
    // Legacy strict admission is unchanged: a future record is not valid now.
    assert!(accept_offer(
        &mut book(),
        activation_offer(offer.lease.clone().unwrap()),
        2000
    )
    .is_err());
    let waiting = accept_service_offer_with_activation_v2(
        &owner,
        offer,
        key(2).verifying_key().to_bytes(),
        &endpoint,
        now,
        || Ok(()),
    );
    let inspect = async {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        let mut book = owner.lock().unwrap();
        assert_eq!(book.routes(now().unwrap()).unwrap()[0].sequence, 1);
        assert!(book.clock < 2120); // signature precheck did not adopt remote time
    };
    let (result, ()) = tokio::join!(waiting, inspect);
    assert_eq!(result?, 100_000);
    assert!(now()? >= 2120);
    assert_eq!(owner.lock().unwrap().routes(now()?)?[0].sequence, 2);
    Ok(())
}

#[tokio::test]
async fn activation_wait_rejects_invalid_expired_and_excessive_future_records() -> Result<()> {
    let mut forged = record(2, 2, 2100, 100_000);
    forged.signature[0] ^= 1;
    for record in [
        forged,
        record(2, 2, 1500, 1900),
        record(2, 2, 4001, 100_000),
        record(5, 2, 2100, 100_000),
        record(2, 1, 2100, 100_000),
    ] {
        let owner = Mutex::new(book());
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(150),
            accept_service_offer_with_activation_v2(
                &owner,
                activation_offer(record),
                key(2).verifying_key().to_bytes(),
                &address(3),
                || Ok(2000),
                || Ok(()),
            ),
        )
        .await?;
        assert!(result.is_err());
        assert_eq!(owner.lock().unwrap().routes(2000)?[0].sequence, 1);
    }
    Ok(())
}

#[tokio::test]
async fn activation_wait_persists_revocation_before_considering_future_lease() -> Result<()> {
    let dir = HostDirectory::new();
    let mut b = book();
    let checkpoint = b.checkpoint()?;
    let file =
        ServiceLeaseFileV1::create(&dir.path(), zeroize::Zeroizing::new([91; 32]), &checkpoint)?;
    b.attach_persistence(file, &checkpoint)?;
    let owner = Mutex::new(b);
    let offer = ServiceLeaseOfferV2 {
        version: 2,
        sources: vec![source(vec![], 2000)],
        lease: Some(record(2, 2, 2100, 100_000)),
    };
    assert!(accept_service_offer_with_activation_v2(
        &owner,
        offer,
        key(2).verifying_key().to_bytes(),
        &address(3),
        || Ok(2000),
        || Ok(())
    )
    .await
    .is_err());
    drop(owner);
    let (_, bytes) = ServiceLeaseFileV1::open(&dir.path(), zeroize::Zeroizing::new([91; 32]))?;
    let mut restored = restore(&bytes, 2200)?;
    assert!(restored.routes(2200)?.is_empty());
    assert!(restored
        .accept_renewal(
            record(2, 2, 2200, 100_000),
            key(2).verifying_key().to_bytes(),
            key(3).verifying_key().to_bytes(),
            [4; 32],
            2200
        )
        .is_err());
    Ok(())
}

#[tokio::test]
async fn activation_wait_rechecks_revocation_while_waiting() -> Result<()> {
    let owner = Mutex::new(book());
    let start = std::time::Instant::now();
    let now = || Ok(2000 + start.elapsed().as_millis() as u64);
    let endpoint = address(3);
    let wait = accept_service_offer_with_activation_v2(
        &owner,
        activation_offer(record(2, 2, 2300, 100_000)),
        key(2).verifying_key().to_bytes(),
        &endpoint,
        now,
        || Ok(()),
    );
    let revoke = async {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        owner
            .lock()
            .unwrap()
            .replace_sources(vec![source(vec![], 2010)], now().unwrap())
            .unwrap();
    };
    let (result, ()) = tokio::join!(wait, revoke);
    assert!(result.is_err());
    assert!(owner.lock().unwrap().routes(now()?)?.is_empty());
    Ok(())
}

#[tokio::test]
async fn activation_wait_obeys_cancel_original_deadline_and_clock_rollback() -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    for rollback in [false, true] {
        let owner = Mutex::new(book());
        let clock = AtomicU64::new(2000);
        let checks = AtomicU64::new(0);
        let result = accept_service_offer_with_activation_v2(
            &owner,
            activation_offer(record(2, 2, 2100, 100_000)),
            key(2).verifying_key().to_bytes(),
            &address(3),
            || Ok(clock.load(Ordering::Acquire)),
            || {
                if checks.fetch_add(1, Ordering::AcqRel) > 0 {
                    if rollback {
                        clock.store(1999, Ordering::Release);
                    } else {
                        anyhow::bail!("original operation cancelled or expired");
                    }
                }
                Ok(())
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(owner.lock().unwrap().routes(2200)?[0].sequence, 1);
    }
    Ok(())
}

#[tokio::test]
async fn activation_wait_frozen_clock_exhausts_monotonic_budget() -> Result<()> {
    let owner = Mutex::new(book());
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        accept_service_offer_with_activation_v2(
            &owner,
            activation_offer(record(2, 2, 2100, 100_000)),
            key(2).verifying_key().to_bytes(),
            &address(3),
            || Ok(2000),
            || Ok(()),
        ),
    )
    .await?;
    assert_eq!(
        result.unwrap_err().to_string(),
        "service lease activation wait exceeded"
    );
    assert_eq!(owner.lock().unwrap().routes(2000)?[0].sequence, 1);
    Ok(())
}

#[test]
fn host_persists_before_offer_and_continues_after_reopen() -> Result<()> {
    let dir = HostDirectory::new();
    let host = durable_host(&dir)?;
    assert!(reopen_host(&dir, 3000).is_err()); // exclusive process owner
    assert_eq!(host.offer(3000)?.lease.unwrap().sequence, 2);
    drop(host); // intentionally lose the network response/ACK
    let host = reopen_host(&dir, 4000)?;
    let offer = host.offer(4000)?;
    assert_eq!(offer.lease.as_ref().unwrap().sequence, 3);
    accept_offer(&mut book(), offer, 4000)?;
    assert!(host.offer(3999).is_err());
    drop(host);
    let raw = std::fs::read(dir.path())?;
    assert!(!raw
        .windows(b"installation-source".len())
        .any(|w| w == b"installation-source"));
    assert!(ServiceLeaseFileV1::open(&dir.path(), zeroize::Zeroizing::new([92; 32])).is_err());
    assert!(reopen_host(&dir, 3999).is_err());
    Ok(())
}

#[test]
fn host_refresh_and_revocation_persist_without_root_rotation() -> Result<()> {
    let dir = HostDirectory::new();
    let host = durable_host(&dir)?;
    let mut client = book();
    accept_offer(&mut client, host.offer(3000)?, 3000)?;
    let updated = source(vec![record(2, 10, 4000, 450_000)], 4000);
    host.replace_sources(vec![updated], 4000)?;
    let offer = host.offer(5000)?;
    assert_eq!(offer.lease.as_ref().unwrap().sequence, 11);
    accept_offer(&mut client, offer, 5000)?;
    assert_eq!(client.sources()[0].manifest.issued_at_ms, 4000);
    host.replace_sources(vec![source(vec![], 6000)], 6000)?;
    assert!(accept_offer(&mut client, host.offer(7000)?, 7000).is_err());
    assert!(client.routes(7000)?.is_empty());
    drop(host);
    let host = reopen_host(&dir, 8000)?;
    assert!(host.offer(8000)?.lease.is_none());
    assert!(host
        .replace_sources(vec![source(vec![record(2, 12, 9000, 450_000)], 9000)], 9000)
        .is_err());
    assert!(host.offer(500_000).is_err());
    Ok(())
}

#[test]
fn host_rejects_torn_state_and_missing_history_without_reset() -> Result<()> {
    use std::io::Write;
    let dir = HostDirectory::new();
    let host = durable_host(&dir)?;
    host.offer(3000)?;
    drop(host);
    let mut file = std::fs::OpenOptions::new().append(true).open(dir.path())?;
    file.write_all(&99u32.to_le_bytes())?;
    file.sync_all()?;
    drop(file);
    assert!(reopen_host(&dir, 4000).is_err());
    assert!(durable_host(&dir).is_err()); // no create-or-reset
    assert!(
        ServiceLeaseFileV1::open(&dir.0.join("missing"), zeroize::Zeroizing::new([91; 32]))
            .is_err()
    );
    Ok(())
}

#[test]
fn host_write_failure_never_releases_a_lease() -> Result<()> {
    let mut b = book();
    let checkpoint = b.checkpoint()?;
    let owner = Arc::new(MemoryCheckpoint(
        Mutex::new(checkpoint.clone()),
        false.into(),
    ));
    b.attach_persistence(owner.clone(), &checkpoint)?;
    let host = ServiceLeaseHostV1::new(key(2), b)?;
    owner.1.store(true, std::sync::atomic::Ordering::Release);
    assert!(host.offer(3000).is_err());
    owner.1.store(false, std::sync::atomic::Ordering::Release);
    assert!(host.offer(4000).is_err());
    let b = restore(&owner.0.lock().unwrap(), 4000)?;
    assert_eq!(
        b.entries[&key(2).verifying_key().to_bytes()]
            .current
            .sequence,
        1
    );
    Ok(())
}

#[test]
fn offer_cannot_extend_authority_with_a_self_signed_source() -> Result<()> {
    let dir = HostDirectory::new();
    let host = durable_host(&dir)?;
    let mut offer = host.offer(3000)?;
    offer.sources[0].manifest.signatures.clear();
    directory::sign_bootstrap_manifest_v1(&mut offer.sources[0].manifest, &key(2))?;
    let mut b = book();
    assert!(accept_offer(&mut b, offer, 3000).is_err());
    assert_eq!(b.routes(3000)?[0].sequence, 1);
    let mut wrong = host.offer(4000)?;
    wrong.version = 1;
    assert!(accept_offer(&mut b, wrong, 4000).is_err());
    Ok(())
}

#[test]
fn online_refresh_keeps_independent_sources_and_local_priorities() -> Result<()> {
    let dir = HostDirectory::new();
    let host = durable_host(&dir)?;
    let mut other = source(vec![record(5, 1, 1000, 400_000)], 1000);
    other.manifest.manifest_id = "independent-source".into();
    directory::sign_bootstrap_manifest_v1(&mut other.manifest, &key(1))?;
    let mut sources = book().sources();
    sources.push(other);
    let mut client = ServiceLeaseBookV1::new(
        BootstrapTrustPolicyV1::single_signer(key(1).verifying_key().to_bytes()),
        sources,
        2000,
    )?;
    let mut offer = host.offer(3000)?;
    offer.sources[0].source_kind = BootstrapSourceKindV1::Official;
    offer.sources[0].priority = 999;
    accept_offer(&mut client, offer, 3000)?;
    assert_eq!(client.routes(3000)?.len(), 2);
    assert_eq!(
        client.sources()[0].source_kind,
        BootstrapSourceKindV1::FriendInvite
    );
    assert_eq!(client.sources()[0].priority, 0);
    host.replace_sources(vec![source(vec![], 4000)], 4000)?;
    assert!(accept_offer(&mut client, host.offer(5000)?, 5000).is_err());
    assert_eq!(client.routes(5000)?.len(), 1);
    assert_eq!(
        client.routes(5000)?[0].relay_public_key,
        key(5).verifying_key().to_bytes()
    );
    Ok(())
}

#[test]
fn durable_host_child() -> Result<()> {
    let Some(path) = std::env::var_os("NOVOVM_TEST_LEASE_JOURNAL") else {
        return Ok(());
    };
    // Test-only keys and clocks. Reopen existing history, never create on error.
    let (file, checkpoint) = ServiceLeaseFileV1::open(
        std::path::Path::new(&path),
        zeroize::Zeroizing::new([91; 32]),
    )?;
    let mut book = restore(&checkpoint, 10_000)?;
    book.attach_persistence(file, &checkpoint)?;
    let host = ServiceLeaseHostV1::new(key(2), book)?;
    let _lost_response = host.offer(10_000)?;
    Ok(())
}

#[test]
fn real_process_restarts_do_not_reissue_lost_responses() -> Result<()> {
    let dir = HostDirectory::new();
    drop(durable_host(&dir)?);
    for expected in 2..=3 {
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "product_service_lease::tests::durable_host_child",
            ])
            .env("NOVOVM_TEST_LEASE_JOURNAL", dir.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait()? {
                ensure!(status.success(), "host child failed");
                break;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("host child timed out");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (_file, checkpoint) =
            ServiceLeaseFileV1::open(&dir.path(), zeroize::Zeroizing::new([91; 32]))?;
        let b = restore(&checkpoint, 11_000)?;
        assert_eq!(
            b.entries[&key(2).verifying_key().to_bytes()]
                .current
                .sequence,
            expected
        );
    }
    Ok(())
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
fn new_connection_locator_is_not_truncated_by_previous_online_lease() -> Result<()> {
    let mut b = book();
    let grant = b.bootstrap_routes(2000)?[0].clone();
    let short = record(2, 2, 2000, 3000);
    accept(&mut b, short.clone(), 2000)?;
    // Reproduce the old admission input: a new attempt has just 50ms left even
    // though the independent publisher's locator remains valid for minutes.
    assert_eq!(b.routes(2950)?[0].expires_at_ms - 2950, 50);
    assert_eq!(b.bootstrap_routes(2950)?, vec![grant.clone()]);
    let checkpoint = b.checkpoint()?;
    let mut restored = restore(&checkpoint, 2960)?;
    assert_eq!(restored.bootstrap_routes(2960)?, vec![grant.clone()]);
    assert_eq!(restored.routes(2960)?, vec![short]);
    assert!(accept(&mut restored, grant.clone(), 2960).is_err()); // not an online renewal
    assert_eq!(restored.bootstrap_routes(3100)?, vec![grant]);
    accept(&mut restored, record(2, 3, 3100, 4100), 3100)?;
    assert_eq!(restored.routes(3100)?[0].expires_at_ms, 4100);
    assert!(restored.bootstrap_routes(400_000)?.is_empty());
    Ok(())
}

#[test]
fn bootstrap_locator_retains_source_expiry_revocation_epoch_and_clock_gates() -> Result<()> {
    let mut b = book();
    assert!(b.bootstrap_routes(1999).is_err());
    let next = prepared_epoch(66, 2500, [5; 32], 6);
    b.replace_sources(vec![source(vec![next.clone()], 2600)], 3100)?;
    assert_eq!(b.bootstrap_routes(3100)?, vec![next]);
    assert!(b
        .check_route(
            key(2).verifying_key().to_bytes(),
            key(3).verifying_key().to_bytes(),
            [4; 32],
            3100
        )
        .is_err());
    b.replace_sources(vec![source(vec![], 3200)], 3200)?;
    let mut restored = restore(&b.checkpoint()?, 3300)?;
    assert!(restored.bootstrap_routes(3300)?.is_empty());
    assert!(restored
        .replace_sources(vec![source(vec![record(2, 1, 1000, 400_000)], 3400)], 3400)
        .is_err());
    let mut shorter = source(vec![record(2, 1, 1000, 400_000)], 1000);
    shorter.manifest.expires_at_ms = 3000;
    shorter.manifest.signatures.clear();
    directory::sign_bootstrap_manifest_v1(&mut shorter.manifest, &key(1))?;
    let mut bounded = ServiceLeaseBookV1::new(
        BootstrapTrustPolicyV1::single_signer(key(1).verifying_key().to_bytes()),
        vec![shorter],
        2000,
    )?;
    assert_eq!(bounded.bootstrap_routes(2999)?.len(), 1);
    assert!(bounded.bootstrap_routes(3000)?.is_empty());
    Ok(())
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

struct MemoryCheckpoint(Mutex<Vec<u8>>, std::sync::atomic::AtomicBool);
impl ServiceLeasePersistenceV1 for MemoryCheckpoint {
    fn commit(&self, expected: &[u8], next: &[u8]) -> Result<()> {
        ensure!(
            !self.1.load(std::sync::atomic::Ordering::Acquire),
            "injected write failure"
        );
        let mut value = self.0.lock().unwrap();
        ensure!(*value == expected, "stale checkpoint");
        *value = next.to_vec();
        Ok(())
    }
}
fn restore(bytes: &[u8], now: u64) -> Result<ServiceLeaseBookV1> {
    ServiceLeaseBookV1::restore(
        BootstrapTrustPolicyV1::single_signer(key(1).verifying_key().to_bytes()),
        bytes,
        now,
    )
}
#[test]
fn durable_renewal_and_revocation_survive_reopen() -> Result<()> {
    let mut b = book();
    let initial = b.checkpoint()?;
    let owner = Arc::new(MemoryCheckpoint(Mutex::new(initial.clone()), false.into()));
    b.attach_persistence(owner.clone(), &initial)?;
    accept(&mut b, record(2, 8, 3000, 100_000), 3000)?;
    let saved = owner.0.lock().unwrap().clone();
    drop(b);
    let mut b = restore(&saved, 4000)?;
    b.attach_persistence(owner.clone(), &saved)?;
    assert!(accept(&mut b, record(2, 2, 4000, 100_000), 4000).is_err());
    b.replace_sources(vec![source(vec![], 5000)], 5000)?;
    let mut b = restore(&owner.0.lock().unwrap(), 6000)?;
    assert!(b.routes(6000)?.is_empty());
    assert!(b
        .replace_sources(vec![source(vec![record(2, 9, 6000, 400_000)], 6000)], 6000)
        .is_err());
    Ok(())
}
#[test]
fn durable_expired_history_is_retained_but_never_authorizes_route() -> Result<()> {
    let b = book();
    let bytes = b.checkpoint()?;
    let mut reopened = restore(&bytes, 900_000)?;
    assert!(reopened.routes(900_000)?.is_empty());
    assert!(reopened
        .check_route(
            key(2).verifying_key().to_bytes(),
            key(3).verifying_key().to_bytes(),
            [4; 32],
            900_000
        )
        .is_err());
    assert!(restore(&bytes, 1999).is_err());
    let wrong = BootstrapTrustPolicyV1::single_signer(key(7).verifying_key().to_bytes());
    assert!(ServiceLeaseBookV1::restore(wrong, &bytes, 4000).is_err());
    let mut invalid: serde_json::Value = serde_json::from_slice(&bytes)?;
    invalid["current"][0]["sequence"] = 999.into();
    assert!(restore(&serde_json::to_vec(&invalid)?, 4000).is_err());
    Ok(())
}
#[test]
fn durable_failure_freezes_book_and_stale_writer_cannot_replace_newer_state() -> Result<()> {
    let mut b = book();
    let initial = b.checkpoint()?;
    let owner = Arc::new(MemoryCheckpoint(Mutex::new(initial.clone()), false.into()));
    b.attach_persistence(owner.clone(), &initial)?;
    let mut stale = restore(&initial, 2000)?;
    stale.attach_persistence(owner.clone(), &initial)?;
    accept(&mut b, record(2, 3, 3000, 100_000), 3000)?;
    assert!(accept(&mut stale, record(2, 2, 3000, 100_000), 3000).is_err());
    assert!(stale.routes(3000).is_err());
    owner.1.store(true, std::sync::atomic::Ordering::Release);
    assert!(b.replace_sources(vec![source(vec![], 4000)], 4000).is_err());
    assert!(b.routes(4000).is_err());
    assert!(b.checkpoint().is_err());
    Ok(())
}
#[tokio::test]
async fn real_carrier_v2_refreshes_sources_and_durably_issues() -> Result<()> {
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
                let dir = HostDirectory::new();
                let mut book = ServiceLeaseBookV1::new(
                    BootstrapTrustPolicyV1::single_signer(key(1).verifying_key().to_bytes()),
                    vec![source(vec![initial], 1000)],
                    2000,
                )?;
                let checkpoint = book.checkpoint()?;
                let file = ServiceLeaseFileV1::create(
                    &dir.path(),
                    zeroize::Zeroizing::new([91; 32]),
                    &checkpoint,
                )?;
                book.attach_persistence(file, &checkpoint)?;
                let host = ServiceLeaseHostV1::new(key(2), book)?;
                for _ in 0..3 {
                    let mut stream = scope.accept_inbound().await?.into();
                    serve_service_offer_v2(&mut stream, &host, || Ok(3000)).await?;
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
                    let r = request_service_offer_v2(&mut stream).await?;
                    assert_eq!(r.lease.as_ref().unwrap().sequence, seq);
                    b.accept_offer(
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

#[test]
fn bootstrap_refresh_recovers_expired_routes_without_changing_trust() -> Result<()> {
    let mut b = book();
    let saved = b.checkpoint()?;
    let file = HostDirectory::new();
    let store =
        ServiceLeaseFileV1::create(&file.path(), zeroize::Zeroizing::new([92; 32]), &saved)?;
    b.attach_persistence(store.clone(), &saved)?;
    assert!(b.routes(500_001)?.is_empty());
    let mut next = source(vec![record(2, 65, 500_001, 600_000)], 500_001);
    next.manifest.expires_at_ms = 600_000;
    directory::sign_bootstrap_manifest_v1(&mut next.manifest, &key(1))?;
    next.priority = 999;
    next.source_kind = BootstrapSourceKindV1::Official;
    b.accept_bootstrap_sources(vec![next.clone()], 500_002)?;
    assert_eq!(b.routes(500_003)?[0].sequence, 65);
    assert_eq!(b.sources()[0].priority, 0);
    assert_eq!(
        b.sources()[0].source_kind,
        BootstrapSourceKindV1::FriendInvite
    );
    drop(b);
    drop(store);
    let (_, bytes) = ServiceLeaseFileV1::open(&file.path(), zeroize::Zeroizing::new([92; 32]))?;
    let mut restored = restore(&bytes, 500_004)?;
    assert_eq!(restored.routes(500_005)?[0].sequence, 65);
    next.manifest.issued_at_ms += 10;
    next.manifest.relay_records.clear();
    directory::sign_bootstrap_manifest_v1(&mut next.manifest, &key(1))?;
    restored.accept_bootstrap_sources(vec![next], 500_020)?;
    assert!(restored.routes(500_021)?.is_empty());
    let mut resurrect = source(vec![record(2, 66, 500_030, 600_000)], 500_030);
    resurrect.manifest.expires_at_ms = 600_000;
    directory::sign_bootstrap_manifest_v1(&mut resurrect.manifest, &key(1))?;
    assert!(restored
        .accept_bootstrap_sources(vec![resurrect], 500_031)
        .is_err());
    Ok(())
}

#[test]
fn bootstrap_refresh_rejects_omission_new_identity_and_bad_signatures() -> Result<()> {
    let mut b = book();
    let previous = b.checkpoint()?;
    assert!(b.accept_bootstrap_sources(vec![], 2000).is_err());
    let valid = source(vec![record(2, 2, 2000, 400_000)], 2000);
    let mut unknown = valid.clone();
    unknown.manifest.manifest_id = "unapproved-source".into();
    directory::sign_bootstrap_manifest_v1(&mut unknown.manifest, &key(1))?;
    assert!(b.accept_bootstrap_sources(vec![unknown], 2000).is_err());
    let mut forged = valid.clone();
    forged.manifest.signatures[0].signature[0] ^= 1;
    assert!(b.accept_bootstrap_sources(vec![forged], 2000).is_err());
    assert_eq!(b.checkpoint()?, previous);
    b.accept_bootstrap_sources(vec![valid], 2000)?;
    assert!(b
        .accept_bootstrap_sources(vec![source(vec![record(2, 1, 1000, 400_000)], 1000)], 2000)
        .is_err());
    Ok(())
}

#[path = "product_bootstrap_recovery_tests.rs"]
mod bootstrap_recovery;
