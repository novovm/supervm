//! Shared four-original-node process fixture and historical HTTP comparisons.
//! application::queries and native_load exercise native-only original tick;
//! the retained HTTP tests are not native-release acceptance evidence.
//! Wallets supply signatures, never execution outputs, proposals or QCs.
use super::*;
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
use crate::native_pipeline::service::config::{protocol_commitment, ORIGINAL_PROFILE, PROFILE};
use crate::native_pipeline::service::{GenesisAllocation, GenesisConfig, GenesisValidator};
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};

#[path = "native_product_client.rs"]
mod application;
#[path = "native_product_load.rs"]
mod native_load;

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
        let config: Value = serde_json::from_slice(&fs::read(&self.configs[index])?)?;
        let mut command = Command::new(&self.binary);
        command
            .env_remove("NOVOVM_NATIVE_RESIDENT_CONFIG")
            .env_remove("NOVOVM_NATIVE_RESIDENT_START")
            .env_remove("NOVOVM_NATIVE_RESIDENT_RUN_MS")
            .env_remove("NOVOVM_NATIVE_EXECUTION_CONFIG")
            .env_remove("NOVOVM_NATIVE_EXECUTION_START")
            .env_remove("NOVOVM_ALLOW_LEGACY_HOST_EXECUTION")
            .stdin(Stdio::null())
            .stdout(Stdio::from(fs::File::create(
                self.directory.join(format!("{mode}-{index}.stdout.log")),
            )?))
            .stderr(Stdio::from(fs::File::create(
                self.directory.join(format!("{mode}-{index}.stderr.log")),
            )?));
        if config.get("rpc_addr").is_none_or(Value::is_null) {
            ensure!(
                config["profile"] == ORIGINAL_PROFILE,
                "native fixture must select original tick"
            );
            let chain = config["genesis"]["chain_id"]
                .as_u64()
                .context("native fixture chain id missing")?;
            command
                .env("NOVOVM_NODE_MODE", ORIGINAL_PROFILE)
                .env("NOVOVM_NATIVE_EXECUTION_CONFIG", &self.configs[index])
                .env("NOVOVM_NATIVE_EXECUTION_START", mode)
                .env("NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID", chain.to_string())
                .env("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", "0")
                .env("NOVOVM_NATIVE_PIPELINE_TRANSPORT", "novorudp")
                .env("NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_ENABLED", "1");
            if std::env::var("NOVOVM_NATIVE_LOAD_DIAGNOSTICS").as_deref() == Ok("1") {
                command.env(
                    "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH",
                    self.directory.join(format!("{mode}-{index}.progress.json")),
                );
            }
        } else {
            // Explicit historical comparison only, never fallback after native
            // startup failure. A release test must not open an HTTP observer.
            ensure!(
                cfg!(feature = "historical-http"),
                "HTTP fixture requires historical-http"
            );
            ensure!(
                config["profile"] == PROFILE,
                "historical HTTP fixture profile mismatch"
            );
            command
                .env("NOVOVM_NODE_MODE", PROFILE)
                .env("NOVOVM_NATIVE_RESIDENT_CONFIG", &self.configs[index])
                .env("NOVOVM_NATIVE_RESIDENT_START", mode);
        }
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

