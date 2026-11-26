//! Opt-in existing-store regression. Only the detached node's query and cold-read owners are polled.
use super::{ResidentConfig, ResidentNode, RpcLifecycle, StartMode};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct QueryOwner {
    rpc: RpcLifecycle,
    config: ResidentConfig,
}

impl QueryOwner {
    fn poll(&mut self) -> Result<()> {
        let now = Instant::now();
        let wall_ms = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let node = &mut self.rpc.node;
        if node.controller.progress.is_recovering() {
            node.controller.progress.poll_recovery(
                &node.pipeline,
                &self.config.genesis.policy,
                super::super::native_decode_limits(
                    self.config.batch_size,
                    self.config.uca_initialization.is_some(),
                ),
            )?;
            node.controller
                .progress
                .poll_acknowledgements(&node.pipeline)?;
        }
        node.controller.poll_io(wall_ms, now)?;
        self.rpc.poll_projection()?;
        self.rpc.poll_native_query(now)
    }

    fn until(&mut self, mut ready: impl FnMut(&Self) -> Result<bool>) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.poll()?;
            if ready(self)? {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "query snapshot regression deadline"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn active_client(&self) -> Result<(String, u64)> {
        let diagnostics = self
            .rpc
            .node
            .controller
            .consensus_diagnostic(Instant::now());
        let peers = diagnostics["configured_peers"]
            .as_array()
            .context("peer diagnostics")?;
        let mut clients = peers.iter().filter(|row| {
            row["validator"].is_null() && row["session_established"].as_bool() == Some(true)
        });
        let row = clients.next().context("no established query client")?;
        ensure!(
            clients.next().is_none(),
            "more than one dynamic query client"
        );
        Ok((
            row["current_route"]["endpoint"]
                .as_str()
                .context("query client endpoint")?
                .to_owned(),
            row["last_authenticated_age_us"]
                .as_u64()
                .context("last authenticated age")?,
        ))
    }

    fn query(&mut self, binary: &Path, profile: PathBuf, request: &Path) -> Result<Value> {
        let profile_json: Value = serde_json::from_slice(&std::fs::read(&profile)?)?;
        let server = self.config.udp_addr.to_string();
        ensure!(
            profile_json["node"]["address"].as_str() == Some(server.as_str())
                && profile_json["bind_addr"]
                    .as_str()
                    .is_some_and(|bind| bind.starts_with("127.0.0.1:"))
                && profile_json["timeout_ms"]
                    .as_u64()
                    .is_some_and(|timeout| (1..=30_000).contains(&timeout)),
            "client must use an isolated loopback route"
        );
        let mut command = Command::new(binary);
        command.arg(profile).arg("query").arg(request);
        let worker = thread::spawn(move || command.output());
        self.until(|_| Ok(worker.is_finished()))?;
        let output = worker
            .join()
            .map_err(|_| anyhow::anyhow!("query worker panicked"))??;
        ensure!(
            output.status.success(),
            "query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let result = &report["result"];
        ensure!(
            result["kind"] == "node_observation",
            "missing authenticated response"
        );
        let receipt = &result["answer"]["Receipt"];
        ensure!(
            receipt["state"]["Finalized"]["success"] == true,
            "original receipt is not finalized successfully"
        );
        Ok(receipt.clone())
    }
}

fn input(name: &str) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(
        std::env::var_os(name).with_context(|| format!("missing {name}"))?,
    )?)
}

#[test]
#[ignore = "requires a detached original RocksDB copy and two purpose-bound query profiles"]
fn original_snapshot_reclaims_idle_slot_for_another_authenticated_receipt() -> Result<()> {
    let mut config = ResidentConfig::load(&input("NOVOVM_QUERY_COPY_CONFIG")?)?;
    ensure!(
        config.database.is_dir()
            && config.database.file_name().and_then(|name| name.to_str())
                == Some("query-reclaim-copy.rocksdb")
            && config.database.join("NOVOVM-CANDIDATE-DOMAIN").is_file()
            && config.udp_addr.ip().is_loopback()
            && config
                .peers
                .iter()
                .all(|peer| peer.address.ip().is_loopback())
            && config.novonet_config_file.is_none()
            && config.rpc_addr.is_none()
            && config.native_client_limit == 1,
        "requires an isolated copied store, loopback-only routes and one query slot"
    );
    config.database = std::fs::canonicalize(&config.database)?;
    ensure!(
        config.database.file_name().and_then(|name| name.to_str())
            == Some("query-reclaim-copy.rocksdb"),
        "resolved database is not the detached copy"
    );
    let binary = input("NOVOVM_QUERY_COPY_CLIENT")?;
    let request = input("NOVOVM_QUERY_COPY_REQUEST")?;
    let first = input("NOVOVM_QUERY_COPY_FIRST_PROFILE")?;
    let second = input("NOVOVM_QUERY_COPY_SECOND_PROFILE")?;
    let node = ResidentNode::start(config.clone(), StartMode::Existing)?;
    let mut owner = QueryOwner {
        rpc: RpcLifecycle::new(node, config.batch_size)?,
        config,
    };
    owner
        .until(|owner| {
            let core = &owner.rpc.node.controller;
            Ok(!core.is_recovering()
                && owner.rpc.indexed_height > 0
                && owner.rpc.indexed_height == core.parent().height)
        })
        .context("wait for the copied chain's verified receipt projection")?;
    let head = owner.rpc.node.controller.parent();
    let position = (
        owner.rpc.node.controller.progress.context(),
        owner.rpc.node.controller.progress.round(),
    );
    let original = owner.query(&binary, first, &request)?;
    let (old_route, _) = owner.active_client()?;
    owner
        .until(|owner| Ok(owner.active_client()?.1 >= 30_000_000))
        .context("wait for the previous authenticated client to become idle")?;
    let (retained_route, idle_us) = owner.active_client()?;
    ensure!(
        retained_route == old_route,
        "old slot was not retained until authenticated demand"
    );
    let result = owner.query(&binary, second, &request)?;
    let (new_route, _) = owner.active_client()?;
    ensure!(
        old_route != new_route && original == result,
        "slot replacement or receipt mismatch"
    );
    ensure!(
        owner.rpc.node.controller.parent() == head,
        "detached query recovery changed the chain head"
    );
    ensure!(
        position
            == (
                owner.rpc.node.controller.progress.context(),
                owner.rpc.node.controller.progress.round(),
            ),
        "detached queries advanced consensus"
    );
    println!("idle_us={idle_us} old={old_route} new={new_route} receipt={result}");
    owner.rpc.node.shutdown()
}
