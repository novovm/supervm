//! Opt-in three-host qualification driver. All writes belong to a fresh test domain.
use super::*;
use crate::native_pipeline::consensus::chain::ChainRecord;
use novovm_protocol::native_query::{Answer, Query};
use serde::Deserialize;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

const ROLES: [&str; 4] = ["A1", "A2", "C1", "D1"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Slot {
    role: String,
    address: SocketAddr,
    validator: Vec<u8>,
    transport: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    run_id: String,
    chain_id: u64,
    start_unix: u64,
    end_unix: u64,
    node_sha256: String,
    aoem_sha256: String,
    harness_sha256: String,
    ctl_sha256: String,
    slots: Vec<Slot>,
}

fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn plain_hex(bytes: &[u8]) -> String {
    hex(bytes)[2..].to_owned()
}

fn pin(path: &Path, expected: &str) -> Result<()> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    ensure!(
        plain_hex(&hash.finalize()) == expected,
        "SHA-256 mismatch for {}",
        path.display()
    );
    Ok(())
}

fn output(path: &Path, value: Value) -> Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(&value)?)?;
    file.sync_all()?;
    Ok(())
}

impl Plan {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.run_id.len() == 32
                && self
                    .run_id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                && (7_000_000_000..8_000_000_000).contains(&self.chain_id),
            "expected a new test run id and isolated chain id"
        );
        ensure!(
            (60..=1800).contains(&self.end_unix.saturating_sub(self.start_unix)),
            "test window must be 60–1800 seconds"
        );
        ensure!(
            [
                &self.node_sha256,
                &self.aoem_sha256,
                &self.harness_sha256,
                &self.ctl_sha256
            ]
            .iter()
            .all(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }),
            "invalid binary pins"
        );
        ensure!(self.slots.len() == 4, "four test slots required");
        let mut addresses = HashSet::new();
        let mut keys = HashSet::new();
        for (role, slot) in ROLES.into_iter().zip(&self.slots) {
            let IpAddr::V4(ip) = slot.address.ip() else {
                anyhow::bail!("IPv4 LAN endpoints required");
            };
            ensure!(
                slot.role == role
                    && ip.is_private()
                    && (45_000..50_000).contains(&slot.address.port())
                    && addresses.insert(slot.address),
                "invalid or duplicate endpoint for {role}"
            );
            for key in [&slot.validator, &slot.transport] {
                ensure!(
                    key.len() == 1952 && keys.insert(key),
                    "invalid or duplicate key for {role}"
                );
            }
        }
        ensure!(
            self.slots[0].address.ip() == self.slots[1].address.ip()
                && self
                    .slots
                    .iter()
                    .map(|slot| slot.address.ip())
                    .collect::<HashSet<_>>()
                    .len()
                    == 3,
            "A1/A2 must share one address and C1/D1 must use two other addresses"
        );
        Ok(())
    }

    fn slot(&self, role: &str) -> Result<&Slot> {
        self.slots
            .iter()
            .find(|slot| slot.role == role)
            .context("unknown slot")
    }

    fn require_window(&self) -> Result<()> {
        ensure!(
            (self.start_unix..self.end_unix).contains(&now()?),
            "outside the agreed UTC window"
        );
        Ok(())
    }
}

fn env_path(name: &str) -> Result<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .with_context(|| format!("missing {name}"))
}

fn isolated_directory(path: &Path, kind: &str) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
        "{kind} must be a real directory"
    );
    Ok(())
}

fn local_config(
    plan: &Plan,
    slot: &Slot,
    directory: &Path,
    library: &Path,
    genesis: &GenesisConfig,
) -> Value {
    json!({
        "profile": ORIGINAL_PROFILE,
        "library": library,
        "database": directory.join("isolated-validator.rocksdb"),
        "signing_key_file": directory.join("validator.sealed"),
        "transport_key_file": directory.join("transport.sealed"),
        "key_storage": "protected_v1",
        "udp_addr": slot.address,
        "peers": plan.slots.iter().filter(|peer| peer.role != slot.role).map(|peer| {
            json!({"public_key":peer.validator,"address":peer.address})
        }).collect::<Vec<_>>(),
        "genesis": genesis,
        "workers": 4,
        "execution_backend": "cpu_callbacks",
        "native_client_limit": 2,
        "batch_size": 2
    })
}

