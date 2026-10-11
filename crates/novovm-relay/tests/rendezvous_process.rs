#![cfg(feature = "iroh-transport")]
//! Actual installed-entry binary, loopback transport and explicit fixture keys.
//! No production accounts, public relay, wallet, or phone acceptance implied.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    discovery_control::DiscoveryClient,
    iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1},
    product_directory::*,
    product_rendezvous_daemon::now_ms,
    product_service_lease::*,
};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

#[tokio::test(flavor = "current_thread")]
async fn executable_rejects_old_wire_and_unapproved_client_without_fallback() -> Result<()> {
    let fixture = Fixture::new();
    let mut process = fixture.launch(1, 1, 2, true, true)?;
    let record = process.proposal()?;
    let endpoint = IrohServiceEndpointV1::from_record(&record)?;
    process.approve(record, 71)?;
    let config =
        IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)?;
    with_iroh_scope_v1(
        config,
        IrohControlV1::new(Instant::now() + Duration::from_secs(12), || Ok(()))?,
        move |scope| {
            Box::pin(async move {
                let mut old = scope.connect(&endpoint.address).await?;
                old.write_frame(b"NVLSRQ01").await?;
                ensure!(old.read_frame().await.is_err(), "old prelude was accepted");
                drop(old);
                let mut stream = scope.connect(&endpoint.address).await?.into();
                request_service_offer_v2(&mut stream).await?;
                ensure!(
                    DiscoveryClient::connect(&mut stream, &key(83), pin(72), endpoint.instance)
                        .await
                        .is_err(),
                    "unapproved identity accepted"
                );
                Ok(())
            })
        },
    )
    .await?;
    let report = process.finish(true).await?.unwrap();
    assert_eq!(report["completed"], 0);
    assert_eq!(report["rejected_or_timed_out"], 2);
    Ok(())
}
fn pin(seed: u8) -> [u8; 32] {
    key(seed).verifying_key().to_bytes()
}

