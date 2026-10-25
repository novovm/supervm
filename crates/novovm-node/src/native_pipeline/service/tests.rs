//! Configuration/compiler and pre-open startup rejection regressions only.
//! No native execution or TPS claim.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use crate::native_pipeline::business::nov_transfer_batch::NovTransferBody;
use crate::native_pipeline::ingress::batch::authenticate_batch_for_proof;
use crate::native_pipeline::ingress::wire::{
    encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use ed25519_dalek::Signer;

pub(super) fn signed_raw(index: u64, nonce: u64) -> Vec<u8> {
    let mut seed = [0x5a; 32];
    seed[..8].copy_from_slice(&index.to_le_bytes());
    let key = SigningKey::from_bytes(&seed);
    let mut recipient = vec![0xee; 20];
    recipient[..8].copy_from_slice(&index.to_le_bytes());
    let mut transaction = TransferV3 {
        chain_id: 71,
        from: key.verifying_key().to_bytes().to_vec(),
        to: recipient,
        asset: "NOV".into(),
        amount: 1,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 500,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    let signature = key.sign(&signing_message(&transaction).unwrap());
    transaction
        .signature
        .extend_from_slice(key.verifying_key().as_bytes());
    transaction
        .signature
        .extend_from_slice(&signature.to_bytes());
    encode_transfer_v3(&transaction).unwrap()
}

fn config(size: usize) -> ResidentConfig {
    let policy = DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    };
    ResidentConfig {
        profile: config::ORIGINAL_PROFILE.into(),
        library: "not-opened-library".into(),
        database: "not-opened-database".into(),
        signing_key_file: "not-read-test-key".into(),
        transport_key_file: "not-read-test-transport-key".into(),
        key_storage: config::NodeKeyStorage::RawDevelopment,
        rpc_addr: None,
        udp_addr: "127.0.0.1:0".parse().unwrap(),
        peers: Vec::new(),
        native_client_limit: 1,
        uca_initialization: None,
        workers: 1,
        execution_backend: Default::default(),
        batch_size: size,
        genesis: GenesisConfig {
            chain_id: 71,
            validator_epoch: 1,
            protocol_commitment: config::protocol_commitment(),
            genesis_config_commitment: [1; 32],
            timestamp_unix_ms: 123,
            validators: Vec::new(),
            allocations: Vec::new(),
            policy,
        },
    }
}

fn identity_rejection_fixture() -> Result<(ResidentConfig, std::net::UdpSocket)> {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/config-tests/purpose-identity")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    std::fs::create_dir_all(&directory)?;
    let directory = config::native_path(&directory.canonicalize()?);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut fixture = config(2);
    fixture.library = directory.join("not-an-aoem-library");
    std::fs::write(&fixture.library, b"must never load this fixture")?;
    fixture.database = directory.join("must-not-open.rocksdb");
    fixture.signing_key_file = directory.join("test-only-validator.key");
    fixture.transport_key_file = directory.join("test-only-transport.key");
    std::fs::write(&fixture.signing_key_file, [1u8; 32])?;
    std::fs::write(&fixture.transport_key_file, [101u8; 32])?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&fixture.signing_key_file, &fixture.transport_key_file] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    fixture.genesis.validators = (1..=4)
        .map(|seed| GenesisValidator {
            public_key: SigningKey::from_bytes(&[seed; 32])
                .verifying_key()
                .to_bytes(),
            transport_public_key: SigningKey::from_bytes(&[100 + seed; 32])
                .verifying_key()
                .to_bytes(),
            weight: 1,
        })
        .collect();
    fixture.genesis.allocations = vec![GenesisAllocation {
        account_hex: "01".repeat(20),
        amount: "1000".into(),
    }];
    fixture.genesis.genesis_config_commitment = fixture.genesis.derive_commitment()?;
    fixture.peers = fixture
        .genesis
        .validators
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, validator)| NativePeerConfig {
            public_key: validator.public_key,
            address: ([127, 0, 0, 1], 35000 + index as u16).into(),
        })
        .collect();
    // Binding is deliberately unavailable. A valid identity rejection must
    // precede this socket boundary as well as loading AOEM/opening a store.
    let occupied = std::net::UdpSocket::bind("127.0.0.1:0")?;
    fixture.udp_addr = occupied.local_addr()?;
    fixture.validate()?;
    Ok((fixture, occupied))
}

fn rejects_before_socket_library_and_store(fixture: ResidentConfig, expected: &str) {
    let database = fixture.database.clone();
    let library = fixture.library.clone();
    let error = ResidentNode::start(fixture, StartMode::CreateNew)
        .err()
        .expect("invalid purpose identity must reject before startup");
    assert!(
        format!("{error:#}").contains(expected),
        "unexpected boundary: {error:#}"
    );
    assert!(!database.exists(), "rejection opened a chain store");
    assert_eq!(
        std::fs::read(library).unwrap(),
        b"must never load this fixture"
    );
}

