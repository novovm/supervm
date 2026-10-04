//! Actual product executable and HTTP RPC, not the controller test worker.
//! Fresh loopback fixtures distinguish explicit client fanout from one fixed
//! HTTP ingress. Neither claims production activation, TPS or four machines.
//! Normal fixtures never manufacture proposals, votes, QCs or publication
//! permission. The explicit A/A/B fixture adds a Byzantine peer signing ONLY
//! its own conflicting proposals; all honest votes and QCs still come from nodes.

#[path = "resident_rpc_process/load.rs"]
mod load;

#[path = "resident_rpc_process/aab.rs"]
mod aab;
#[path = "resident_rpc_process/offline.rs"]
mod offline;

use super::network_integration::Relay;
use super::*;
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
use crate::native_pipeline::service::config::{protocol_commitment, PROFILE};
use crate::native_pipeline::service::{GenesisAllocation, GenesisConfig, GenesisValidator};
use novovm_network::duplex::peer_id_from_ed25519_public_key_v1;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
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

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
struct ReceiptBlockPartition {
    height: u64,
    block_hash: String,
    transactions: usize,
    transaction_hashes: Vec<String>,
}

/// A JSON-RPC array is not an atomic block: rejected signature rows can leave
/// fewer admitted transactions in a bounded graph. Record that fragmentation
/// explicitly instead of either hiding it by reordering input or assuming that
/// every graph produces a full block. The actual product cap remains enforced.
fn receipt_partition(
    receipts: &[Value],
    expected_hashes: &[String],
    batch_cap: usize,
) -> Result<Vec<ReceiptBlockPartition>> {
    ensure!(
        batch_cap > 0 && !receipts.is_empty(),
        "empty receipt partition or zero batch cap"
    );
    let expected: BTreeSet<_> = expected_hashes.iter().cloned().collect();
    ensure!(
        expected.len() == expected_hashes.len() && receipts.len() == expected.len(),
        "receipt partition expected transaction count/uniqueness differs"
    );
    let mut seen = BTreeSet::new();
    let mut blocks = BTreeMap::<u64, ReceiptBlockPartition>::new();
    for receipt in receipts {
        ensure!(
            receipt["finalized"] == true && receipt["executed"] == true,
            "partition contains a non-finalized receipt"
        );
        let tx_hash = receipt["tx_hash"]
            .as_str()
            .context("partition transaction hash missing")?;
        ensure!(
            expected.contains(tx_hash) && seen.insert(tx_hash.to_owned()),
            "receipt partition contains an unexpected or duplicate transaction"
        );
        let height = receipt["block_height"]
            .as_u64()
            .context("partition height missing")?;
        let block_hash = receipt["block_hash"]
            .as_str()
            .context("partition block hash missing")?;
        let block = blocks
            .entry(height)
            .or_insert_with(|| ReceiptBlockPartition {
                height,
                block_hash: block_hash.to_owned(),
                transactions: 0,
                transaction_hashes: Vec::new(),
            });
        ensure!(
            block.block_hash == block_hash,
            "same-height receipt block hashes differ"
        );
        ensure!(
            block.transactions < batch_cap,
            "receipt block exceeds actual batch cap"
        );
        block.transactions += 1;
        block.transaction_hashes.push(tx_hash.to_owned());
    }
    ensure!(
        seen == expected,
        "receipt partition omitted expected transactions"
    );
    let blocks: Vec<_> = blocks.into_values().collect();
    for (index, block) in blocks.iter().enumerate() {
        let height = u64::try_from(index)?
            .checked_add(1)
            .context("partition height overflow")?;
        ensure!(
            block.height == height,
            "receipt partition heights are not contiguous from one"
        );
    }
    Ok(blocks)
}

fn verify_partition_heads(
    statuses: &[Value],
    partition: &[ReceiptBlockPartition],
    state_version: u64,
) -> Result<()> {
    ensure!(
        statuses.len() == 4,
        "partition requires four independent node reports"
    );
    let last = partition.last().context("empty finalized partition")?;
    let head: ParentPoint = serde_json::from_value(statuses[0]["head"].clone())?;
    ensure!(
        statuses
            .iter()
            .all(|status| status["head"] == statuses[0]["head"])
            && head.height == last.height
            && hex(&head.block_hash) == last.block_hash
            && head.state_version == state_version,
        "actual finalized partition/head/state version differs"
    );
    Ok(())
}