fn marker(chain_id: u64, nonce: u64) -> Result<(Vec<Vec<u8>>, Vec<[u8; 32]>)> {
    ensure!(nonce <= 45, "marker nonce must be 0–45");
    let amount = if nonce == 7 { 1_000_001 } else { 100 };
    let raw = vec![
        signed_in_chain(chain_id, 1, nonce, amount)?,
        signed_in_chain(chain_id, 3, nonce, 50)?,
    ];
    let hashes = raw
        .iter()
        .map(|bytes| Ok(authenticate_transfer_v3(bytes, chain_id, 1024)?.tx_hash()))
        .collect::<Result<Vec<_>>>()?;
    Ok((raw, hashes))
}

fn archive(config: &ResidentConfig, set: &ValidatorSet, height: u64) -> Result<Value> {
    isolated_directory(&config.database, "test database")?;
    let store = CandidateStore::open(
        StoreConfig {
            library: config.library.clone(),
            database: config.database.clone(),
            domain: StorageDomain {
                chain_id: config.genesis.chain_id,
                genesis_config_commitment: config.genesis.genesis_config_commitment,
                protocol_commitment: config.genesis.protocol_commitment,
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        },
        OpenMode::Existing,
    )?;
    let read = |key| -> Result<Vec<u8>> {
        store
            .read_metadata(&[key])?
            .values
            .pop()
            .flatten()
            .context("archive field absent")
    };
    let record_bytes = read(MetaKey::ChainBlock { height })?;
    let record = ChainRecord::decode(&record_bytes)?;
    let point = record.point();
    ensure!(point.height == height, "archive height mismatch");
    let outbox = read(record.outbox_key())?;
    let (proposal, qc) = record.checked_outbox(&outbox, set)?;
    let verified = qc.verify(set)?;
    Ok(json!({
        "height":height, "round":proposal.round, "block_hash":plain_hex(&point.block_hash),
        "state_root":plain_hex(&point.state_root), "signed_weight":verified.signed_weight(),
        "receipt_batch_commitment":plain_hex(&point.receipt_batch_commitment),
        "quorum_weight":set.quorum_weight(), "signatures":qc.votes.len(),
        "signers":qc.votes.iter().map(|vote| plain_hex(&vote.validator_id)).collect::<Vec<_>>(),
        "record_sha256":plain_hex(&Sha256::digest(record_bytes)),
        "outbox_sha256":plain_hex(&Sha256::digest(outbox))
    }))
}

#[test]
#[ignore = "explicit, isolated Windows multi-host qualification"]
fn native_distributed_validator_fixture() -> Result<()> {
    let action = std::env::var("NOVOVM_CATCHUP_ACTION")?;
    let plan_path = env_path("NOVOVM_CATCHUP_PLAN")?;
    let plan_bytes = fs::read(&plan_path)?;
    let plan_sha256 = plain_hex(&Sha256::digest(&plan_bytes));
    ensure!(
        plan_sha256 == std::env::var("NOVOVM_CATCHUP_PLAN_SHA256")?,
        "plan SHA-256 mismatch"
    );
    let plan: Plan = serde_json::from_slice(&plan_bytes)?;
    plan.validate()?;
    let genesis = fixture_genesis(
        plan.chain_id,
        plan.slots
            .iter()
            .map(|slot| GenesisValidator {
                public_key: slot.validator.clone(),
                transport_public_key: slot.transport.clone(),
                weight: 1,
            })
            .collect(),
    )?;
    let report = env_path("NOVOVM_CATCHUP_REPORT")?;
    if action == "validate" {
        return output(
            &report,
            json!({"plan_sha256":plan_sha256,"genesis":genesis}),
        );
    }

    pin(&std::env::current_exe()?, &plan.harness_sha256)?;
    let library = env_path("NOVOVM_CATCHUP_AOEM")?;
    pin(&library, &plan.aoem_sha256)?;
    let root = env_path("LOCALAPPDATA")?
        .join("NOVOVM-Isolated-Catchup")
        .join(&plan.run_id);
    isolated_directory(
        root.parent().context("no fixture parent")?,
        "fixture parent",
    )?;
    isolated_directory(&root, "run directory")?;
    let role = std::env::var("NOVOVM_CATCHUP_ROLE")?;
    let slot = plan.slot(&role)?;
    let directory = root.join(&role);
    let config_doc = local_config(&plan, slot, &directory, &library, &genesis);
    let config: ResidentConfig = serde_json::from_value(config_doc.clone())?;
    if matches!(
        action.as_str(),
        "create" | "existing" | "archive" | "preflight"
    ) {
        config.validate()?;
    }
    if action == "preflight" {
        isolated_directory(&directory, "validator slot")?;
        pin(&env_path("NOVOVM_CATCHUP_NODE")?, &plan.node_sha256)?;
        UdpSocket::bind(slot.address).context("expected local test UDP endpoint unavailable")?;
        return output(
            &report,
            json!({"plan_sha256":plan_sha256,"genesis":genesis}),
        );
    }
    let mut evidence = json!({
        "action":action, "plan_sha256":plan_sha256,"run_id":plan.run_id,"role":role,
        "chain_id":plan.chain_id,
        "genesis":plain_hex(&genesis.genesis_config_commitment),
        "protocol":plain_hex(&genesis.protocol_commitment)
    });
    if action != "query" {
        evidence["observed_unix"] = json!(now()?);
    }
    if matches!(action.as_str(), "create" | "existing") {
        isolated_directory(&directory, "validator slot")?;
        plan.require_window()?;
        let binary = env_path("NOVOVM_CATCHUP_NODE")?;
        pin(&binary, &plan.node_sha256)?;
        let config_path = directory.join("resident.json");
        let stop = directory.join("stop");
        ensure!(
            !stop.exists(),
            "remove the prior stop marker after the prior process exits"
        );
        UdpSocket::bind(slot.address).context("expected local test UDP endpoint unavailable")?;
        let launch_lock = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("launch.lock"))
            .context("slot already owned; verify previous owner before clearing a stale lock")?;
        let mut nodes = ProductNodes {
            binary,
            directory: directory.clone(),
            configs: vec![config_path.clone()],
            endpoints: Vec::new(),
            processes: vec![None],
            agent: ureq::Agent::new_with_defaults(),
        };
        let result = (|| -> Result<()> {
            if action == "create" {
                ensure!(
                    !config.database.exists(),
                    "create requires a fresh test database"
                );
                output(&config_path, config_doc)?;
            } else {
                isolated_directory(&config.database, "existing test database")?;
                ensure!(
                    serde_json::from_slice::<Value>(&fs::read(&config_path)?)? == config_doc,
                    "existing requires the exact prior config and original test database"
                );
            }
            nodes.start_with_progress(0, &action, Some(500))?;
            evidence["started_unix"] = json!(now()?);
            evidence["node_pid"] =
                json!(nodes.processes[0].as_ref().context("no test child")?.id());
            while now()? < plan.end_unix && !stop.exists() {
                nodes.alive()?;
                std::thread::sleep(Duration::from_millis(100));
            }
            nodes.stop_all()?;
            evidence["stop_reason"] = json!(if stop.exists() {
                "operator_marker"
            } else {
                "window_expired"
            });
            evidence["stopped_unix"] = json!(now()?);
            let progress = directory.join(format!("{action}-0.progress.json"));
            if progress.is_file() {
                let snapshot = report.with_extension("progress.json");
                let bytes = fs::read(progress)?;
                output(&snapshot, serde_json::from_slice(&bytes)?)?;
                evidence["progress_sha256"] =
                    json!(plain_hex(&Sha256::digest(fs::read(snapshot)?)));
            }
            output(&report, evidence)?;
            if stop.exists() {
                fs::remove_file(&stop)?;
            }
            Ok(())
        })();
        drop(nodes);
        drop(launch_lock);
        fs::remove_file(directory.join("launch.lock"))?;
        return result;
    }

    let facade = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
    let crypto = Arc::new(AoemConsensusCrypto::new(&facade)?);
    let set = Arc::new(ValidatorSet::new_with_verifier(
        plan.chain_id,
        1,
        1,
        genesis
            .validators
            .iter()
            .map(|validator| Validator::new(&validator.public_key, validator.weight))
            .collect::<Result<Vec<_>>>()?,
        crypto,
    )?);
    ensure!(
        set.total_weight() == 4 && set.quorum_weight() == 3,
        "test quorum changed"
    );
    if action == "archive" {
        isolated_directory(&directory, "validator slot")?;
        let lock_path = directory.join("launch.lock");
        let lock = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .context("stop the owned test child before archive verification")?;
        let result = (|| -> Result<()> {
            let height = std::env::var("NOVOVM_CATCHUP_NUMBER")?.parse()?;
            evidence["archive"] = archive(&config, &set, height)?;
            output(&report, evidence)
        })();
        drop(lock);
        fs::remove_file(lock_path)?;
        return result;
    }

    ensure!(
        matches!(action.as_str(), "query" | "submit"),
        "unknown action"
    );
    plan.require_window()?;
    // Leave the full native transport and query timeout inside the agreed window.
    ensure!(
        plan.end_unix.saturating_sub(now()?) > 35,
        "test window is closing"
    );
    let actor = plan.slot(&std::env::var("NOVOVM_CATCHUP_ACTOR")?)?;
    ensure!(
        actor.role != "A2",
        "use the co-located A1 operator identity"
    );
    ensure!(
        action != "submit" || actor.role == "A1",
        "A1 is the sole marker submitter"
    );
    let actor_dir = root.join(&actor.role);
    isolated_directory(&actor_dir, "operator slot")?;
    let identity = novovm_relay::transport_identity::load_transport_identity(
        &actor_dir.join("operator.sealed"),
        plan.chain_id,
        &library,
        &plan.aoem_sha256,
    )?;
    let mut client = connect_fixture(
        &config,
        set,
        identity,
        &slot.transport,
        SocketAddr::new(actor.address.ip(), 0),
    )?;
    let number = std::env::var("NOVOVM_CATCHUP_NUMBER")?;
    evidence["operator"] = json!(actor.role);
    if action == "submit" {
        let nonce: u64 = number.parse()?;
        let (raw, hashes) = marker(plan.chain_id, nonce)?;
        evidence["nonce"] = json!(nonce);
        evidence["marker_hashes"] = json!(hashes
            .iter()
            .map(|hash| plain_hex(hash))
            .collect::<Vec<_>>());
        output(
            &actor_dir.join(format!("submitted-{nonce}.json")),
            evidence.clone(),
        )?;
        send(&mut client, &raw)?;
        evidence["transport_completed"] = json!(true);
    } else {
        let query_started = now()?;
        let Answer::Status(status) = queries::query(&mut client, Query::Status)? else {
            anyhow::bail!("expected authenticated Status");
        };
        ensure!(!status.projection_failed, "node reports projection failure");
        evidence["status"] = json!(status);
        if number != "-" {
            let nonce: u64 = number.parse()?;
            let (_, hashes) = marker(plan.chain_id, nonce)?;
            let mut receipts = Vec::new();
            for hash in hashes {
                let Answer::Receipt {
                    state,
                    indexed_height,
                } = queries::query(&mut client, Query::Receipt { hash })?
                else {
                    anyhow::bail!("expected authenticated Receipt");
                };
                receipts.push(
                    json!({"hash":plain_hex(&hash),"state":state,"indexed_height":indexed_height}),
                );
            }
            evidence["nonce"] = json!(nonce);
            evidence["receipts"] = json!(receipts);
        }
        // A responding child can start while the authenticated read is in flight.
        let observed = now()?;
        ensure!(
            (plan.start_unix..plan.end_unix).contains(&query_started)
                && (query_started..plan.end_unix).contains(&observed),
            "authenticated query did not complete inside the agreed UTC window"
        );
        evidence["query_started_unix"] = json!(query_started);
        evidence["observed_unix"] = json!(observed);
    }
    output(&report, evidence)
}

