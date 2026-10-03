//! Actual product executable and HTTP RPC, not the controller test worker.
//! This fresh loopback fixture explicitly fans out identical signed inputs;
//! it does not claim mempool gossip, production activation, TPS or four machines.
//! The parent never manufactures a proposal, vote, QC or publication permission.

#[path = "resident_rpc_process/load.rs"]
mod load;

use super::network_integration::Relay;
use super::*;
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
use crate::native_pipeline::service::config::{protocol_commitment, PROFILE};
use crate::native_pipeline::service::{GenesisAllocation, GenesisConfig, GenesisValidator};
use novovm_network::duplex::peer_id_from_ed25519_public_key_v1;
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};

struct ProductNodes {
    binary: PathBuf,
    directory: PathBuf,
    configs: Vec<PathBuf>,
    endpoints: Vec<String>,
    processes: Vec<Option<Child>>,
    agent: ureq::Agent,
}

impl ProductNodes {
    fn start(&mut self, index: usize, mode: &str) -> Result<()> {
        ensure!(self.processes[index].is_none(), "test node already running");
        let mut command = Command::new(&self.binary);
        command
            .env("NOVOVM_NODE_MODE", "native_resident")
            .env("NOVOVM_NATIVE_RESIDENT_CONFIG", &self.configs[index])
            .env("NOVOVM_NATIVE_RESIDENT_START", mode)
            .env_remove("NOVOVM_NATIVE_RESIDENT_RUN_MS")
            .env_remove("NOVOVM_ALLOW_LEGACY_HOST_EXECUTION")
            .stdin(Stdio::null())
            .stdout(Stdio::from(fs::File::create(
                self.directory.join(format!("{mode}-{index}.stdout.log")),
            )?))
            .stderr(Stdio::from(fs::File::create(
                self.directory.join(format!("{mode}-{index}.stderr.log")),
            )?));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // No console window for a test child.
        }
        self.processes[index] = Some(command.spawn().context("start actual novovm-node")?);
        Ok(())
    }

    fn alive(&mut self) -> Result<()> {
        for (index, child) in self.processes.iter_mut().enumerate() {
            if let Some(child) = child {
                ensure!(
                    child.try_wait()?.is_none(),
                    "product node {index} exited; logs={}",
                    self.directory.display()
                );
            }
        }
        Ok(())
    }

    fn stop_all(&mut self) -> Result<()> {
        for process in &mut self.processes {
            if let Some(mut child) = process.take() {
                if child.try_wait()?.is_none() {
                    child.kill()?;
                }
                let _ = child.wait()?;
            }
        }
        Ok(())
    }

    fn request(&self, index: usize, request: Value) -> Result<Value> {
        let response = self
            .agent
            .post(&self.endpoints[index])
            .set("Content-Type", "application/json")
            .send_string(&serde_json::to_string(&request)?)?
            .into_string()?;
        Ok(serde_json::from_str(&response)?)
    }

    fn rpc(&self, index: usize, method: &str, params: Value) -> Result<Value> {
        let response = self.request(
            index,
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),
        )?;
        ensure!(
            response.get("error").is_none(),
            "RPC {method} failed on {index}: {response}"
        );
        response
            .get("result")
            .cloned()
            .context("RPC result missing")
    }

    fn wait_ready(&mut self, indices: &[usize]) -> Result<Vec<Value>> {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            self.alive()?;
            let reports = indices
                .iter()
                .map(|index| self.rpc(*index, "nov_chainStatus", json!([])))
                .collect::<Result<Vec<_>>>();
            if let Ok(reports) = reports {
                if reports.iter().all(|report| {
                    report["recovery_in_progress"] == false
                        && report["projection_error"].is_null()
                        && report["rpc_indexed_height"].as_u64()
                            == Some(report["head"]["height"].as_u64().unwrap_or(0))
                }) {
                    return Ok(reports);
                }
            }
            ensure!(
                Instant::now() < deadline,
                "product RPC startup/recovery timeout; {}",
                self.directory.display()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn submit_batch(&self, index: usize, raw: &[Vec<u8>]) -> Result<Vec<Value>> {
        let requests: Vec<_> = raw.iter().enumerate().map(|(id, raw)|
            json!({"jsonrpc":"2.0","id":id,"method":"nov_sendRawTransaction","params":[hex(raw)]})).collect();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let response = self.request(index, Value::Array(requests.clone()))?;
            let results = response.as_array().context("RPC batch response missing")?;
            ensure!(
                results.len() == raw.len(),
                "RPC batch response count changed"
            );
            let mut catching_up = false;
            for result in results {
                if let Some(error) = result.get("error") {
                    ensure!(
                        error["message"]
                            .as_str()
                            .is_some_and(|message| message.contains("projection catching up")),
                        "signed batch rejected: {result}"
                    );
                    catching_up = true;
                }
            }
            if !catching_up {
                return results
                    .iter()
                    .map(|value| value.get("result").cloned().context("batch result missing"))
                    .collect();
            }
            ensure!(
                Instant::now() < deadline,
                "RPC admission projection did not catch up"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_receipts(&mut self, indices: &[usize], hashes: &[String]) -> Result<Vec<Vec<Value>>> {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            self.alive()?;
            let reports = indices
                .iter()
                .map(|index| {
                    hashes
                        .iter()
                        .map(|hash| self.rpc(*index, "nov_getTransactionStatus", json!([hash])))
                        .collect::<Result<Vec<_>>>()
                })
                .collect::<Result<Vec<_>>>()?;
            if reports
                .iter()
                .flatten()
                .all(|receipt| receipt["finalized"] == true)
            {
                ensure!(
                    reports.iter().all(|receipts| receipts == &reports[0]),
                    "nodes disagree on full user receipts"
                );
                return Ok(reports);
            }
            ensure!(
                Instant::now() < deadline,
                "user receipt timeout; reports={reports:?}; {}",
                self.directory.display()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn balances(
        &mut self,
        indices: &[usize],
        receipts: &[Value],
        transferred: [u128; 2],
    ) -> Result<Vec<Vec<Value>>> {
        let fee = |parity: usize| -> Result<u128> {
            receipts
                .iter()
                .skip(parity)
                .step_by(2)
                .try_fold(0u128, |total, receipt| {
                    total
                        .checked_add(
                            receipt["charged_fee"]
                                .as_str()
                                .context("missing receipt fee")?
                                .parse::<u128>()?,
                        )
                        .context("fixture fee sum overflow")
                })
        };
        let expected = [
            1_000_000 - transferred[0] - fee(0)?,
            transferred[0] + transferred[1],
            1_000_000 - transferred[1] - fee(1)?,
        ];
        let mut snapshots = Vec::new();
        for index in indices {
            let mut balances = Vec::new();
            for (seed, expected) in [1, 2, 3].into_iter().zip(expected) {
                let deadline = Instant::now() + Duration::from_secs(10);
                let balance = loop {
                    self.alive()?;
                    let reply = self.rpc(
                        *index,
                        "nov_getAssetBalance",
                        json!({"account":hex(account(seed).as_bytes()),"asset":"NOV"}),
                    )?;
                    if reply["query_complete"] == true {
                        break reply;
                    }
                    ensure!(
                        Instant::now() < deadline,
                        "product balance readback timeout"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                };
                ensure!(
                    balance["balance"].as_str() == Some(expected.to_string().as_str())
                        && balance["finalized"] == true,
                    "authoritative balance/fee conservation differs: {balance}"
                );
                balances.push(balance);
            }
            snapshots.push(balances);
        }
        ensure!(
            snapshots.iter().all(|balances| balances == &snapshots[0]),
            "four authoritative balance/root readbacks differ"
        );
        Ok(snapshots)
    }
}

impl Drop for ProductNodes {
    fn drop(&mut self) {
        let _ = self.stop_all();
    }
}

fn hex(bytes: &[u8]) -> String {
    crate::native_pipeline::service::rpc::hex(bytes)
}

fn signed(seed: u8, nonce: u64, amount: u128) -> Result<Vec<u8>> {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let mut transaction = TransferV3 {
        chain_id: CHAIN,
        from: account(seed).as_bytes().to_vec(),
        to: account(2).as_bytes().to_vec(),
        asset: "NOV".into(),
        amount,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 0,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    let signature = key.sign(&signing_message(&transaction)?);
    transaction.signature = key.verifying_key().to_bytes().to_vec();
    transaction
        .signature
        .extend_from_slice(&signature.to_bytes());
    encode_transfer_v3(&transaction)
}

fn transaction_hashes(raw: &[Vec<u8>]) -> Result<Vec<String>> {
    raw.iter()
        .map(|raw| Ok(hex(&authenticate_transfer_v3(raw, CHAIN, 1024)?.tx_hash())))
        .collect()
}

fn genesis() -> Result<GenesisConfig> {
    let mut genesis = GenesisConfig {
        chain_id: CHAIN,
        validator_epoch: 1,
        protocol_commitment: protocol_commitment(),
        genesis_config_commitment: [0; 32],
        timestamp_unix_ms: 1_900_000_000_000,
        validators: (0..4)
            .map(|index| GenesisValidator {
                public_key: validator_key(index).verifying_key().to_bytes(),
                weight: 1,
            })
            .collect(),
        allocations: [1, 3]
            .into_iter()
            .map(|seed| GenesisAllocation {
                account_hex: hex(account(seed).as_bytes()),
                amount: "1000000".into(),
            })
            .collect(),
        policy: policy(),
    };
    genesis.genesis_config_commitment = genesis.derive_commitment()?;
    genesis.validate()?;
    Ok(genesis)
}

fn setup(
    directory: &Path,
    relay: &Relay,
    binary: PathBuf,
) -> Result<(ProductNodes, Arc<ValidatorSet>)> {
    setup_with_genesis(directory, relay, binary, genesis()?, 2)
}

fn setup_with_genesis(
    directory: &Path,
    relay: &Relay,
    binary: PathBuf,
    genesis: GenesisConfig,
    batch_size: usize,
) -> Result<(ProductNodes, Arc<ValidatorSet>)> {
    genesis.validate()?;
    ensure!(
        (1..=1024).contains(&batch_size),
        "invalid test RPC batch size"
    );
    let set = Arc::new(ValidatorSet::new(
        CHAIN,
        1,
        1,
        genesis
            .validators
            .iter()
            .map(|validator| Validator::new(validator.public_key, validator.weight))
            .collect::<Result<Vec<_>>>()?,
    )?);
    let library = library()?;
    let relay_identity = SigningKey::from_bytes(&[91; 32]);
    let mut configs = Vec::new();
    let mut endpoints = Vec::new();
    // Reserve the four distinct addresses together, then release them before
    // spawning. Binding failure is a test failure, never a changed quorum.
    let listeners = (0..4)
        .map(|_| TcpListener::bind("127.0.0.1:0"))
        .collect::<std::io::Result<Vec<_>>>()?;
    for (index, listener) in listeners.iter().enumerate() {
        let address: SocketAddr = listener.local_addr()?;
        let key_path = directory.join(format!("test-only-validator-{index}.key"));
        fs::write(&key_path, validator_key(index).to_bytes())?;
        let config_path = directory.join(format!("node-{index}.json"));
        let config = json!({
            "profile":PROFILE,"library":library,"database":directory.join(format!("validator-{index}.rocksdb")),
            "rpc_addr":address,"signing_key_file":key_path,"genesis":genesis,"workers":4,"batch_size":batch_size,
            "relay":{"endpoint":relay.endpoint,"expected_relay_peer_id":peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes()),
                "connect_timeout_ms":2000,"read_timeout_ms":10,"tls_trust":{"explicit_ca":{"certificate_path":relay.certificate}}}
        });
        fs::write(&config_path, serde_json::to_vec_pretty(&config)?)?;
        configs.push(config_path);
        endpoints.push(format!("http://{address}"));
    }
    drop(listeners);
    Ok((
        ProductNodes {
            binary,
            directory: directory.to_owned(),
            configs,
            endpoints,
            processes: (0..4).map(|_| None).collect(),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_millis(100))
                .timeout_read(Duration::from_secs(2))
                .timeout_write(Duration::from_secs(2))
                .build(),
        },
        set,
    ))
}

#[test]
#[ignore = "requires actual NOVOVM_RESIDENT_NODE_BINARY and real AOEM; four product processes and RPC, not TPS"]
fn actual_product_rpc_signed_batches_failures_quorum_and_restart() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit NOVOVM_RESIDENT_NODE_BINARY required")?,
    )
    .canonicalize()?;
    ensure!(binary.is_file(), "actual product binary missing");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/resident-rpc-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    eprintln!("actual product RPC artifacts={}", directory.display());
    let mut relay = Relay::start(&directory.join("relay"))?;
    let (mut nodes, set) = setup(&directory, &relay, binary)?;
    let leader = set.leader(1, 0)?;
    let leader = (0..4)
        .find(|index| {
            Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)
                .is_ok_and(|validator| validator.id() == leader)
        })
        .context("missing scheduled leader")?;
    let mut order = vec![leader];
    order.extend((0..4).filter(|index| *index != leader));
    for index in &order[..2] {
        nodes.start(*index, "create")?;
    }
    nodes.wait_ready(&order[..2])?;
    let first = vec![signed(1, 0, 100)?, signed(3, 0, 50)?];
    let mut all_raw = first.clone();
    let mut all_hashes = transaction_hashes(&first)?;
    let mut admitted = Vec::new();
    for index in &order[..2] {
        admitted.extend(nodes.submit_batch(*index, &first)?);
    }
    ensure!(
        admitted.iter().all(|reply| reply["state"] == "received"
            && reply["admission_durable"] == false
            && reply["executed"] == false
            && reply["finalized"] == false),
        "admission falsely claimed execution or durability"
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let minority = loop {
        nodes.alive()?;
        let reports = order[..2]
            .iter()
            .map(|index| nodes.rpc(*index, "nov_chainStatus", json!([])))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            reports.iter().all(|status| status["head"].is_null()),
            "2/4 published a head"
        );
        if reports
            .iter()
            .all(|status| status["executed_batches"].as_u64().unwrap_or(0) >= 1)
        {
            break reports;
        }
        ensure!(
            Instant::now() < deadline,
            "two nodes did not execute proposed batch"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    fs::write(
        directory.join("two-of-four-no-head.json"),
        serde_json::to_vec_pretty(&minority)?,
    )?;
    nodes.start(order[2], "create")?;
    nodes.wait_ready(&order[2..3])?;
    nodes.submit_batch(order[2], &first)?;
    nodes.wait_receipts(&order[..3], &all_hashes)?;
    nodes.start(order[3], "create")?;
    nodes.wait_ready(&order[3..])?;
    nodes.submit_batch(order[3], &first)?;
    nodes.wait_receipts(&order, &all_hashes)?;

    // Bad signature cannot reserve a nonce or enter execution.
    let mut bad = signed(1, 1, 100)?;
    *bad.last_mut().unwrap() ^= 1;
    ensure!(
        authenticate_transfer_v3(&bad, CHAIN, 1024).is_err(),
        "negative signature fixture became valid"
    );
    let rejected = nodes.request(
        leader,
        json!({"jsonrpc":"2.0","id":8,"method":"nov_sendRawTransaction","params":[hex(&bad)]}),
    )?;
    ensure!(
        rejected.get("error").is_some(),
        "bad signature accepted by product RPC"
    );
    for (nonce, amount) in [(1, 2_000_000), (2, 100)] {
        let raw = vec![signed(1, nonce, amount)?, signed(3, nonce, 50)?];
        for index in &order {
            nodes.submit_batch(*index, &raw)?;
        }
        all_hashes.extend(transaction_hashes(&raw)?);
        all_raw.extend(raw);
        nodes.wait_receipts(&order, &all_hashes)?;
    }
    let before = nodes.wait_receipts(&order, &all_hashes)?;
    let successes = before[0]
        .iter()
        .filter(|receipt| receipt["success"] == true)
        .count();
    let business_failures = before[0]
        .iter()
        .filter(|receipt| receipt["success"] == false)
        .count();
    ensure!(
        successes == 5 && business_failures == 1,
        "business failures were counted as successes"
    );
    for (index, receipt) in before[0].iter().enumerate() {
        ensure!(
            receipt["nonce_after"].as_u64() == Some(index as u64 / 2 + 1)
                && receipt["charged_fee"]
                    .as_str()
                    .context("fee missing")?
                    .parse::<u128>()?
                    > 0
                && receipt["proof_verified"] == false,
            "nonce/fees/proof reporting differs"
        );
    }
    ensure!(
        nodes.submit_batch(leader, &all_raw)? == before[0],
        "exact replay not idempotent"
    );
    let conflicting = signed(1, 0, 101)?;
    let replay = nodes.request(leader, json!({"jsonrpc":"2.0","id":9,"method":"nov_sendRawTransaction","params":[hex(&conflicting)]}))?;
    ensure!(
        replay.get("error").is_some(),
        "different signed nonce replay accepted"
    );
    let statuses = nodes.wait_ready(&order)?;
    ensure!(
        statuses
            .iter()
            .all(|status| status["head"] == statuses[0]["head"]
                && status["head"]["height"] == 3
                && status["head"]["state_version"] == 6
                && status["mempool_gossip"] == false
                && status["pending_survives_restart"] == false),
        "three-height head/profile differs"
    );
    let balances_before = nodes.balances(&order, &before[0], [200, 150])?;
    let live_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
    nodes.stop_all()?;
    for index in &order {
        nodes.start(*index, "existing")?;
    }
    nodes.wait_ready(&order)?;
    let reopened = nodes.wait_receipts(&order, &all_hashes)?;
    ensure!(reopened == before, "cold product RPC receipts changed");
    let balances_reopened = nodes.balances(&order, &reopened[0], [200, 150])?;
    ensure!(
        balances_reopened == balances_before,
        "cold authoritative balances/root changed"
    );
    let cold_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
    ensure!(
        cold_pids.iter().all(|pid| !live_pids.contains(pid)),
        "product did not really restart"
    );
    let final_raw = vec![signed(1, 3, 100)?, signed(3, 3, 50)?];
    for index in &order {
        nodes.submit_batch(*index, &final_raw)?;
    }
    all_hashes.extend(transaction_hashes(&final_raw)?);
    let final_receipts = nodes.wait_receipts(&order, &all_hashes)?;
    ensure!(
        final_receipts[0][..6] == before[0]
            && final_receipts[0][6..]
                .iter()
                .all(|receipt| receipt["success"] == true && receipt["nonce_after"] == 4),
        "restart did not preserve nonce and advance"
    );
    let final_statuses = nodes.wait_ready(&order)?;
    ensure!(
        final_statuses
            .iter()
            .all(|status| status["head"] == final_statuses[0]["head"]
                && status["head"]["height"] == 4
                && status["head"]["state_version"] == 8),
        "post-restart heads differ"
    );
    let final_balances = nodes.balances(&order, &final_receipts[0], [300, 200])?;
    let report = json!({"schema":"novovm/resident-product-rpc-functional/v1", "product_binary":nodes.binary,
        "product_binary_sha256":hex(&Sha256::digest(fs::read(&nodes.binary)?)),
        "aoem_sha256":hex(&Sha256::digest(fs::read(library()?)?)),
        "topology":"one host; four real novovm-node executables; HTTP RPC and WSS/E2E relay",
        "input_distribution":"explicit identical signed RPC fanout, not mempool gossip",
        "legacy_host_permission":false,"live_pids":live_pids,"restarted_pids":cold_pids,
        "two_of_four_no_head":minority,"admission":admitted,"bad_signature_response":rejected,
        "nonce_replay_response":replay,"unique_finalized_transactions":8,"successful_transactions":7,
        "business_failed_transactions":1,"cold_receipts_equal":true,"receipts":final_receipts[0],
        "balances_before_restart":balances_before,"balances_after_restart":balances_reopened,
        "final_balances":final_balances,"final_status":final_statuses,"performance_measured":false,"four_machine_test":false});
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    nodes.stop_all()?;
    relay.shutdown()?;
    eprintln!("actual product RPC: 8 unique finalized (7 success, 1 business failure), 2/4 no head, four-height continuation and four-process cold restart PASS; {}", directory.display());
    Ok(())
}
