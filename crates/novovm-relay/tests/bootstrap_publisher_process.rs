#![cfg(all(windows, feature = "iroh-transport"))]
#[path = "support/bootstrap_fixture.rs"]
mod fixture;
use anyhow::{ensure, Result};
use fixture::*;
use novovm_network::product_rendezvous_daemon::now_ms;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[tokio::test(flavor = "current_thread")]
async fn independent_publisher_auto_authorizes_successor_and_both_process_restarts() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut node = fixture.launch_node(2)?;
    let first = fixture.proposal(&BTreeSet::new()).await?;
    fixture.enroll(first.relay_public_key)?;
    let publisher_keys = std::fs::read(fixture.root.join("publisher/keys.dpapi"))?;
    let mut publisher = fixture.launch_publisher(60)?;
    fixture.exchange(&first).await?;
    let mut seen = BTreeSet::from([first.sequence]);
    let second = fixture.proposal(&seen).await?;
    seen.insert(second.sequence);
    ensure!(
        now_ms()? < first.expires_at_ms,
        "successor not prepared before expiry"
    );
    ensure!(
        second.sequence >= first.sequence + 65,
        "sequence reservation"
    );
    fixture.exchange(&second).await?;
    node.finish(true, 16).await?;
    publisher.0.kill()?;
    publisher.0.wait()?;
    let _publisher = fixture.launch_publisher(30)?;
    let mut node = fixture.launch_node(1)?;
    let third = fixture.proposal(&seen).await?;
    ensure!(
        third.relay_public_key == first.relay_public_key && third.sequence >= second.sequence + 65,
        "node restart identity/sequence"
    );
    ensure!(
        std::fs::read(fixture.root.join("publisher/keys.dpapi"))? == publisher_keys,
        "publisher identity changed"
    );
    fixture.exchange(&third).await?;
    node.finish(true, 14).await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn online_revoke_survives_request_removal_and_both_owner_restarts() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut node = fixture.launch_node(2)?;
    let first = fixture.proposal(&BTreeSet::new()).await?;
    fixture.enroll(first.relay_public_key)?;
    let mut publisher = fixture.launch_publisher(30)?;
    fixture.exchange(&first).await?;
    once(
        "--revoke",
        &fixture.root.join("publisher-config.json"),
        true,
    )?;
    node.finish(false, 5).await?;
    let status: Value = read_json(&fixture.root.join("publisher/status.json")).await?;
    ensure!(
        status["revoked"] == true,
        "publisher did not durably revoke"
    );
    publisher.0.kill()?;
    publisher.0.wait()?;
    std::fs::remove_file(fixture.root.join("publisher/revoke.request"))?;
    let _publisher = fixture.launch_publisher(20)?;
    let mut node = fixture.launch_node(1)?;
    node.finish(false, 5).await?;
    let status: Value = read_json(&fixture.root.join("publisher/status.json")).await?;
    ensure!(status["revoked"] == true, "revoked identity resurrected");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn restart_republishes_exact_durable_grant_and_rejects_equivocation() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut node = fixture.launch_node(1)?;
    let mut first = fixture.proposal(&BTreeSet::new()).await?;
    fixture.enroll(first.relay_public_key)?;
    // Hard crash leaves a real signed proposal, without a live node removing it.
    node.0.kill()?;
    node.0.wait()?;
    let mut publisher = fixture.launch_publisher(30)?;
    let original = serde_json::to_vec(&fixture.source(&first).await?)?;
    publisher.0.kill()?;
    publisher.0.wait()?;
    std::fs::remove_file(fixture.authority_path(first.sequence))?;
    let mut publisher = fixture.launch_publisher(30)?;
    ensure!(
        serde_json::to_vec(&fixture.source(&first).await?)? == original,
        "restart re-signed a committed grant"
    );
    first.signature[0] ^= 1;
    json_file(
        &fixture
            .root
            .join(format!("node/epoch-{}.proposal.json", first.sequence)),
        &first,
    )?;
    publisher.finish(false, 4).await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn forged_proposal_fails_before_any_authority_is_published() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut node = fixture.launch_node(1)?;
    let mut first = fixture.proposal(&BTreeSet::new()).await?;
    fixture.enroll(first.relay_public_key)?;
    node.0.kill()?;
    node.0.wait()?;
    first.signature[0] ^= 1;
    json_file(
        &fixture
            .root
            .join(format!("node/epoch-{}.proposal.json", first.sequence)),
        &first,
    )?;
    let mut publisher = fixture.launch_publisher(10)?;
    publisher.finish(false, 4).await?;
    ensure!(
        !fixture.authority_path(first.sequence).exists(),
        "forged proposal published"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn policy_replacement_and_missing_custody_never_reenroll() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut node = fixture.launch_node(1)?;
    let first = fixture.proposal(&BTreeSet::new()).await?;
    fixture.enroll(first.relay_public_key)?;
    node.0.kill()?;
    node.0.wait()?;
    let config_path = fixture.root.join("publisher-config.json");
    once("--enroll-policy", &config_path, false)?;
    let mut config: Value = read_json(&config_path).await?;
    config["manifest_id"] = json!("unapproved-source-replacement");
    json_file(&config_path, &config)?;
    let mut publisher = fixture.launch_publisher(10)?;
    publisher.finish(false, 4).await?;
    ensure!(
        !fixture.authority_path(first.sequence).exists(),
        "changed policy published"
    );
    std::fs::remove_file(fixture.root.join("publisher/keys.dpapi"))?;
    once("--initialize", &fixture.root.join("publisher"), false)?;
    let mut publisher = fixture.launch_publisher(10)?;
    publisher.finish(false, 4).await?;
    ensure!(
        !fixture.root.join("publisher/keys.dpapi").exists(),
        "lost key silently regenerated"
    );
    Ok(())
}