#[test]
fn distributed_fixture_rejects_reused_keys_wrong_topology_and_live_chain() -> Result<()> {
    let mut plan = Plan {
        run_id: "ab".repeat(16),
        chain_id: 7_000_000_001,
        start_unix: 100,
        end_unix: 200,
        node_sha256: "00".repeat(32),
        aoem_sha256: "00".repeat(32),
        harness_sha256: "00".repeat(32),
        ctl_sha256: "00".repeat(32),
        slots: ROLES
            .into_iter()
            .zip([1, 1, 2, 3])
            .enumerate()
            .map(|(index, (role, host))| Slot {
                role: role.into(),
                address: SocketAddr::new(
                    Ipv4Addr::new(192, 168, 1, host).into(),
                    45000 + index as u16,
                ),
                validator: vec![index as u8; 1952],
                transport: vec![index as u8 + 4; 1952],
            })
            .collect(),
    };
    plan.validate()?;
    let first_ip = plan.slots[0].address.ip();
    plan.slots[3].address.set_ip(first_ip);
    assert!(plan.validate().is_err());
    plan.slots[3].address.set_ip("192.168.1.3".parse()?);
    plan.slots[3].transport = plan.slots[0].validator.clone();
    assert!(plan.validate().is_err());
    plan.slots[3].transport = vec![7; 1952];
    plan.chain_id = CHAIN;
    assert!(plan.validate().is_err());
    Ok(())
}
