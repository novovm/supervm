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