fn setup_with_genesis(
    directory: &Path,
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
    let execution_backend = test_execution_backend()?;
    let sockets = (0..4)
        .map(|_| std::net::UdpSocket::bind("127.0.0.1:0"))
        .collect::<std::io::Result<Vec<_>>>()?;
    let udp = sockets
        .iter()
        .map(|s| s.local_addr())
        .collect::<std::io::Result<Vec<_>>>()?;
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
            "execution_backend":execution_backend,
            "udp_addr":udp[index], "peers": (0..4).filter(|peer| *peer != index).map(|peer|
                json!({"public_key":validator_key(peer).verifying_key().to_bytes(),"address":udp[peer]})).collect::<Vec<_>>()
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

/// Test deployment selection only, serialized into the actual product config.
/// CPU and GPU run the identical signed-input/RPC/network/durable fixture.
fn test_execution_backend() -> Result<String> {
    let value = std::env::var("NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND")
        .unwrap_or_else(|_| "cpu_callbacks".into());
    ensure!(
        matches!(value.as_str(), "cpu_callbacks" | "aoem_integer_v7"),
        "invalid explicit resident test execution backend"
    );
    Ok(value)
}

#[test]
#[ignore = "real AOEM and original node binary; 2/4 must not finalize; run alone"]
fn native_product_minority_cannot_publish_and_pending_signature_is_not_cached_by_hash() -> Result<()>
{
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY").context("node binary required")?,
    )
    .canonicalize()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-rpc")
        .join(format!(
            "minority-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    eprintln!("native minority artifacts={}", directory.display());
    let (mut nodes, set) = setup_with_genesis(&directory, binary, genesis()?, 2)?;
    let leader = set.leader(1, 0)?;
    let proposer = (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == leader
        })
        .unwrap();
    let ingress = (proposer + 1) % 4;
    let online = [proposer, ingress];
    for i in online {
        nodes.start(i, "create")?;
    }
    nodes.wait_ready(&online)?;
    let raw = vec![signed(1, 0, 100)?, signed(3, 0, 50)?];
    let hashes = transaction_hashes(&raw)?;
    nodes.submit_batch(ingress, &raw)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let minority = loop {
        nodes.alive()?;
        let status = online
            .iter()
            .map(|i| nodes.rpc(*i, "nov_chainStatus", json!([])))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            status.iter().all(|s| s["head"].is_null()),
            "2/4 published a head"
        );
        if status
            .iter()
            .all(|s| s["executed_batches"].as_u64().unwrap_or(0) > 0)
        {
            break status;
        }
        ensure!(
            Instant::now() < deadline,
            "minority execution timeout: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut bad = raw[0].clone();
    *bad.last_mut().unwrap() ^= 1;
    let bad_reply = nodes.request(
        ingress,
        json!({"jsonrpc":"2.0","id":1,"method":"nov_sendRawTransaction","params":[hex(&bad)]}),
    )?;
    ensure!(
        bad_reply.get("error").is_some(),
        "pending canonical hash bypassed signature check"
    );
    let conflicting=nodes.request(ingress,json!({"jsonrpc":"2.0","id":2,"method":"nov_sendRawTransaction","params":[hex(&signed(1,0,101)?)]}))?;
    ensure!(
        conflicting.get("error").is_some(),
        "same nonce reservation admitted two values"
    );
    let repeated = nodes.submit_batch(ingress, &raw)?;
    ensure!(
        repeated.iter().all(|r| r["finalized"] == false),
        "minority repeat fabricated finality"
    );
    // Start actual missing validators; no synthetic vote, manual QC or reset.
    for i in 0..4 {
        if !online.contains(&i) {
            nodes.start(i, "create")?;
        }
    }
    nodes.wait_ready(&[0, 1, 2, 3])?;
    let receipts = nodes.wait_receipts(&[0, 1, 2, 3], &hashes)?;
    nodes.balances(&[0, 1, 2, 3], &receipts[0], [100, 50])?;
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(
            &json!({"minority":minority,"bad_signature":bad_reply,"nonce_conflict":conflicting,"receipts":receipts}),
        )?,
    )?;
    nodes.stop_all()?;
    Ok(())
}

#[test]
#[ignore = "real AOEM plus NOVOVM_RESIDENT_NODE_BINARY; four original node processes; run alone"]
fn native_product_single_rpc_fees_nonce_finality_and_cold_restart() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY").context("explicit node binary required")?,
    )
    .canonicalize()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-rpc")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    eprintln!("native product RPC artifacts={}", directory.display());
    let (mut nodes, set) = setup_with_genesis(&directory, binary, genesis()?, 2)?;
    for i in 0..4 {
        nodes.start(i, "create")?;
    }
    let initial = nodes.wait_ready(&[0, 1, 2, 3])?;
    let leader = set.leader(1, 0)?;
    let ingress = (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                != leader
        })
        .unwrap();
    let first = vec![signed(1, 0, 100)?, signed(3, 0, 50)?];
    let mut bad = first[0].clone();
    *bad.last_mut().unwrap() ^= 1;
    let bad_reply = nodes.request(
        ingress,
        json!({"jsonrpc":"2.0","id":9,"method":"nov_sendRawTransaction","params":[hex(&bad)]}),
    )?;
    ensure!(
        bad_reply.get("error").is_some(),
        "invalid signature admitted"
    );
    let mut all_hashes = Vec::new();
    let mut receipts = Vec::new();
    let mut admissions = Vec::new();
    for raw in [
        first,
        vec![signed(1, 1, 1_000_001)?, signed(3, 1, 25)?],
        vec![signed(1, 2, 50)?, signed(3, 2, 25)?],
    ] {
        let reply = nodes.submit_batch(ingress, &raw)?;
        ensure!(
            reply.iter().all(|r| r["signature_verified"] == true
                && r["admission_durable"] == false
                && r["executed"] == false
                && r["finalized"] == false),
            "received was misreported as finality"
        );
        admissions.push(reply);
        let hashes = transaction_hashes(&raw)?;
        let done = nodes.wait_receipts(&[0, 1, 2, 3], &hashes)?;
        receipts.extend(done[0].clone());
        all_hashes.extend(hashes);
    }
    ensure!(
        receipts
            .iter()
            .enumerate()
            .all(|(i, r)| r["success"] == (i != 2)),
        "business failure/nonce successor semantics changed: {receipts:?}"
    );
    let balances = nodes.balances(&[0, 1, 2, 3], &receipts, [150, 100])?;
    let statuses = nodes.wait_ready(&[0, 1, 2, 3])?;
    ensure!(
        statuses
            .iter()
            .all(|s| s["head"] == statuses[0]["head"] && s["transport"] == "native_novorudp_apfl"),
        "native heads diverged"
    );
    ensure!(
        statuses
            .iter()
            .all(|s| s["executed_batches"].as_u64().unwrap_or(0) >= 3),
        "every validator must execute, not trust the ingress result"
    );
    let integer = test_execution_backend()? == "aoem_integer_v7";
    for s in &statuses {
        ensure!(
            s["execution_apfl_view_transactions_total"]
                .as_u64()
                .unwrap_or(0)
                >= 6,
            "structural input was not consumed by execution"
        );
        let rows = s["execution_integer_rows_total"]
            .as_u64()
            .context("integer observation missing")?;
        let plans = s["execution_integer_plans_total"]
            .as_u64()
            .context("integer plan observation missing")?;
        ensure!(
            if integer {
                rows >= 6 && plans >= 3
            } else {
                rows == 0 && plans == 0
            },
            "configured backend did not execute: {s}"
        );
    }
    nodes.stop_all()?;
    for i in 0..4 {
        nodes.start(i, "existing")?;
    }
    let recovered = nodes.wait_ready(&[0, 1, 2, 3])?;
    ensure!(
        recovered.iter().all(|s| s["head"] == statuses[0]["head"]),
        "cold recovered head mismatch"
    );
    let cold = nodes.wait_receipts(&[0, 1, 2, 3], &all_hashes)?;
    ensure!(cold[0] == receipts, "cold receipts changed");
    ensure!(
        nodes.balances(&[0, 1, 2, 3], &receipts, [150, 100])? == balances,
        "cold balances changed"
    );
    // A signed replay cannot execute again, while the next nonce can.
    let replay = nodes.rpc(
        ingress,
        "nov_sendRawTransaction",
        json!([hex(&signed(1, 0, 100)?)]),
    )?;
    ensure!(
        replay["finalized"] == true,
        "finalized replay not recovered"
    );
    let next = vec![signed(1, 3, 1)?, signed(3, 3, 1)?];
    nodes.submit_batch(ingress, &next)?;
    let after = nodes.wait_receipts(&[0, 1, 2, 3], &transaction_hashes(&next)?)?;
    ensure!(
        after[0].iter().all(|r| r["success"] == true),
        "post-restart successor failed"
    );
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&json!({
            "profile":PROFILE,"backend":test_execution_backend()?,"single_ingress":ingress,
            "initial":initial,"admissions":admissions,"bad_signature":bad_reply,"receipts":receipts,
            "balances":balances,"before_restart":statuses,"cold":recovered,"after_restart":after,
            "fixture_initial_http_inputs":(0..4).map(|i|if i==ingress {7}else{0}).collect::<Vec<_>>(),
            "real_processes":4,"real_machines":1,"throughput_acceptance":false
        }))?,
    )?;
    nodes.stop_all()?;
    Ok(())
}