#[tokio::test(flavor = "current_thread")]
async fn executable_signed_revocation_ends_active_stream_and_persists_tombstone() -> Result<()> {
    let fixture = Fixture::new();
    let mut process = fixture.launch(1, 1, 2, true, true)?;
    let record = process.proposal()?;
    let endpoint = IrohServiceEndpointV1::from_record(&record)?;
    let mut source = process.approve(record, 71)?;
    let source_path = process.source.clone();
    let config =
        IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)?;
    with_iroh_scope_v1(
        config,
        IrohControlV1::new(Instant::now() + Duration::from_secs(12), || Ok(()))?,
        move |scope| {
            Box::pin(async move {
                let mut stream = scope.connect(&endpoint.address).await?.into();
                request_service_offer_v2(&mut stream).await?;
                let mut rpc =
                    DiscoveryClient::connect(&mut stream, &key(73), pin(72), endpoint.instance)
                        .await?;
                rpc.put([91; 32], 1, 10_000, vec![92; 128]).await?;
                source.manifest.relay_records.clear();
                source.manifest.issued_at_ms = now_ms()?;
                source.manifest.signatures.clear();
                sign_bootstrap_manifest_v1(&mut source.manifest, &key(71))?;
                let stage = source_path.with_extension("revocation");
                std::fs::write(&stage, serde_json::to_vec(&vec![source])?)?;
                std::fs::rename(stage, source_path)?;
                tokio::time::sleep(Duration::from_millis(600)).await;
                // Abort does not promise a graceful QUIC close notification.
                // Bound observation; no record may arrive after revocation.
                let lookup = tokio::time::timeout(Duration::from_secs(2), rpc.get([91; 32])).await;
                ensure!(
                    !matches!(lookup, Ok(Ok(_))),
                    "revoked service continued serving"
                );
                Ok(())
            })
        },
    )
    .await?;
    process.finish(false).await?;
    let (file, checkpoint) =
        ServiceLeaseFileV1::open(&fixture.journal(), Zeroizing::new([75; 32]))?;
    let mut restored = ServiceLeaseBookV1::restore(
        BootstrapTrustPolicyV1::single_signer(pin(71)),
        &checkpoint,
        now_ms()?,
    )?;
    assert!(restored.routes(now_ms()?)?.is_empty());
    let state: Value = serde_json::from_slice(&checkpoint)?;
    assert_eq!(state["tombstones"], serde_json::json!([pin(72)]));
    drop(file);
    Ok(())
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "novovm-rendezvous-entry-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn journal(&self) -> PathBuf {
        self.0.join("host.journal")
    }
    fn launch(
        &self,
        generation: u8,
        sequence: u64,
        count: usize,
        enroll: bool,
        allowed: bool,
    ) -> Result<Process> {
        let source = self.0.join(format!("source-{generation}.json"));
        let config = self.0.join(format!("config-{generation}.json"));
        std::fs::write(
            &config,
            serde_json::to_vec(&serde_json::json!({
                "version":2,"engineering_direct_allowed":allowed,
                "trusted_signers":[pin(71)],"minimum_signatures":1,"allowed_clients":[pin(73),pin(74)],
                "relays":[],"nameserver":"127.0.0.1","bind_address":"127.0.0.1:0","relay_only":false,
                "journal_path":self.journal(),"source_path":source,"enroll":enroll,
                "record_id":"explicit-loopback-fixture","grant_sequence":sequence,"run_for_ms":15000,"max_sessions":count
            }))?,
        )?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_supervm-relay"));
        command
            .args(["--runtime", "rendezvous-v2"])
            .arg(config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn()?;
        let mut input = child.stdin.take().unwrap();
        // Only deterministic test keys; the normal executable never contains them.
        let _ = input.write_all(b"NVKEYS02");
        let _ = input.write_all(&[72; 32]);
        let _ = input.write_all(&[75; 32]);
        drop(input);
        let stdout = child.stdout.take().unwrap();
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Process {
            child,
            receive,
            source,
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Process {
    child: Child,
    receive: mpsc::Receiver<std::io::Result<String>>,
    source: PathBuf,
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Process {
    fn proposal(&self) -> Result<PeerSignedRelayRecordV1> {
        let json: Value =
            serde_json::from_str(&self.receive.recv_timeout(Duration::from_secs(8))??)?;
        ensure!(
            json["event"] == "awaiting_authorization",
            "entry proposal missing"
        );
        Ok(serde_json::from_value(json["record"].clone())?)
    }
    fn approve(&self, record: PeerSignedRelayRecordV1, publisher: u8) -> Result<BootstrapSourceV1> {
        let mut manifest = SignedBootstrapManifestV1 {
            version: 1,
            manifest_id: "independent-fixture-source".into(),
            issued_at_ms: record.issued_at_ms,
            expires_at_ms: record.expires_at_ms,
            candidate_limit: 2,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: vec![record],
            signatures: vec![],
        };
        sign_bootstrap_manifest_v1(&mut manifest, &key(publisher))?;
        let source = BootstrapSourceV1 {
            source_kind: BootstrapSourceKindV1::FriendInvite,
            priority: 0,
            manifest,
        };
        let staging = self.source.with_extension("tmp");
        std::fs::write(&staging, serde_json::to_vec(&vec![source.clone()])?)?;
        std::fs::rename(staging, &self.source)?;
        Ok(source)
    }
    async fn finish(&mut self, success: bool) -> Result<Option<Value>> {
        let until = Instant::now() + Duration::from_secs(18);
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            ensure!(
                Instant::now() < until,
                "node did not terminate within epoch"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        let mut error = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut error)?;
        ensure!(
            status.success() == success,
            "unexpected node status: {error}"
        );
        if success {
            let line = self.receive.recv_timeout(Duration::from_secs(1))??;
            Ok(Some(serde_json::from_str(&line)?))
        } else {
            Ok(None)
        }
    }
}

async fn client(
    source: BootstrapSourceV1,
    seed: u8,
    barrier: Arc<tokio::sync::Barrier>,
) -> Result<()> {
    let endpoint = IrohServiceEndpointV1::from_record(&source.manifest.relay_records[0])?;
    let mut book = ServiceLeaseBookV1::new(
        BootstrapTrustPolicyV1::single_signer(pin(71)),
        vec![source],
        now_ms()?,
    )?;
    let config =
        IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)?;
    with_iroh_scope_v1(
        config,
        IrohControlV1::new(Instant::now() + Duration::from_secs(12), || Ok(()))?,
        move |scope| {
            Box::pin(async move {
                let mut stream = scope.connect(&endpoint.address).await?.into();
                let offer = request_service_offer_v2(&mut stream).await?;
                book.accept_offer(
                    offer,
                    pin(72),
                    endpoint.address.endpoint_key,
                    endpoint.instance,
                    now_ms()?,
                )?;
                let mut rpc =
                    DiscoveryClient::connect(&mut stream, &key(seed), pin(72), endpoint.instance)
                        .await?;
                rpc.put([seed; 32], 1, 10000, vec![seed; 128]).await?;
                barrier.wait().await;
                let other = if seed == 73 { 74 } else { 73 };
                let record = rpc
                    .get([other; 32])
                    .await?
                    .context("peer ciphertext missing")?;
                ensure!(record.ciphertext == vec![other; 128], "wrong opaque record");
                ensure!(
                    rpc.ack([other; 32], record.revision, record.digest).await?,
                    "ack failed"
                );
                ensure!(
                    !rpc.ack([other; 32], record.revision, record.digest).await?,
                    "duplicate ack succeeded"
                );
                ensure!(
                    rpc.get([other; 32]).await?.is_none(),
                    "consumed record remained"
                );
                barrier.wait().await;
                rpc.close().await
            })
        },
    )
    .await
}

async fn round(
    fixture: &Fixture,
    generation: u8,
    sequence: u64,
    enroll: bool,
) -> Result<(PeerSignedRelayRecordV1, u64)> {
    let mut service = fixture.launch(generation, sequence, 2, enroll, true)?;
    let record = service.proposal()?;
    let source = service.approve(record.clone(), 71)?;
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let (a, b) = tokio::join!(
        client(source.clone(), 73, barrier.clone()),
        client(source, 74, barrier)
    );
    a?;
    b?;
    let report = service.finish(true).await?.unwrap();
    assert_eq!(report["accepted"], 2);
    assert_eq!(report["completed"], 2);
    assert_eq!(report["rejected_or_timed_out"], 0);
    let (file, checkpoint) =
        ServiceLeaseFileV1::open(&fixture.journal(), Zeroizing::new([75; 32]))?;
    let saved: Value = serde_json::from_slice(&checkpoint)?;
    let seq = saved["current"][0]["sequence"]
        .as_u64()
        .context("missing persisted sequence")?;
    drop(file);
    Ok((record, seq))
}

#[tokio::test(flavor = "current_thread")]
async fn executable_two_clients_restart_and_stale_grant_rejection() -> Result<()> {
    let fixture = Fixture::new();
    let (first, sequence) = round(&fixture, 1, 1, true).await?;
    assert!(sequence > 1);
    let (second, next) = round(&fixture, 2, sequence + 1, false).await?;
    assert!(next > sequence + 1);
    let a = IrohServiceEndpointV1::from_record(&first)?;
    let b = IrohServiceEndpointV1::from_record(&second)?;
    assert_ne!(a.instance, b.instance);
    assert_ne!(a.address.endpoint_key, b.address.endpoint_key);
    // A signed but older source cannot reset journal floors after restart.
    let mut stale = fixture.launch(3, 1, 2, false, true)?;
    let _proposal = stale.proposal()?;
    stale.approve(first, 71)?;
    stale.finish(false).await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn executable_rejects_untrusted_source_missing_restore_and_implicit_permission() -> Result<()>
{
    let fixture = Fixture::new();
    let mut missing = fixture.launch(1, 1, 1, false, true)?;
    missing.finish(false).await?;
    assert!(missing.receive.try_recv().is_err());
    let mut denied = fixture.launch(2, 1, 1, true, false)?;
    denied.finish(false).await?;
    assert!(denied.receive.try_recv().is_err());
    let mut forged = fixture.launch(3, 1, 1, true, true)?;
    let record = forged.proposal()?;
    forged.approve(record, 79)?;
    forged.finish(false).await?;
    assert!(!fixture.journal().exists());
    Ok(())
}
