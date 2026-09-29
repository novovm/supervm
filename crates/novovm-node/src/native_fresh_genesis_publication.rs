//! Explicit genesis authority storage; ordinary node activation remains fenced.
use super::*;
use novovm_exec::{
    AoemAtomicGraphRequestV1, AoemAtomicGraphStepV1, AoemAtomicGraphWriteV1,
    AoemSemanticGraphStoreV1, AoemStorageProviderConfigV1,
};

const SCHEMA: &str = "novovm-aoem-owned-fresh-genesis/v1";
const CHUNK: usize = NOVOVM_AOEM_OWNED_NATIVE_STATE_CHUNK_BYTES_V1;
const MAX_IMAGE: usize = 8 * 1024 * 1024;

#[derive(Serialize)]
struct Image<'a> {
    schema: &'static str,
    chain_id: u64,
    namespace_digest: &'a str,
    config_commitment: [u8; 32],
    state_root: [u8; 32],
    state_root_codec: &'static str,
    config: &'a FreshGenesisConfigV1,
    store: &'a NovNativeExecutionStoreV1,
}

#[derive(Serialize)]
pub struct GenesisPublicationV1 {
    pub config_commitment: [u8; 32],
    pub state_root: [u8; 32],
    pub aoem_genesis_state_persisted: bool,
    pub aoem_readback_verified: bool,
    pub chain_canonical: bool,
    pub finalized: bool,
}

fn retained_authority_locks() -> &'static Mutex<Vec<NovNativeExecutionStoreWriteLockV1>> {
    static LOCKS: OnceLock<Mutex<Vec<NovNativeExecutionStoreWriteLockV1>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Library-only, explicit opt-in. Requires a complete durable manifest already