fn next_partition_height(partition: &[ReceiptBlockPartition]) -> Result<u64> {
    partition
        .last()
        .context("empty finalized partition")?
        .height
        .checked_add(1)
        .context("partition successor height overflow")
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
    let mut mixed_admission = Vec::new();
    let mut bad = first[0].clone();
    *bad.last_mut().unwrap() ^= 1;
    let conflicting = signed(1, 0, 101)?;
    for index in &order[..2] {
        // A bad row cannot poison following good rows or reserve their nonce.
        // Two valid same-nonce values must still have only one pool winner,
        // even though signatures are now computed concurrently on AOEM.
        let inputs = [&bad, &first[0], &conflicting, &first[1]];
        let mut requests: Vec<_> = inputs.iter().enumerate().map(|(id, raw)|
            json!({"jsonrpc":"2.0","id":id,"method":"nov_sendRawTransaction","params":[hex(raw)]})
        ).collect();
        requests.push(json!({"jsonrpc":"2.0","id":4,"method":"nov_chainStatus","params":[]}));
        let replies = nodes.request(*index, Value::Array(requests))?;
        let rows = replies
            .as_array()
            .context("mixed admission must preserve batch response")?;
        ensure!(
            rows.len() == 5
                && rows[0].get("error").is_some()
                && rows[2].get("error").is_some()
                && rows[1]["result"]["signature_verified"] == true
                && rows[3]["result"]["signature_verified"] == true
                && rows[4]["result"]["pending"] == 2,
            "deferred mixed signature/nonce admission or query ordering changed"
        );
        for (id, row) in rows.iter().enumerate() {
            ensure!(
                row["id"] == id,
                "deferred response was delivered to another request"
            );
        }
        admitted.push(rows[1]["result"].clone());
        admitted.push(rows[3]["result"].clone());
        mixed_admission.push(replies);
    }
    fs::write(
        directory.join("mixed-async-admission.json"),
        serde_json::to_vec_pretty(&mixed_admission)?,
    )?;
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
    // The canonical hash deliberately excludes signature bytes. A pending
    // lookup/cache hit must therefore require identical raw bytes, never just
    // this hash; exercise it while 2/4 cannot finalize the legitimate input.
    let mut pending_bad_signature = first[0].clone();
    *pending_bad_signature.last_mut().unwrap() ^= 1;
    ensure!(
        authenticate_transfer_v3(&pending_bad_signature, CHAIN, 1024).is_err(),
        "pending negative signature fixture became valid"
    );
    let pending_bad_hash = {
        use crate::native_pipeline::ingress::wire::{canonical_tx_hash, decode_transfer_v3};
        hex(&canonical_tx_hash(&decode_transfer_v3(
            &pending_bad_signature,
            1024,
        )?)?)
    };
    ensure!(
        pending_bad_hash == all_hashes[0],
        "negative fixture must collide only at canonical hash"
    );
    let mut pending_signature_rejections = Vec::new();
    for (position, index) in order[..2].iter().enumerate() {
        let before = nodes.rpc(*index, "nov_getTransactionStatus", json!([all_hashes[0]]))?;
        ensure!(
            before["state"] == "received" && before["finalized"] == false,
            "signature-cache counterexample must exercise actual pending input"
        );
        let rejected = nodes.request(
            *index,
            json!({"jsonrpc":"2.0","id":7,
            "method":"nov_sendRawTransaction","params":[hex(&pending_bad_signature)]}),
        )?;
        ensure!(
            rejected.get("error").is_some(),
            "same-hash altered signature bypassed pending authentication"
        );
        let after = nodes.rpc(*index, "nov_getTransactionStatus", json!([all_hashes[0]]))?;
        let status = nodes.rpc(*index, "nov_chainStatus", json!([]))?;
        ensure!(
            after == before
                && status["head"].is_null()
                && status["pending"] == minority[position]["pending"],
            "rejected same-hash signature changed valid pending state"
        );
        pending_signature_rejections.push(json!({"node":index,"canonical_hash":pending_bad_hash,
            "response":rejected,"pending_before":before,"pending_after":after,"status":status}));
    }
    fs::write(
        directory.join("pending-same-hash-bad-signature.json"),
        serde_json::to_vec_pretty(&pending_signature_rejections)?,
    )?;
    nodes.start(order[2], "create")?;
    nodes.wait_ready(&order[2..3])?;
    nodes.submit_batch(order[2], &first)?;
    nodes.wait_receipts(&order[..3], &all_hashes)?;
    nodes.start(order[3], "create")?;
    nodes.wait_ready(&order[3..])?;
    nodes.submit_batch(order[3], &first)?;
    let first_receipts = nodes.wait_receipts(&order, &all_hashes)?;
    ensure!(
        first_receipts[0]
            .iter()
            .all(|receipt| receipt["success"] == true && receipt["nonce_after"] == 1),
        "rejected colliding signature harmed the original transfers"
    );

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
    let before_partition = receipt_partition(&before[0], &all_hashes, 2);
    // Write the observed shape and all four heads before accepting the result,
    // including when a malformed partition or head mismatch fails this test.
    fs::write(
        directory.join("before-restart-head-and-partition.json"),
        serde_json::to_vec_pretty(&json!({"statuses":statuses,"receipts":before,
            "actual_partition":before_partition.as_ref().ok(),
            "partition_error":before_partition.as_ref().err().map(|error|format!("{error:#}")),
            "batch_cap":2,"expected_unique_transactions":6,
            "scope":"actual mixed-admission block shape; no fixed full-block assumption"}))?,
    )?;
    let before_partition = before_partition?;
    verify_partition_heads(&statuses, &before_partition, 6)?;
    ensure!(
        statuses
            .iter()
            .all(|status| status["mempool_gossip"] == true
                && status["pending_survives_restart"] == false),
        "product profile differs"
    );
    // The retired direct-RISC0 product endpoints must not start a hidden owner
    // or change the normal transaction/query path. Historical receipt assets
    // remain available to separate diagnostics, not as an implicit RPC backend.
    let mut retired_proof_responses = Vec::new();
    for (position, index) in order.iter().enumerate() {
        ensure!(
            statuses[position].get("business_proof").is_none(),
            "retired direct-proof service still exposed in chain status"
        );
        for method in ["nov_proveBlock", "nov_getBlockProof"] {
            let response = nodes.request(
                *index,
                json!({"jsonrpc":"2.0","id":10,"method":method,"params":[1]}),
            )?;
            ensure!(
                response.get("result").is_none()
                    && response["error"]["message"]
                        .as_str()
                        .is_some_and(|message| message.contains("method not connected")),
                "retired direct-proof endpoint unexpectedly connected: {response}"
            );
            retired_proof_responses.push(json!({"node":index,"method":method,"response":response}));
        }
        let after = nodes.rpc(*index, "nov_chainStatus", json!([]))?;
        ensure!(
            after["head"] == statuses[position]["head"]
                && after["pending"] == statuses[position]["pending"]
                && after.get("business_proof").is_none(),
            "retired endpoint changed transaction or chain state"
        );
    }
    ensure!(
        nodes.wait_receipts(&order, &all_hashes)? == before,
        "retired endpoint changed existing finalized receipt queries"
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
    let final_partition = receipt_partition(&final_receipts[0], &all_hashes, 2)?;
    verify_partition_heads(&final_statuses, &final_partition, 8)?;
    let next_height = next_partition_height(&before_partition)?;
    ensure!(
        final_partition.len() == before_partition.len() + 1
            && final_partition[..before_partition.len()] == before_partition
            && final_partition
                .last()
                .is_some_and(|block| block.height == next_height && block.transactions == 2)
            && final_receipts[0][6..]
                .iter()
                .all(|receipt| receipt["block_height"] == next_height),
        "post-restart two-transaction batch did not append exactly one full block"
    );
    let final_balances = nodes.balances(&order, &final_receipts[0], [300, 200])?;
    let report = json!({"schema":"novovm/resident-product-rpc-functional/v1", "product_binary":nodes.binary,
        "product_binary_sha256":hex(&Sha256::digest(fs::read(&nodes.binary)?)),
        "aoem_sha256":hex(&Sha256::digest(fs::read(library()?)?)),
        "topology":"one host; four real novovm-node executables; HTTP RPC and WSS/E2E relay",
        "input_distribution":"explicit identical signed RPC fanout; does not independently prove single-ingress propagation",
        "legacy_host_permission":false,"live_pids":live_pids,"restarted_pids":cold_pids,
        "two_of_four_no_head":minority,"admission":admitted,"mixed_async_admission":mixed_admission,"bad_signature_response":rejected,
        "pending_same_hash_bad_signature":pending_signature_rejections,
        "retired_direct_proof_rpc_rejections":retired_proof_responses,
        "nonce_replay_response":replay,"unique_finalized_transactions":8,"successful_transactions":7,
        "business_failed_transactions":1,"cold_receipts_equal":true,"receipts":final_receipts[0],
        "balances_before_restart":balances_before,"balances_after_restart":balances_reopened,
        "before_restart_status":statuses,"before_restart_partition":before_partition,
        "final_partition":final_partition,"batch_cap":2,
        "final_balances":final_balances,"final_status":final_statuses,"performance_measured":false,"four_machine_test":false});
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    nodes.stop_all()?;
    relay.shutdown()?;
    eprintln!("actual product RPC: 8 unique finalized (7 success, 1 business failure), 2/4 no head, actual partition {final_partition:?}, one full-block continuation after four-process cold restart PASS; {}", directory.display());
    Ok(())
}

mod receipt_partition_tests {
    use super::*;

    fn fixture(counts: &[usize]) -> (Vec<Value>, Vec<String>, Vec<Value>) {
        let mut receipts = Vec::new();
        let mut hashes = Vec::new();
        for (index, count) in counts.iter().enumerate() {
            let height = index as u64 + 1;
            for _ in 0..*count {
                let tx_hash = hex(&[(hashes.len() + 1) as u8; 32]);
                receipts.push(json!({"tx_hash":tx_hash,"block_height":height,
                    "block_hash":hex(&[height as u8;32]),"executed":true,"finalized":true}));
                hashes.push(tx_hash);
            }
        }
        let head = ParentPoint {
            height: counts.len() as u64,
            block_hash: [counts.len() as u8; 32],
            state_root: [42; 32],
            receipt_batch_commitment: [43; 32],
            state_version: hashes.len() as u64,
            decision_hash: [44; 32],
        };
        (receipts, hashes, vec![json!({"head":head}); 4])
    }

    #[test]
    fn actual_partition_preserves_visible_fragments_and_full_block_continuation() -> Result<()> {
        let (receipts, hashes, statuses) = fixture(&[1, 1, 2, 2]);
        let partition = receipt_partition(&receipts, &hashes, 2)?;
        assert_eq!(
            partition
                .iter()
                .map(|block| block.transactions)
                .collect::<Vec<_>>(),
            [1, 1, 2, 2]
        );
        verify_partition_heads(&statuses, &partition, 6)?;
        assert_eq!(next_partition_height(&partition)?, 5);
        let (after, after_hashes, after_statuses) = fixture(&[1, 1, 2, 2, 2]);
        let continued = receipt_partition(&after, &after_hashes, 2)?;
        assert_eq!(continued[..partition.len()], partition);
        assert_eq!(continued.last().unwrap().transactions, 2);
        verify_partition_heads(&after_statuses, &continued, 8)
    }

    #[test]
    fn actual_partition_rejects_missing_height_and_genesis_height() {
        let (mut receipts, hashes, _) = fixture(&[1, 1]);
        receipts[1]["block_height"] = json!(3);
        receipts[1]["block_hash"] = json!(hex(&[3; 32]));
        assert!(receipt_partition(&receipts, &hashes, 2)
            .unwrap_err()
            .to_string()
            .contains("contiguous"));
        receipts[1]["block_height"] = json!(u64::MAX);
        assert!(receipt_partition(&receipts, &hashes, 2).is_err());
        receipts[1]["block_height"] = json!(0);
        assert!(receipt_partition(&receipts, &hashes, 2).is_err());
    }

    #[test]
    fn actual_partition_rejects_same_height_hash_conflict_and_duplicate_transaction() {
        let (receipts, hashes, _) = fixture(&[2]);
        let mut conflict = receipts.clone();
        conflict[1]["block_hash"] = json!(hex(&[99; 32]));
        assert!(receipt_partition(&conflict, &hashes, 2)
            .unwrap_err()
            .to_string()
            .contains("same-height"));
        let mut duplicate = receipts;
        duplicate[1]["tx_hash"] = duplicate[0]["tx_hash"].clone();
        assert!(receipt_partition(&duplicate, &hashes, 2)
            .unwrap_err()
            .to_string()
            .contains("duplicate"));
    }

    #[test]
    fn actual_partition_rejects_overfull_block_and_successor_overflow() {
        let (receipts, hashes, _) = fixture(&[3]);
        assert!(receipt_partition(&receipts, &hashes, 2)
            .unwrap_err()
            .to_string()
            .contains("batch cap"));
        let mut partition = receipt_partition(&receipts, &hashes, 3).unwrap();
        partition[0].height = u64::MAX;
        assert!(next_partition_height(&partition)
            .unwrap_err()
            .to_string()
            .contains("overflow"));
        assert!(next_partition_height(&[]).is_err());
    }

    #[test]
    fn actual_partition_requires_all_four_heads_exact_last_block_and_state_version() -> Result<()> {
        let (receipts, hashes, statuses) = fixture(&[1, 1, 2, 2]);
        let partition = receipt_partition(&receipts, &hashes, 2)?;
        assert!(verify_partition_heads(&statuses[..3], &partition, 6).is_err());
        assert!(verify_partition_heads(&statuses, &partition, 7).is_err());
        let mut disagreement = statuses.clone();
        disagreement[3]["head"]["block_hash"] = json!(vec![99_u8; 32]);
        assert!(verify_partition_heads(&disagreement, &partition, 6).is_err());
        let mut wrong_head = statuses;
        for status in &mut wrong_head {
            status["head"]["block_hash"] = json!(vec![99_u8; 32]);
        }
        assert!(verify_partition_heads(&wrong_head, &partition, 6).is_err());
        Ok(())
    }
}

// A rejected local request must not mint a propagation enqueue. This observes
// the ingress controller's actual enqueue counter, not merely absence from a
// peer's recent query index. Peers and the authoritative head are also checked.
fn reject_without_broadcast(nodes: &mut ProductNodes, ingress: usize, raw: &[u8]) -> Result<Value> {
    use crate::native_pipeline::ingress::wire::{canonical_tx_hash, decode_transfer_v3};
    let indices = [0, 1, 2, 3];
    let before = nodes.wait_ready(&indices)?;
    ensure!(
        before.iter().all(|status| status["pending"] == 0),
        "negative fixture requires empty pools"
    );
    let hash = hex(&canonical_tx_hash(&decode_transfer_v3(raw, 1024)?)?);
    let response = nodes.request(
        ingress,
        json!({"jsonrpc":"2.0","id":1,
        "method":"nov_sendRawTransaction","params":[hex(raw)]}),
    )?;
    ensure!(
        response.get("error").is_some(),
        "negative signed input was accepted"
    );
    let after = nodes.wait_ready(&indices)?;
    let queued = |status: &Value| -> Result<u64> {
        status["transaction_gossip"]["outbound_batches_accepted"]
            .as_u64()
            .context("real transaction propagation enqueue counter missing")
    };
    ensure!(
        queued(&before[ingress])? == queued(&after[ingress])?,
        "rejected input was authorized for propagation"
    );
    ensure!(
        after
            .iter()
            .zip(&before)
            .all(|(after, before)| after["pending"] == 0 && after["head"] == before["head"]),
        "rejected input changed pending or the finalized head"
    );
    let lookups = indices
        .iter()
        .map(|&node| nodes.rpc(node, "nov_getTransactionStatus", json!([hash])))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        lookups
            .iter()
            .all(|status| status["state"] == "not_in_recent_index" && status["finalized"] == false),
        "rejected input appeared on a node"
    );
    Ok(
        json!({"hash":hash,"response":response,"before":before,"after":after,
        "lookups":lookups,"ingress_enqueue_unchanged":true}),
    )
}