#[test]
fn mismatched_transport_key_rejects_before_native_startup() -> Result<()> {
    let (fixture, _occupied) = identity_rejection_fixture()?;
    std::fs::write(&fixture.transport_key_file, [211u8; 32])?;
    rejects_before_socket_library_and_store(
        fixture,
        "local transport key differs from committed validator binding",
    );
    Ok(())
}

#[test]
fn unapproved_local_validator_rejects_before_native_startup() -> Result<()> {
    let (fixture, _occupied) = identity_rejection_fixture()?;
    std::fs::write(&fixture.signing_key_file, [211u8; 32])?;
    rejects_before_socket_library_and_store(
        fixture,
        "validator has no approved transport identity",
    );
    Ok(())
}

#[test]
fn local_or_missing_peer_routes_reject_before_native_startup() -> Result<()> {
    let (mut fixture, _occupied) = identity_rejection_fixture()?;
    fixture.peers[0].public_key = fixture.genesis.validators[0].public_key;
    rejects_before_socket_library_and_store(
        fixture,
        "missing remote validator route or route targets local signer",
    );
    let (mut fixture, _occupied) = identity_rejection_fixture()?;
    fixture.peers.pop();
    rejects_before_socket_library_and_store(fixture, "every remote validator route required");
    Ok(())
}

#[test]
fn protected_custody_never_falls_back_to_raw_keys_before_native_startup() -> Result<()> {
    let (mut fixture, _occupied) = identity_rejection_fixture()?;
    fixture.key_storage = config::NodeKeyStorage::ProtectedV1;
    fixture.validate()?;
    rejects_before_socket_library_and_store(fixture, "read protected node purpose custody");
    Ok(())
}

#[test]
fn gpu_deployment_selection_reaches_the_same_original_pipeline_without_protocol_change() {
    let cpu = config(1024);
    let mut gpu = cpu.clone();
    gpu.execution_backend = crate::native_pipeline::pipeline::ExecutionBackend::AoemIntegerV7;
    let cpu_pipeline = pipeline_config(&cpu);
    let gpu_pipeline = pipeline_config(&gpu);
    assert_eq!(gpu_pipeline.execution_backend, gpu.execution_backend);
    assert_eq!(cpu_pipeline.store.domain, gpu_pipeline.store.domain);
    assert_eq!(
        cpu_pipeline.plan.transactions,
        gpu_pipeline.plan.transactions
    );
    assert_eq!(cpu_pipeline.workers, gpu_pipeline.workers);
    assert_eq!(cpu_pipeline.store.library, gpu_pipeline.store.library);
    assert_eq!(
        serde_json::to_value(cpu.genesis).unwrap(),
        serde_json::to_value(gpu.genesis).unwrap()
    );
}

#[test]
fn rpc_pipeline_budget_covers_real_independent_accounts_not_only_shared_receiver() -> Result<()> {
    for size in [1, 128, 1024] {
        let config = config(size);
        let pipeline = pipeline_config(&config);
        let raw: Vec<_> = (0..size).map(|index| signed_raw(index as u64, 0)).collect();
        let authenticate =
            || authenticate_batch_for_proof(71, raw.clone(), pipeline.authentication);
        let body = NovTransferBody::prepare(
            authenticate()?,
            config.genesis.policy.clone(),
            pipeline.plan,
        )?;
        let context = BatchContext {
            chain_id: 71,
            genesis_config_commitment: [1; 32],
            protocol_commitment: config.genesis.protocol_commitment,
            business_program: program_id(),
            semantic_version: SEMANTIC_VERSION,
            effect_contract: effect_contract(&config.genesis.policy)?,
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: empty_root(),
            parent_receipt_root: empty_root(),
            parent_state_version: 0,
            receipt_codec: receipt_codec(),
            height: 1,
            slot: 1,
            timestamp_unix_ms: 124,
        };
        let bound = body.bind(context)?;
        let actual_keys = bound.plan().declared_access().len();
        assert_eq!(
            actual_keys,
            3 * size + 42,
            "compiler-derived complete access set"
        );
        assert!(actual_keys <= pipeline.plan.access_keys);
        assert!(actual_keys <= pipeline.capture.keys);
        assert!(pipeline.plan.access_keys <= 4096);
        assert!(raw.iter().map(Vec::len).sum::<usize>() <= body_byte_limit(size));
        if size >= 128 {
            let old = PlanBudget {
                access_keys: 2 * size + 128,
                ..pipeline.plan
            };
            let error =
                NovTransferBody::prepare(authenticate()?, config.genesis.policy.clone(), old)
                    .err()
                    .expect("old shared-receiver budget must reject arbitrary RPC batch");
            assert!(error.to_string().contains("access budget"));
        }
    }
    Ok(())
}
