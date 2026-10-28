//! Actual original service startup pins and rejection of legacy asset carriers.
//! Native typed NUAS admission has a separate real-UDP service regression.
use super::uca_genesis::*;
use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{
    uca_effect_contract, uca_program_id, uca_receipt_codec,
};
use crate::native_pipeline::service::{ResidentNode, StartMode};
use novovm_adapter_api::uca_spend_policy::SuppliedAssetSpendGrantV2;
use novovm_exec::pq_signature::AoemMldsaParameterSet;
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::{spend_key_ref_v1, NATIVE_NOV_ASSET};

#[test]
#[ignore = "requires trusted NOVOVM_AOEM_TEST_LIBRARY; actual fresh original service and reopen, not asset activation"]
fn actual_uca_startup_three_pins_reopen_and_legacy_asset_rejection() -> Result<()> {
    actual_uca_startup_with_custody()
}

#[cfg(windows)]
#[test]
#[ignore = "requires trusted NOVOVM_AOEM_TEST_LIBRARY; real Windows DPAPI purpose custody through original fresh/reopen service, not deployment"]
fn actual_uca_startup_protected_windows_keys_and_cold_reopen() -> Result<()> {
    actual_uca_startup_with_custody()
}

fn actual_uca_startup_with_custody() -> Result<()> {
    let library = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted NOVOVM_AOEM_TEST_LIBRARY required")?,
    )
    .canonicalize()?;
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == library,
        "test key generation must use the actual startup library"
    );
    let runtime = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
    let signer = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let identity = signer.generate_keypair()?;
    let management = signer.generate_keypair()?;
    let recovery = signer.generate_keypair()?;
    let spend = signer.generate_keypair()?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests/uca-startup")
        .join(format!("{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let root = native_path(&root.canonicalize()?);
    let _credential = super::test_custody::TestCredentialGuard::new(&root)?;
    let key = root.join("test-only-validator.key");
    let transport_key = root.join("test-only-transport.key");
    let mut genesis = super::tests::genesis();
    use novovm_relay::purpose_keys::{self, generate, PurposeV1, ValidatorKeyMaterialV2};
    let validator = purpose_keys::generate_validator(
        &key,
        genesis.chain_id,
        || {
            let pair = signer.generate_keypair()?;
            ValidatorKeyMaterialV2::new(pair.public_key, pair.expanded_secret_key)
        },
        |public, secret, challenge| {
            signer
                .sign(
                    public,
                    secret,
                    challenge,
                    purpose_keys::VALIDATOR_PAIR_CONTEXT,
                )
                .map(|_| ())
                .map_err(Into::into)
        },
    )?;
    let transport = generate(&transport_key, PurposeV1::Transport, genesis.chain_id)?;
    genesis.validators[0].public_key = validator.public_key;
    genesis.validators[0].transport_public_key = transport
        .public_key
        .try_into()
        .map_err(|_| anyhow::anyhow!("transport public key width"))?;
    for validator in &mut genesis.validators[1..] {
        validator.public_key = signer.generate_keypair()?.public_key;
    }
    genesis.allocations = vec![GenesisAllocation {
        account_hex: "01".repeat(20),
        amount: crate::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS
            .to_string(),
    }];
    let key_storage = NodeKeyStorage::ProtectedV1;
    let mut original_key_files = vec![
        (key.clone(), std::fs::read(&key)?),
        (transport_key.clone(), std::fs::read(&transport_key)?),
    ];
    let id = "uca:startup-owner";
    let manifest = UcaGenesisManifestV2 {
        genesis,
        identities: vec![UcaGenesisIdentityV2 {
            uca_id: id.into(),
            identity_public_key: identity.public_key.clone(),
        }],
        accounts: vec![UcaGenesisAccountV2 {
            uca_id: id.into(),
            asset_account: vec![1; 20],
            management_public_key: management.public_key.clone(),
            recovery_public_key: Some(recovery.public_key.clone()),
            grants: vec![UcaGenesisGrantV2 {
                spend_public_key: spend.public_key.clone(),
                grant: SuppliedAssetSpendGrantV2 {
                    account: vec![1; 20],
                    asset: NATIVE_NOV_ASSET,
                    key_ref: spend_key_ref_v1(&spend.public_key)?,
                    authorization_epoch: 1,
                    permits_nov_transfer: true,
                    revoked: false,
                    not_before_height: 1,
                    expires_at_height: 100,
                    max_debit_per_intent: 100,
                    remaining_debit_budget: 1000,
                },
            }],
        }],
    };
    let domain = manifest.signing_domain()?;
    let approve = || -> Result<Vec<UcaGenesisApprovalV2>> {
        [
            (UcaGenesisApprovalSubjectV2::Identity(id.into()), &identity),
            (
                UcaGenesisApprovalSubjectV2::AssetManagement(vec![1; 20]),
                &management,
            ),
            (
                UcaGenesisApprovalSubjectV2::AssetRecovery(vec![1; 20]),
                &recovery,
            ),
        ]
        .into_iter()
        .map(|(subject, key)| {
            let signature = signer.sign(
                &key.public_key,
                &key.expanded_secret_key,
                &manifest.approval_signing_bytes(&subject)?,
                INITIALIZATION_CONTEXT_V2,
            )?;
            Ok(UcaGenesisApprovalV2 { subject, signature })
        })
        .collect()
    };
    let approvals = approve()?;
    let verified = manifest.compile(&runtime, domain, &approvals)?;
    // Randomized signatures attest the same meaning; signatures themselves are
    // never inserted into the initial state or used to choose a new chain root.
    let repeated = manifest.compile(&runtime, domain, &approve()?)?;
    assert_eq!(verified.state().root(), repeated.state().root());
    assert_eq!(verified.final_commitment(), repeated.final_commitment());
    let material = UcaInitializationConfigV2 {
        identities: manifest.identities.clone(),
        accounts: manifest.accounts.clone(),
        approvals,
        expected_signing_domain: domain,
        expected_initial_root: verified.state().root(),
        expected_final_commitment: verified.final_commitment(),
    };
    let mut genesis = manifest.genesis.clone();
    genesis.genesis_config_commitment = domain;
    let config = ResidentConfig {
        profile: ORIGINAL_PROFILE.into(),
        library,
        database: root.join("state.rocksdb"),
        rpc_addr: None,
        signing_key_file: key.clone(),
        transport_key_file: transport_key,
        key_storage,
        udp_addr: "127.0.0.1:0".parse()?,
        peers: genesis
            .validators
            .iter()
            .skip(1)
            .enumerate()
            .map(|(i, v)| NativePeerConfig {
                public_key: v.public_key.clone(),
                address: SocketAddr::from(([127, 0, 0, 1], 32002 + i as u16)),
            })
            .collect(),
        native_client_limit: 0,
        genesis,
        uca_initialization: Some(material),
        workers: 2,
        execution_backend: Default::default(),
        signature_backend: Default::default(),
        batch_size: 2,
    };
    let public_config_path = root.join("public-initialization.json");
    let public_config = serde_json::json!({
        "profile": config.profile, "library": config.library,
        "database": config.database, "rpc_addr": config.rpc_addr,
        "signing_key_file": config.signing_key_file, "udp_addr": config.udp_addr,
        "transport_key_file": config.transport_key_file, "key_storage": config.key_storage,
        "peers": config.peers, "native_client_limit": config.native_client_limit,
        "genesis": config.genesis, "uca_initialization": config.uca_initialization,
        "workers": config.workers, "batch_size": config.batch_size,
    });
    std::fs::write(&public_config_path, serde_json::to_vec(&public_config)?)?;
    let config = ResidentConfig::load(&public_config_path)?;
    assert_eq!(
        config
            .uca_initialization
            .as_ref()
            .unwrap()
            .expected_initial_root,
        verified.state().root()
    );
    let mut unknown = public_config;
    unknown["uca_initialization"]["ignored_root"] = serde_json::json!(vec![0_u8; 32]);
    assert!(serde_json::from_value::<ResidentConfig>(unknown).is_err());
    // Each rejection must precede CandidateStore::open: even its directory may
    // not appear. The same invalid materials also cannot repair an existing DB.
    let mut invalid = Vec::new();
    for case in 0..13 {
        let mut bad = config.clone();
        let init = bad.uca_initialization.as_mut().unwrap();
        match case {
            0 => {
                init.expected_signing_domain[0] ^= 1;
                bad.genesis.genesis_config_commitment = init.expected_signing_domain;
            }
            1 => init.expected_initial_root[0] ^= 1,
            2 => init.expected_final_commitment[0] ^= 1,
            3 => init.accounts[0].grants[0].grant.remaining_debit_budget += 1,
            4 => init.approvals[0].signature[0] ^= 1,
            5 => {
                let first = bad.genesis.validators[1].transport_public_key;
                bad.genesis.validators[1].transport_public_key =
                    bad.genesis.validators[2].transport_public_key;
                bad.genesis.validators[2].transport_public_key = first;
            }
            6 => {
                bad.transport_key_file = root.join("test-only-wrong-transport.key");
                let wrong = generate(
                    &bad.transport_key_file,
                    PurposeV1::Transport,
                    config.genesis.chain_id,
                )?;
                assert_ne!(
                    wrong.public_key,
                    config.genesis.validators[0].transport_public_key
                );
                original_key_files.push((
                    bad.transport_key_file.clone(),
                    std::fs::read(&bad.transport_key_file)?,
                ));
            }
            7 => bad.key_storage = NodeKeyStorage::RawDevelopment,
            8 => {
                init.approvals.pop();
            }
            9 => init.approvals[1].signature = init.approvals[0].signature.clone(),
            10 => init.approvals[2].signature = init.approvals[1].signature.clone(),
            11 => {
                init.approvals[1].subject =
                    UcaGenesisApprovalSubjectV2::AssetManagement(vec![2; 20])
            }
            12 => {
                // Even a genuine manager signature over just the manifest is
                // not the account/purpose-bound v2 approval transcript.
                init.approvals[1].signature = signer.sign(
                    &management.public_key,
                    &management.expanded_secret_key,
                    &manifest.canonical_bytes()?,
                    INITIALIZATION_CONTEXT_V2,
                )?;
            }
            _ => unreachable!(),
        }
        invalid.push(bad.clone());
        bad.database = root.join(format!("must-not-exist-{case}.rocksdb"));
        let path = bad.database.clone();
        let error = ResidentNode::start(bad, StartMode::CreateNew)
            .err()
            .expect("invalid startup material must be rejected");
        if case == 5 {
            assert!(
                format!("{error:#}").contains("UCA initialization manifest/domain mismatch"),
                "case {case}: {error:#}"
            );
        } else if case == 6 {
            assert!(
                format!("{error:#}")
                    .contains("local transport key differs from committed validator binding"),
                "case {case}: {error:#}"
            );
        }
        assert!(!path.exists(), "rejected initializer opened a DB: {case}");
    }
    let first = ResidentNode::start(config.clone(), StartMode::CreateNew)?;
    assert_eq!(first.template.genesis_config_commitment, domain);
    assert_eq!(first.template.parent_state_root, verified.state().root());
    assert_eq!(first.template.business_program, uca_program_id());
    assert_eq!(first.template.receipt_codec, uca_receipt_codec());
    assert_eq!(
        first.template.effect_contract,
        uca_effect_contract(&config.genesis.policy)?
    );
    let parent = first.controller.parent();
    let mut lifecycle = crate::native_pipeline::service::rpc::RpcLifecycle::new(first, 2)?;
    let signed = runtime
        .asset_spend_signer_v2()?
        .sign(
            novovm_protocol::uca_spend::AssetSpendIntentV2 {
                chain_id: config.genesis.chain_id,
                genesis: domain,
                payer: vec![1; 20],
                authorization_epoch: 1,
                spend_key_ref: spend_key_ref_v1(&spend.public_key)?,
                recipient: vec![1; 20],
                asset: NATIVE_NOV_ASSET,
                amount: 1,
                max_fee: 50,
                nonce: 0,
                not_before_height: 1,
                expires_at_height: 100,
                authority_update: None,
            },
            &spend.public_key,
            &spend.expanded_secret_key,
        )?
        .encode()?;
    lifecycle.assert_uca_legacy_asset_rejection_for_test(signed)?;
    assert_eq!(lifecycle.node.controller.parent(), parent);
    lifecycle.node.shutdown()?;
    assert!(ResidentNode::start(config.clone(), StartMode::CreateNew).is_err());
    for bad in invalid {
        assert!(ResidentNode::start(bad, StartMode::Existing).is_err());
    }
    let reopened = ResidentNode::start(config, StartMode::Existing)?;
    assert_eq!(reopened.controller.parent(), parent);
    assert!(reopened.controller.head().is_none());
    assert_eq!(reopened.controller.context().height, 1);
    reopened.shutdown()?;
    for (path, original) in original_key_files {
        assert_eq!(
            std::fs::read(path)?,
            original,
            "custody changed during startup/reopen or rejection"
        );
    }
    println!("UCA startup: custody={key_storage:?}; real approvals + three pins before DB; fresh/reopen root retained; invalid fresh/Existing rejected; legacy Raw/APFL and HTTP asset carriers rejected");
    Ok(())
}