#[test]
#[ignore = "requires actual NOVOVM_RESIDENT_NODE_BINARY and real AOEM; one HTTP ingress, four real product nodes and cold restart; run alone"]
fn actual_product_rpc_single_ingress_gossip_failures_and_restart() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit NOVOVM_RESIDENT_NODE_BINARY required")?,
    )
    .canonicalize()?;
    ensure!(binary.is_file(), "actual product binary missing");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/resident-rpc-single-ingress-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    eprintln!(
        "single-ingress product RPC artifacts={}",
        directory.display()
    );
    let mut evidence = json!({});
    let mut phase = "startup";
    let result = (|| -> Result<()> {
        let mut relay = Relay::start(&directory.join("relay"))?;
        let (mut nodes, set) = setup(&directory, &relay, binary.clone())?;
        let leader = set.leader(1, 0)?;
        let leader = (0..4)
            .find(|&index| {
                Validator::new(validator_key(index).verifying_key().to_bytes(), 1)
                    .is_ok_and(|validator| validator.id() == leader)
            })
            .context("missing leader")?;
        let indices = [0, 1, 2, 3];
        evidence["round_zero_scheduled_proposer"] = json!(leader);
        for &node in &indices {
            nodes.start(node, "create")?;
        }
        let initial = nodes.wait_ready(&indices)?;
        ensure!(
            initial.iter().all(|status| status["head"].is_null()
                && status["mempool_gossip"] == true
                && status["pending_survives_restart"] == false),
            "single-ingress propagation profile not active"
        );
        // Pick a current non-proposer from observed status, not the assumed
        // round-zero leader: startup may already have advanced the pacemaker.
        let ingress = initial
            .iter()
            .position(|status| status["local_proposer"] == false)
            .context("no observed non-proposer ingress")?;
        evidence["ingress_node"] = json!(ingress);
        evidence["ingress_was_non_proposer_at_selection"] = json!(true);
        evidence["initial_status"] = json!(initial);
        let mut all_raw = Vec::new();
        let mut all_hashes = Vec::new();
        phase = "single-ingress finalized prefix";
        let first = vec![signed(1, 0, 100)?, signed(3, 0, 50)?];
        let admission = nodes.submit_batch(ingress, &first)?;
        ensure!(
            admission
                .iter()
                .all(|receipt| receipt["state"] == "received"
                    && receipt["admission_durable"] == false
                    && receipt["executed"] == false
                    && receipt["finalized"] == false),
            "single ingress claimed premature finality"
        );
        evidence["initial_admission"] = json!(admission);
        all_hashes.extend(transaction_hashes(&first)?);
        all_raw.extend(first);
        nodes.wait_receipts(&indices, &all_hashes)?;

        phase = "rejected signature and stale nonce cannot broadcast";
        let mut bad = signed(1, 1, 100)?;
        *bad.last_mut().unwrap() ^= 1;
        ensure!(
            authenticate_transfer_v3(&bad, CHAIN, 1024).is_err(),
            "negative signature valid"
        );
        evidence["bad_signature"] = reject_without_broadcast(&mut nodes, ingress, &bad)?;
        let conflict = signed(1, 0, 101)?;
        ensure!(
            authenticate_transfer_v3(&conflict, CHAIN, 1024).is_ok(),
            "nonce negative must have valid signature"
        );
        evidence["stale_nonce"] = reject_without_broadcast(&mut nodes, ingress, &conflict)?;

        phase = "business failure, fees and nonce continuation";
        for (nonce, amount) in [(1, 2_000_000), (2, 100)] {
            let raw = vec![signed(1, nonce, amount)?, signed(3, nonce, 50)?];
            nodes.submit_batch(ingress, &raw)?;
            all_hashes.extend(transaction_hashes(&raw)?);
            all_raw.extend(raw);
            nodes.wait_receipts(&indices, &all_hashes)?;
        }
        let before = nodes.wait_receipts(&indices, &all_hashes)?;
        ensure!(
            before[0].iter().filter(|r| r["success"] == true).count() == 5
                && before[0].iter().filter(|r| r["success"] == false).count() == 1,
            "single-ingress business failure count differs"
        );
        for (index, receipt) in before[0].iter().enumerate() {
            ensure!(
                receipt["nonce_after"].as_u64() == Some(index as u64 / 2 + 1)
                    && receipt["charged_fee"]
                        .as_str()
                        .context("fee missing")?
                        .parse::<u128>()?
                        > 0
                    && receipt["proof_verified"] == false
                    && receipt["finality_kind"] == "BFT_durable",
                "single-ingress receipt economics/finality differs"
            );
        }
        let balances_before = nodes.balances(&indices, &before[0], [200, 150])?;
        let statuses = nodes.wait_ready(&indices)?;
        ensure!(
            statuses.iter().all(|s| s["head"] == statuses[0]["head"]
                && s["head"]["state_version"] == 6
                && s["pending"] == 0),
            "single-ingress heads differ"
        );
        for (node, status) in statuses.iter().enumerate() {
            if node != ingress {
                ensure!(
                    status["transaction_gossip"]["inbound_batches"]
                        .as_u64()
                        .unwrap_or(0)
                        > 0,
                    "non-ingress peer received no raw propagation batches"
                );
            }
        }
        // Exact replay is served from finalized receipts, not repropagated.
        let replay_counter =
            statuses[ingress]["transaction_gossip"]["outbound_batches_accepted"].clone();
        ensure!(
            nodes.submit_batch(ingress, &all_raw)? == before[0],
            "single-ingress replay changed receipts"
        );
        let replay_status = nodes.rpc(ingress, "nov_chainStatus", json!([]))?;
        ensure!(
            replay_status["transaction_gossip"]["outbound_batches_accepted"] == replay_counter,
            "finalized replay re-authorized propagation"
        );
        evidence["before_restart_status"] = json!(statuses);
        let live_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        nodes.stop_all()?;
        phase = "four-store cold restart";
        for &node in &indices {
            nodes.start(node, "existing")?;
        }
        let cold_status = nodes.wait_ready(&indices)?;
        ensure!(
            cold_status.iter().all(|s| s["head"] == statuses[0]["head"]),
            "cold single-ingress head changed"
        );
        let cold_receipts = nodes.wait_receipts(&indices, &all_hashes)?;
        ensure!(
            cold_receipts == before,
            "cold single-ingress receipts changed"
        );
        let balances_cold = nodes.balances(&indices, &cold_receipts[0], [200, 150])?;
        ensure!(
            balances_cold == balances_before,
            "cold single-ingress balances/root changed"
        );
        let cold_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        ensure!(
            cold_pids.iter().all(|pid| !live_pids.contains(pid)),
            "processes did not cold restart"
        );
        phase = "same single ingress continues after restart";
        let final_raw = vec![signed(1, 3, 100)?, signed(3, 3, 50)?];
        nodes.submit_batch(ingress, &final_raw)?;
        all_hashes.extend(transaction_hashes(&final_raw)?);
        let final_receipts = nodes.wait_receipts(&indices, &all_hashes)?;
        ensure!(
            final_receipts[0][..6] == before[0]
                && final_receipts[0][6..]
                    .iter()
                    .all(|r| r["success"] == true && r["nonce_after"] == 4),
            "post-restart nonce continuation differs"
        );
        let final_status = nodes.wait_ready(&indices)?;
        ensure!(
            final_status
                .iter()
                .all(|s| s["head"] == final_status[0]["head"]
                    && s["head"]["state_version"] == 8
                    && s["pending"] == 0),
            "post-restart four heads differ"
        );
        let final_balances = nodes.balances(&indices, &final_receipts[0], [300, 200])?;
        evidence["live_pids"] = json!(live_pids);
        evidence["cold_pids"] = json!(cold_pids);
        evidence["cold_status"] = json!(cold_status);
        evidence["final_status"] = json!(final_status);
        evidence["receipts"] = json!(final_receipts[0]);
        evidence["balances_before_restart"] = json!(balances_before);
        evidence["balances_after_restart"] = json!(balances_cold);
        evidence["final_balances"] = json!(final_balances);
        nodes.stop_all()?;
        relay.shutdown()?;
        phase = "complete";
        Ok(())
    })();
    let report = json!({"schema":"novovm/resident-product-rpc-single-ingress/v1","passed":result.is_ok(),
        "failure":result.as_ref().err().map(|e|format!("{e:#}")),"phase":phase,"evidence":evidence,
        "product_binary":binary,"product_binary_sha256":hex(&Sha256::digest(fs::read(&binary)?)),
        "aoem_sha256":hex(&Sha256::digest(fs::read(library()?)?)),
        "distribution":"one fixed non-proposer HTTP ingress; peer raw transaction propagation; all four queried",
        "expected_unique_finalized":8,"expected_successful":7,"expected_business_failed":1,
        "legacy_host_permission":false,"pending_durable":false,"performance_measured":false,
        "four_machine_test":false,"production_acceptance":false});
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    eprintln!(
        "single-ingress product RPC report={}",
        directory.join("result.json").display()
    );
    result
}
