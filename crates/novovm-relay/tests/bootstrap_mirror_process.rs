#![cfg(all(windows, feature = "iroh-transport"))]
#[path = "support/bootstrap_fixture.rs"]
mod fixture;
use anyhow::{ensure, Context, Result};
use fixture::*;
use novovm_network::{
    iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1},
    product_bootstrap_refresh::{refresh_bootstrap_sources_v1, BootstrapRefreshPeerV1},
    product_directory::{BootstrapSourceV1, BootstrapTrustPolicyV1},
    product_rendezvous_daemon::now_ms,
    product_service_lease::ServiceLeaseBookV1,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    net::UdpSocket,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const MIRROR: &str = env!("CARGO_BIN_EXE_novovm-bootstrap-mirror");

fn initialize(f: &Fixture, expiry: u64) -> Result<()> {
    std::fs::create_dir(f.root.join("mirror"))?;
    let port = UdpSocket::bind("127.0.0.1:0")?.local_addr()?;
    json_file(
        &f.root.join("mirror-config.json"),
        &json!({"version":1,"engineering_direct_allowed":true,
        "state_directory":f.root.join("mirror"),"source_path":f.root.join("node/active.sources.json"),
        "trust_anchors":[f.publisher_pin],"minimum_signatures":1,"source_ids":["test-independent-publisher"],
        "relays":[],"dns":"127.0.0.1","bind_address":port,"relay_only":false,"permission_expires_at_ms":expiry}),
    )?;
    let output = command(MIRROR)
        .arg("--initialize")
        .arg(f.root.join("mirror-config.json"))
        .output()?;
    ensure!(
        output.status.success(),
        "mirror initialize: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
fn launch(f: &Fixture, seconds: u8) -> Result<Process> {
    Ok(Process(
        command(MIRROR)
            .arg("--foreground")
            .arg(f.root.join("mirror-config.json"))
            .arg(seconds.to_string())
            .spawn()?,
    ))
}
async fn fetch(peer: BootstrapRefreshPeerV1, book: Arc<Mutex<ServiceLeaseBookV1>>) -> Result<()> {
    let config =
        IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)?;
    with_iroh_scope_v1(
        config,
        IrohControlV1::new(Instant::now() + Duration::from_secs(4), || Ok(()))?,
        move |scope| {
            Box::pin(
                async move { refresh_bootstrap_sources_v1(scope, &[peer], &book, now_ms).await },
            )
        },
    )
    .await
}
async fn active(f: &Fixture) -> Result<Vec<BootstrapSourceV1>> {
    read_json(&f.root.join("node/active.sources.json")).await
}

#[tokio::test(flavor = "current_thread")]
async fn real_mirror_serves_active_epoch_then_restores_identity_and_client_cache() -> Result<()> {
    let f = Fixture::new()?;
    let _node = f.launch_node(3)?;
    let first = f.proposal(&BTreeSet::new()).await?;
    f.enroll(first.relay_public_key)?;
    let _publisher = f.launch_publisher(60)?;
    let sources = active(&f).await?;
    ensure!(
        sources[0].manifest.relay_records[0] == first,
        "unactivated epoch exported"
    );
    initialize(&f, now_ms()? + 60000)?;
    let mut mirror = launch(&f, 60)?;
    let peer: BootstrapRefreshPeerV1 = read_json(&f.root.join("mirror/peer.json")).await?;
    let invite: Value = read_json(&f.root.join("mirror/invitation.json")).await?;
    ensure!(
        invite["transport"] == "novovm-bootstrap-v1"
            && invite["trust_anchors"][0] == json!(f.publisher_pin),
        "wrong invitation"
    );
    let old = ServiceLeaseBookV1::new(
        BootstrapTrustPolicyV1::single_signer(f.publisher_pin),
        sources,
        now_ms()?,
    )?;
    let checkpoint = old.checkpoint()?;
    let a = Arc::new(Mutex::new(old));
    let b = Arc::new(Mutex::new(ServiceLeaseBookV1::restore(
        BootstrapTrustPolicyV1::single_signer(f.publisher_pin),
        &checkpoint,
        now_ms()?,
    )?));
    let (one, two) = tokio::join!(
        fetch(peer.clone(), a.clone()),
        fetch(peer.clone(), b.clone())
    );
    one.context("first client refresh")?;
    two.context("second client refresh")?;
    mirror.0.kill()?;
    mirror.0.wait()?;
    std::fs::remove_file(f.root.join("mirror/peer.json"))?;
    let _mirror = launch(&f, 60)?;
    let resumed: BootstrapRefreshPeerV1 = read_json(&f.root.join("mirror/peer.json")).await?;
    ensure!(
        serde_json::to_vec(&peer)? == serde_json::to_vec(&resumed)?,
        "mirror changed its approved identity/address/expiry"
    );
    // Original invitation still dials the SAME service after process restart.
    fetch(peer.clone(), a.clone())
        .await
        .context("A refresh after restart/expiry")?;
    let until = Instant::now() + Duration::from_secs(16);
    loop {
        if now_ms()? > first.expires_at_ms
            && active(&f).await?[0].manifest.relay_records[0].sequence >= first.sequence + 130
        {
            break;
        }
        ensure!(Instant::now() < until, "node renewal deadline");
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
    // Renewal and mirror publication are separate processes. Observe the
    // expected activated generation before testing its RPC; a signed address
    // still within TTL is not a guarantee that a retiring carrier stays open.
    let fresh = Instant::now() + Duration::from_secs(3);
    loop {
        fetch(peer.clone(), a.clone())
            .await
            .context("A fresh generation")?;
        if a.lock().unwrap().routes(now_ms()?)?[0].sequence >= first.sequence + 130 {
            break;
        }
        ensure!(
            Instant::now() < fresh,
            "mirror failed to publish activated successor"
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    fetch(peer, b.clone()).await.context("B expired refresh")?;
    let record = a.lock().unwrap().routes(now_ms()?)?.remove(0);
    ensure!(
        record.sequence > first.sequence && b.lock().unwrap().routes(now_ms()?)?[0] == record,
        "expired clients failed to refresh"
    );
    f.exchange(&record)
        .await
        .context("discovered service RPC after old expiry")?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn signed_revocation_reaches_mirror_and_survives_process_restart() -> Result<()> {
    let f = Fixture::new()?;
    let mut node = f.launch_node(2)?;
    let first = f.proposal(&BTreeSet::new()).await?;
    f.enroll(first.relay_public_key)?;
    let _publisher = f.launch_publisher(30)?;
    let sources = active(&f).await?;
    initialize(&f, now_ms()? + 30000)?;
    let mut mirror = launch(&f, 30)?;
    let peer: BootstrapRefreshPeerV1 = read_json(&f.root.join("mirror/peer.json")).await?;
    let book = Arc::new(Mutex::new(ServiceLeaseBookV1::new(
        BootstrapTrustPolicyV1::single_signer(f.publisher_pin),
        sources.clone(),
        now_ms()?,
    )?));
    fetch(peer.clone(), book.clone()).await?;
    once("--revoke", &f.root.join("publisher-config.json"), true)?;
    node.finish(false, 5).await?;
    ensure!(
        active(&f).await?[0].manifest.relay_records.is_empty(),
        "node failed to export durable revocation"
    );
    tokio::time::sleep(Duration::from_millis(180)).await;
    fetch(peer.clone(), book.clone()).await?;
    ensure!(
        book.lock().unwrap().routes(now_ms()?)?.is_empty(),
        "client retained revoked route"
    );
    mirror.0.kill()?;
    mirror.0.wait()?;
    // Roll back only public input; protected mirror history must reject it.
    json_file(&f.root.join("node/active.sources.json"), &sources)?;
    let mut mirror = launch(&f, 10)?;
    mirror.finish(false, 4).await?;
    ensure!(
        fetch(peer, book).await.is_err(),
        "rolled-back mirror served"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn mirror_permission_expiry_and_mutation_do_not_renew_on_restart() -> Result<()> {
    let f = Fixture::new()?;
    let _node = f.launch_node(2)?;
    let first = f.proposal(&BTreeSet::new()).await?;
    f.enroll(first.relay_public_key)?;
    let _publisher = f.launch_publisher(30)?;
    active(&f).await?;
    initialize(&f, now_ms()? + 1800)?;
    let mut mirror = launch(&f, 10)?;
    let _: BootstrapRefreshPeerV1 = read_json(&f.root.join("mirror/peer.json")).await?;
    mirror.finish(false, 5).await?;
    let mut mirror = launch(&f, 10)?;
    mirror.finish(false, 4).await?;
    let path = f.root.join("mirror-config.json");
    let mut config: Value = read_json(&path).await?;
    config["permission_expires_at_ms"] = json!(now_ms()? + 60000);
    json_file(&path, &config)?;
    let mut mirror = launch(&f, 10)?;
    mirror.finish(false, 4).await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn forged_live_source_stops_distribution_and_missing_keys_never_reenroll() -> Result<()> {
    let f = Fixture::new()?;
    let _node = f.launch_node(2)?;
    let first = f.proposal(&BTreeSet::new()).await?;
    f.enroll(first.relay_public_key)?;
    let _publisher = f.launch_publisher(30)?;
    let mut sources = active(&f).await?;
    initialize(&f, now_ms()? + 30000)?;
    let mut mirror = launch(&f, 30)?;
    let peer: BootstrapRefreshPeerV1 = read_json(&f.root.join("mirror/peer.json")).await?;
    let book = Arc::new(Mutex::new(ServiceLeaseBookV1::new(
        BootstrapTrustPolicyV1::single_signer(f.publisher_pin),
        sources.clone(),
        now_ms()?,
    )?));
    fetch(peer.clone(), book.clone()).await?;
    sources[0].manifest.signatures[0].signature[0] ^= 1;
    json_file(&f.root.join("node/active.sources.json"), &sources)?;
    mirror.finish(false, 4).await?;
    ensure!(
        fetch(peer, book).await.is_err(),
        "bad source fell back to old cached distribution"
    );
    std::fs::remove_file(f.root.join("mirror/keys.dpapi"))?;
    let mut mirror = launch(&f, 10)?;
    mirror.finish(false, 4).await?;
    let result = command(MIRROR)
        .arg("--initialize")
        .arg(f.root.join("mirror-config.json"))
        .output()?;
    ensure!(
        !result.status.success() && !f.root.join("mirror/keys.dpapi").exists(),
        "lost mirror identity regenerated"
    );
    Ok(())
}