/// reserved at the normal ledger path. Uses a new, exclusively claimed AOEM DB;
/// never imports Host state or overwrites an existing transaction authority.
/// Until activation/recovery integration exists, ordinary startup must reject
/// both the reserved ledger and this distinct authority-head codec.
pub fn publish_v1(
    chain_id: u64,
    expected_config: [u8; 32],
    params: &serde_json::Value,
) -> Result<GenesisPublicationV1> {
    validate_native_persistence_path_isolation_v1(params)?;
    let gates = tx_ingress_aoem_ownership_gates_from_params_v1(params);
    if !gates.explicit || !(gates.production_candidate || gates.semantic_graph_v3_required) {
        bail!("fresh genesis publication requires explicit AOEM ownership");
    }
    let protocol = parse_fixed_hex_32_v1(
        &verify_required_native_business_protocol_config_pin_v1()?,
        "genesis protocol",
    )?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("fresh genesis requires an explicit native execution store path")?;
    let lock = acquire_nov_native_execution_store_write_lock_v1(&native_path)?;
    let namespace = native_aoem_owned_state_namespace_digest_v1(params, chain_id);
    let namespace_bytes = parse_fixed_hex_32_v1(&namespace, "genesis namespace")?;
    let config = NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(
        &nov_native_block_ledger_rocksdb_path_v1(&native_path),
        expected_config,
        namespace_bytes,
    )?
    .context("durable full genesis manifest is required before AOEM publication")?;
    if config.chain_id != chain_id || config.protocol_config_commitment != protocol {
        bail!("fresh genesis runtime chain or protocol differs from approved manifest");
    }
    // Inspect all Host backends, not just the selected one: a backend switch
    // must not hide an old test snapshot during fresh initialization.
    for projection in [
        native_path.clone(),
        nov_native_execution_store_rocksdb_path_v1(&native_path),
        nov_native_execution_store_json_backup_path_v1(&native_path),
    ] {
        if projection.exists() {
            bail!(
                "fresh genesis refuses existing Host projection storage: {}",
                projection.display()
            );
        }
    }
    let compiled = config.compile()?;
    let mut store = compiled.initial_store().clone();
    store.authority_chain_id = Some(chain_id);
    store.authority_namespace_digest = namespace.clone();
    let image = serde_json::to_vec(&Image {
        schema: SCHEMA,
        chain_id,
        namespace_digest: &namespace,
        config_commitment: expected_config,
        state_root: compiled.state_root(),
        state_root_codec: NOVOVM_NATIVE_STATE_ROOT_CODEC_V3,
        config: &config,
        store: &store,
    })?;
    if image.len() > MAX_IMAGE {
        bail!("fresh genesis AOEM image exceeds bound");
    }
    let digest = sha256_bytes_v1(&[b"novovm-aoem-genesis-image-v1\0", &image]);
    // No fabricated batch id, transaction receipt or QC appears in this head.
    // Fixed 152-byte genesis head: NVG1, chain, namespace, config, state root,
    // image digest, length, chunk count. Old transaction-head JSON readers reject it.
    let mut head = b"NVG1".to_vec();
    head.extend_from_slice(&chain_id.to_be_bytes());
    head.extend_from_slice(&namespace_bytes);
    head.extend_from_slice(&expected_config);
    head.extend_from_slice(&compiled.state_root());
    head.extend_from_slice(&digest);
    head.extend_from_slice(&(image.len() as u64).to_be_bytes());
    head.extend_from_slice(&(image.len().div_ceil(CHUNK) as u32).to_be_bytes());
    let path = native_aoem_owned_state_db_path_v1(params);
    let runtime = native_aoem_owned_runtime_config_v1()?;
    if runtime.persist_backend.trim().eq_ignore_ascii_case("none") {
        bail!("fresh genesis requires persistent AOEM storage");
    }
    // A separate immutable local claim distinguishes a fresh DB owned by this
    // initializer from an occupied DB with a missing/corrupt authority head.
    // Claim bytes contain local physical identity, not shared consensus data.
    let mut claim_path = path.as_os_str().to_os_string();
    claim_path.push(".fresh-genesis-claim-v1");
    let claim_path = PathBuf::from(claim_path);
    let claim = serde_json::to_vec(&serde_json::json!({
        "schema": SCHEMA, "config": expected_config, "namespace": namespace,
        "authority_lock": fs::canonicalize(nov_native_execution_store_lock_path_v1(&native_path))?,
        "image_digest": digest,
    }))?;
    if claim_path.exists() {
        if fs::read(&claim_path)? != claim {
            bail!("fresh genesis DB ownership claim mismatch");
        }
    } else {
        if path.exists() {
            bail!("fresh genesis refuses an existing AOEM DB without its ownership claim");
        }
        if let Some(parent) = claim_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&claim_path)?;
        file.write_all(&claim)?;
        file.sync_all()?;
    }
    let graph =
        AoemSemanticGraphStoreV1::open(&runtime, &path, &AoemStorageProviderConfigV1::default())?;
    let head_key = native_aoem_owned_state_head_key_v1(chain_id, &namespace);
    let chunks: Vec<_> = image
        .chunks(CHUNK)
        .enumerate()
        .map(|(index, bytes)| {
            let mut key = b"NVM1GENESIS".to_vec();
            key.extend_from_slice(&digest);
            key.extend_from_slice(&(index as u32).to_be_bytes());
            (key, bytes.to_vec())
        })
        .collect();
    let previous = graph.get(&head_key)?;
    if previous.as_ref().is_some_and(|bytes| bytes != &head) {
        bail!("fresh genesis refuses a different AOEM authority head");
    }
    for (key, bytes) in &chunks {
        if graph
            .get(key)?
            .as_ref()
            .is_some_and(|stored| stored != bytes)
        {
            bail!("fresh genesis AOEM chunk conflicts with approved image");
        }
    }
    let verify = || -> Result<()> {
        if graph.get(&head_key)?.as_ref() != Some(&head) {
            bail!("genesis authority head readback mismatch");
        }
        for (key, bytes) in &chunks {
            if graph.get(key)?.as_ref() != Some(bytes) {
                bail!("genesis image readback incomplete or changed");
            }
        }
        Ok(())
    };
    if previous.is_some() {
        verify()?; // missing chunks behind a completed head are corruption, not a retry
    } else {
        let steps = chunks
            .chunks(4)
            .map(|group| AoemAtomicGraphStepV1 {
                task_kind: 1,
                task_payload: digest.to_vec(),
                event: None,
                writes: group
                    .iter()
                    .map(|(key, value)| AoemAtomicGraphWriteV1::Put {
                        key: key.clone(),
                        value: value.clone(),
                    })
                    .collect(),
            })
            .collect();
        let published = graph
            .commit(AoemAtomicGraphRequestV1 {
                graph_id: u64::from_be_bytes(digest[..8].try_into()?).max(1),
                steps,
                completion_write: AoemAtomicGraphWriteV1::Put {
                    key: head_key.clone(),
                    value: head.clone(),
                },
            })
            .and_then(|_| verify());
        if let Err(error) = published {
            // Unknown completion may still write. Fence authority until process
            // exit, then recover from the exact durable manifest and image.
            retained_authority_locks()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(lock);
            return Err(error).context(
                "genesis publication uncertain; authority lock retained until process exit",
            );
        }
    }
    Ok(GenesisPublicationV1 {
        config_commitment: expected_config,
        state_root: compiled.state_root(),
        aoem_genesis_state_persisted: true,
        aoem_readback_verified: true,
        chain_canonical: false,
        finalized: false,
    })
}
