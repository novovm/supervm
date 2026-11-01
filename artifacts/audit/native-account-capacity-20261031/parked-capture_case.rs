//! Parked, unqualified large-account component probe; not compiled product wiring.
use super::*;
use crate::native_pipeline::{
    business::{
        nov_transfer_batch::{balance_key, nonce_key},
        uca_spend_state::{grant_write_declarations, AssetSpendStateAccessV2},
    },
    persistence::io::{IoBudget, IoService},
    state::frontier::{BulkCapture, CaptureBudget, CaptureStep, OwnedStateInput},
};

#[test]
#[ignore = "explicit stopped native-load configuration and trusted AOEM library; no live database"]
fn inspect_original_asset_capture_frontier_for_stopped_case() -> Result<()> {
    let config_path = PathBuf::from(
        std::env::var_os("NOVOVM_NATIVE_CAPTURE_CASE_CONFIG")
            .context("explicit stopped load configuration required")?,
    );
    let config = ResidentConfig::load(&config_path)?;
    let material = config
        .uca_initialization
        .as_ref()
        .context("asset initialization required")?;
    let root = material.expected_initial_root;
    let payers = env_count("NOVOVM_NATIVE_CAPTURE_PAYERS", 65_536, 65_536)?;
    let rows = env_count(
        "NOVOVM_NATIVE_CAPTURE_ROWS",
        CONFIGURED_BATCH,
        CONFIGURED_BATCH,
    )?;
    ensure!(
        rows <= payers,
        "capture inspection needs distinct payer/recipient rows"
    );
    let accounts = material
        .accounts
        .iter()
        .map(|account| (account.asset_account.as_slice(), account))
        .collect::<BTreeMap<_, _>>();
    let mut accesses = Vec::with_capacity(rows);
    let mut declarations = BTreeMap::new();
    let mut writable = BTreeSet::new();
    for index in 0..rows {
        let payer = account_bytes(index);
        let recipient = account_bytes(payers + index);
        let account = accounts
            .get(payer.as_slice())
            .context("case payer missing")?;
        ensure!(
            accounts.contains_key(recipient.as_slice()),
            "case recipient missing"
        );
        let grant = account.grants.first().context("case spend grant missing")?;
        let intent = AssetSpendIntentV2 {
            chain_id: config.genesis.chain_id,
            genesis: config.genesis.genesis_config_commitment,
            payer: payer.to_vec(),
            recipient: recipient.to_vec(),
            authorization_epoch: grant.grant.authorization_epoch,
            spend_key_ref: spend_key_ref_v1(&grant.spend_public_key)?,
            asset: 1,
            amount: 1,
            max_fee: 50,
            nonce: 0,
            not_before_height: 1,
            expires_at_height: 257,
            authority_update: None,
            recipient_creation: None,
        };
        for declaration in grant_write_declarations(&intent)? {
            writable.insert(declaration.key);
        }
        writable.extend([
            balance_key(&payer.into()),
            balance_key(&recipient.into()),
            nonce_key(&intent.nonce_identity()?),
        ]);
        let access = AssetSpendStateAccessV2::new(intent, payer.into(), recipient.into())?;
        for declaration in access.declarations() {
            declarations
                .entry(declaration.key.clone())
                .or_insert_with(|| declaration.clone());
        }
        accesses.push(access);
    }
    let mut declarations = declarations.into_values().collect::<Vec<_>>();
    // Same captured keys and traversal as admission. Only puts are enabled for
    // the detached staging probe; no deletion siblings or persisted effects.
    for declaration in &mut declarations {
        declaration.may_put = writable.contains(&declaration.key);
    }
    let shared_budget = crate::native_pipeline::service::ASSET_CAPTURE_BUDGET;
    let budget = if std::env::var("NOVOVM_NATIVE_CAPTURE_ORIGINAL_BUDGET").as_deref() == Ok("1") {
        CaptureBudget {
            nodes: 65_536,
            ..shared_budget
        }
    } else {
        shared_budget
    };
    let io = IoService::start(
        StoreConfig {
            library: config.library,
            database: config.database,
            domain: StorageDomain {
                chain_id: config.genesis.chain_id,
                genesis_config_commitment: config.genesis.genesis_config_commitment,
                protocol_commitment: config.genesis.protocol_commitment,
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        },
        OpenMode::Existing,
        IoBudget {
            requests: 1,
            ..IoBudget::default()
        },
    )?;
    let started = Instant::now();
    let mut accepted_nodes = 0usize;
    let mut accepted_bytes = 0usize;
    let mut requests = 0usize;
    let result = (|| -> Result<OwnedStateInput> {
        let mut capture = BulkCapture::new(root, &declarations, budget)?;
        loop {
            match capture.advance(64)? {
                CaptureStep::More => {}
                CaptureStep::NeedRead => {
                    let hashes = capture
                        .next_request()?
                        .context("capture blocked without request")?;
                    ensure!(
                        !hashes.is_empty() && hashes.len() <= 64,
                        "original I/O quantum changed"
                    );
                    let count = hashes.len();
                    let values = io
                        .try_read_nodes(hashes)?
                        .context("sole inspection I/O owner backpressured")?
                        .wait()?;
                    let bytes = values.iter().flatten().map(Vec::len).sum::<usize>();
                    requests += 1;
                    capture.accept(values)?;
                    accepted_nodes += count;
                    accepted_bytes += bytes;
                }
                CaptureStep::Complete => return capture.finish(),
            }
        }
    })();
    let capture_complete = result.is_ok();
    let mut stage_probe = json!({"status":"NOT_ATTEMPTED"});
    let result = result.and_then(|input| {
        ensure!(
            input.captured_nodes() == accepted_nodes && input.captured_bytes() == accepted_bytes,
            "capture inspection retention accounting mismatch"
        );
        for access in accesses {
            access.read(&input, root)?;
        }
        let keys = declarations.iter().filter(|declaration| declaration.may_put)
            .map(|declaration| declaration.key.clone()).collect::<Vec<_>>();
        let values = input.read_many(&keys)?;
        let changes = keys.into_iter().zip(values)
            .filter_map(|(key, value)| value.filter(|bytes| !bytes.is_empty()).map(|mut value| {
                value[0] ^= 1;
                StateChange::Put { key, value }
            })).collect::<Vec<_>>();
        let staged_at = Instant::now();
        let staged = input.stage(&changes);
        stage_probe = json!({"scope":"detached original immutable-tree put-capacity probe only; bit-flipped encoded values are NOT business effects and are never persisted",
            "changes":changes.len(),"elapsed_seconds":staged_at.elapsed().as_secs_f64(),
            "complete":staged.is_ok(),"new_nodes":staged.as_ref().ok().map(|state|state.nodes().len()),
            "staged_root":staged.as_ref().ok().map(|state|hex(&state.root())),
            "error":staged.as_ref().err().map(|error|format!("{error:#}"))});
        staged?;
        Ok(input)
    });
    println!(
        "{}",
        json!({"scope":"stopped fixture, original bounded BulkCapture and sole IoService; read requests only, no signature authority, candidate writes or finality claim; declarations match pre-pool asset admission, excluding candidate policy/fee keys",
        "rows":rows,"declaration_keys":declarations.len(),"node_budget":budget.nodes,
        "byte_budget":budget.bytes,"accepted_nodes":accepted_nodes,"accepted_bytes":accepted_bytes,
        "io_requests":requests,"elapsed_seconds":started.elapsed().as_secs_f64(),
        "capture_complete":capture_complete,"stage_probe":stage_probe,
        "complete":result.is_ok(),"error":result.as_ref().err().map(|error|format!("{error:#}"))})
    );
    io.shutdown()?;
    result.map(|_| ())
}
