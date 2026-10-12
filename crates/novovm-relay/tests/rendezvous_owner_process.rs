#![cfg(all(windows, feature = "iroh-transport"))]
//! Real Windows account custody + service executable. The parent is an
//! explicitly isolated TEST publisher; no production trust root is created.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    discovery_control::DiscoveryClient,
    iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1},
    product_directory::*,
    product_rendezvous_daemon::now_ms,
    product_service_lease::*,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn pin(seed: u8) -> [u8; 32] {
    key(seed).verifying_key().to_bytes()
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "novovm-owner-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("initialize.request"), b"explicit test enrollment")?;
        std::fs::write(
            root.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "version":1,"prepare_before_ms":5000,"epoch":{
                    "version":2,"engineering_direct_allowed":true,"trusted_signers":[pin(71)],
                    "minimum_signatures":1,"allowed_clients":[pin(73),pin(74)],"relays":[],
                    "nameserver":"127.0.0.1","bind_address":"127.0.0.1:0","relay_only":false,
                    "journal_path":root.join("service.journal"),"source_path":root.join("authority.json"),
                    "enroll":false,"record_id":"owner-test-node","grant_sequence":1,"run_for_ms":10000,"max_sessions":64
                }
            }))?,
        )?;
        Ok(Self(root))
    }
    fn launch(&self, epochs: u8) -> Result<Process> {
        use std::os::windows::process::CommandExt;
        let child = Command::new(env!("CARGO_BIN_EXE_novovm-rendezvous-service"))
            .arg("--foreground")
            .arg(self.0.join("config.json"))
            .arg(epochs.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000)
            .spawn()?;
        Ok(Process(child))
    }
    async fn proposal(&self, seen: &BTreeSet<u64>) -> Result<PeerSignedRelayRecordV1> {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            for file in std::fs::read_dir(&self.0)? {
                let path = file?.path();
                if path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".proposal.json")
                {
                    // Publication may still be writing; only retry this fixture
                    // read, never a real stream or failed signature validation.
                    if let Ok(record) =
                        serde_json::from_slice::<PeerSignedRelayRecordV1>(&std::fs::read(path)?)
                    {
                        if !seen.contains(&record.sequence) {
                            return Ok(record);
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        anyhow::bail!("no new owner proposal")
    }
    fn source_path(&self, sequence: u64) -> PathBuf {
        self.0.join(format!("epoch-{sequence}.authority.json"))
    }
    fn source(&self, record: &PeerSignedRelayRecordV1) -> Result<BootstrapSourceV1> {
        validate_relay_record_v1(record, now_ms()?)?;
        let mut manifest = SignedBootstrapManifestV1 {
            version: 1,
            manifest_id: "owner-test-publisher".into(),
            issued_at_ms: now_ms()?,
            expires_at_ms: record.expires_at_ms,
            candidate_limit: 1,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: vec![record.clone()],
            signatures: vec![],
        };
        sign_bootstrap_manifest_v1(&mut manifest, &key(71))?;
        let source = BootstrapSourceV1 {
            source_kind: BootstrapSourceKindV1::FriendInvite,
            priority: 0,
            manifest,
        };
        Ok(source)
    }
    fn approve(&self, record: &PeerSignedRelayRecordV1) -> Result<BootstrapSourceV1> {
        let source = self.source(record)?;
        publish(&self.source_path(record.sequence), &source)?;
        Ok(source)
    }
}
fn publish(path: &Path, source: &BootstrapSourceV1) -> Result<()> {
    let stage = path.with_extension("staging");
    std::fs::write(&stage, serde_json::to_vec(&vec![source])?)?;
    std::fs::rename(stage, path)?;
    Ok(())
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Process {
    async fn finish(&mut self, success: bool) -> Result<()> {
        let until = Instant::now() + Duration::from_secs(14);
        while Instant::now() < until {
            if let Some(status) = self.0.try_wait()? {
                if status.success() != success {
                    use std::io::Read;
                    let mut error = String::new();
                    self.0.stderr.as_mut().unwrap().read_to_string(&mut error)?;
                    anyhow::bail!("unexpected owner result: {error}");
                }
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        anyhow::bail!("owner did not stop within deadline")
    }
}
async fn exchange(record: &PeerSignedRelayRecordV1, source: BootstrapSourceV1) -> Result<()> {
    let endpoint = IrohServiceEndpointV1::from_record(record)?;
    let node = record.relay_public_key;
    let config =
        IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)?;
    with_iroh_scope_v1(
        config,
        IrohControlV1::new(Instant::now() + Duration::from_secs(4), || Ok(()))?,
        move |scope| {
            Box::pin(async move {
                let mut stream = scope.connect(&endpoint.address).await?.into();
                let offer = request_service_offer_v2(&mut stream).await?;
                let mut book = ServiceLeaseBookV1::new(
                    BootstrapTrustPolicyV1::single_signer(pin(71)),
                    vec![source],
                    now_ms()?,
                )?;
                book.accept_offer(
                    offer,
                    node,
                    endpoint.address.endpoint_key,
                    endpoint.instance,
                    now_ms()?,
                )?;
                let mut rpc =
                    DiscoveryClient::connect(&mut stream, &key(73), node, endpoint.instance)
                        .await?;
                let slot = rand::random(); // Each probe is a distinct test object.
                rpc.put(slot, 1, 2000, vec![12; 128]).await?;
                let value = rpc.get(slot).await?.context("missing opaque object")?;
                assert_eq!(value.ciphertext, vec![12; 128]);
                assert!(rpc.ack(slot, value.revision, value.digest).await?);
                assert!(!rpc.ack(slot, value.revision, value.digest).await?);
                rpc.close().await?;
                Ok(())
            })
        },
    )
    .await
}

#[tokio::test(flavor = "current_thread")]
async fn protected_owner_prepares_next_epoch_and_recovers_identity_without_raw_key_pipe(
) -> Result<()> {
    let fixture = Fixture::new()?;
    let mut process = fixture.launch(2)?;
    let mut seen = BTreeSet::new();
    let first = fixture.proposal(&seen).await?;
    seen.insert(first.sequence);
    let source = fixture.approve(&first)?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    exchange(&first, source.clone()).await?;
    let protected_keys = std::fs::read(fixture.0.join("keys.dpapi"))?;
    let second = fixture.proposal(&seen).await?;
    seen.insert(second.sequence);
    assert!(
        now_ms()? < first.expires_at_ms,
        "next epoch was not prepared before expiry"
    );
    assert_eq!(first.relay_public_key, second.relay_public_key);
    assert!(second.sequence >= first.sequence + 65);
    assert_ne!(
        IrohServiceEndpointV1::from_record(&first)?
            .address
            .endpoint_key,
        IrohServiceEndpointV1::from_record(&second)?
            .address
            .endpoint_key
    );
    let published: Vec<BootstrapSourceV1> =
        serde_json::from_slice(&std::fs::read(fixture.0.join("active.sources.json"))?)?;
    assert_eq!(
        published[0].manifest.relay_records[0], first,
        "prepared/unapproved epoch leaked to distribution"
    );
    // The old epoch remains live while the next proposal awaits its publisher.
    // Its online lease is now newer in wall time than the reserved next grant.
    // Handoff must order separate epochs by the reserved sequence, without
    // accepting a stale online renewal or making an unapproved route visible.
    assert!(now_ms()? > second.issued_at_ms);
    exchange(&first, source)
        .await
        .context("old epoch online renewal during preparation")?;
    let source = fixture.approve(&second)?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    exchange(&second, source).await?;
    process.finish(true).await?;
    let mut restart = fixture.launch(1)?;
    let third = fixture.proposal(&seen).await?;
    assert_eq!(third.relay_public_key, first.relay_public_key);
    assert!(third.sequence >= second.sequence + 65);
    assert_eq!(std::fs::read(fixture.0.join("keys.dpapi"))?, protected_keys);
    let source = fixture.approve(&third)?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    exchange(&third, source).await?;
    restart.finish(true).await?;
    assert!(!fixture.0.join("initialize.request").exists());
    assert!(std::fs::read_dir(&fixture.0)?.all(|f| !f
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("epoch-")));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn revoked_owner_cannot_rejoin_after_restart_even_with_new_signed_grant() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut process = fixture.launch(1)?;
    let first = fixture.proposal(&BTreeSet::new()).await?;
    let mut source = fixture.approve(&first)?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    exchange(&first, source.clone()).await?;
    source.manifest.relay_records.clear();
    source.manifest.issued_at_ms = now_ms()?;
    source.manifest.signatures.clear();
    sign_bootstrap_manifest_v1(&mut source.manifest, &key(71))?;
    publish(&fixture.source_path(first.sequence), &source)?;
    process.finish(false).await?;
    let mut restarted = fixture.launch(1)?;
    let next = fixture.proposal(&BTreeSet::from([first.sequence])).await?;
    fixture.approve(&next)?;
    restarted.finish(false).await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn missing_keys_cannot_trigger_reenrollment_and_forged_publisher_never_activates(
) -> Result<()> {
    let fixture = Fixture::new()?;
    let mut process = fixture.launch(1)?;
    let first = fixture.proposal(&BTreeSet::new()).await?;
    let mut source = fixture.source(&first)?;
    source.manifest.signatures.clear();
    sign_bootstrap_manifest_v1(&mut source.manifest, &key(79))?;
    publish(&fixture.source_path(first.sequence), &source)?;
    process.finish(false).await?;
    ensure!(
        !fixture.0.join("service.journal").exists(),
        "forged publisher activated service"
    );
    std::fs::remove_file(fixture.0.join("keys.dpapi"))?;
    let mut restarted = fixture.launch(1)?;
    restarted.finish(false).await?;
    ensure!(
        !fixture.0.join("keys.dpapi").exists(),
        "missing keys regenerated"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn publisher_outage_expires_service_without_reusing_old_authority() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut process = fixture.launch(2)?;
    let first = fixture.proposal(&BTreeSet::new()).await?;
    let source = fixture.approve(&first)?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    exchange(&first, source).await?;
    let next = fixture.proposal(&BTreeSet::from([first.sequence])).await?;
    assert!(next.issued_at_ms < first.expires_at_ms);
    // Independent publisher unavailable: deliberately supply no next source.
    process.finish(false).await?;
    assert!(!fixture.source_path(next.sequence).exists());
    let status: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("status.json"))?)?;
    assert_eq!(status["phase"], "failed_closed");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn crash_consumes_reserved_sequence_and_old_signed_proposal_cannot_authorize_restart(
) -> Result<()> {
    let fixture = Fixture::new()?;
    let mut process = fixture.launch(1)?;
    let first = fixture.proposal(&BTreeSet::new()).await?;
    let old_source = fixture.source(&first)?;
    process.0.kill()?;
    process.0.wait()?;
    let mut restarted = fixture.launch(1)?;
    let next = fixture.proposal(&BTreeSet::from([first.sequence])).await?;
    assert_eq!(next.relay_public_key, first.relay_public_key);
    assert!(next.sequence >= first.sequence + 65);
    publish(&fixture.source_path(next.sequence), &old_source)?;
    restarted.finish(false).await?;
    ensure!(
        !fixture.0.join("service.journal").exists(),
        "old authorization activated a new carrier"
    );
    Ok(())
}
