//! Actual original service startup pins and rejection of legacy asset carriers.
//! Native typed NUAS admission has a separate real-UDP service regression.
use super::uca_genesis::*;
use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{
    uca_effect_contract, uca_program_id, uca_receipt_codec,
};
use crate::native_pipeline::service::{ResidentNode, StartMode};
use novovm_adapter_api::uca_spend_policy::SuppliedSpendGrantV1;
use novovm_exec::pq_signature::AoemMldsaParameterSet;
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::spend_key_ref_v1;

#[test]
#[ignore = "requires trusted NOVOVM_AOEM_TEST_LIBRARY; actual fresh original service and reopen, not asset activation"]
fn actual_uca_startup_three_pins_reopen_and_legacy_asset_rejection() -> Result<()> {
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
    let management =
        aoem_bindings::mldsa_keygen_v1_auto(65)?.context("ML-DSA-65 keygen unavailable")?;
    let spend = aoem_bindings::mldsa_keygen_v1_auto(65)?.context("ML-DSA-65 keygen unavailable")?;
    let id = "uca:startup-owner";
    let manifest = UcaGenesisManifestV1 {
        genesis: super::tests::genesis(),
        accounts: vec![UcaGenesisAccountV1 {
            uca_id: id.into(),
            asset_account: vec![1; 20],
            management_public_key: management.0.clone(),
            grants: vec![UcaGenesisGrantV1 {
                spend_public_key: spend.0.clone(),
                grant: SuppliedSpendGrantV1 {
                    account_id: id.into(),
                    key_ref: spend_key_ref_v1(&spend.0)?,
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
    let approvals = vec![UcaGenesisApprovalV1 {
        uca_id: id.into(),
        signature: runtime
            .mldsa_signer_v1(AoemMldsaParameterSet::MlDsa65)?
            .sign(
                &management.0,
                &management.1,
                &manifest.canonical_bytes()?,
                INITIALIZATION_CONTEXT_V1,
            )?,
    }];
    let verified = manifest.compile(&runtime, domain, &approvals)?;
    let material = UcaInitializationConfigV1 {
        accounts: manifest.accounts.clone(),
        approvals,
        expected_signing_domain: domain,
        expected_initial_root: verified.state().root(),
        expected_final_commitment: verified.final_commitment(),
    };
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests/uca-startup")
        .join(format!("{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let root = root.canonicalize()?;
    let key = root.join("test-only-validator.key");
    std::fs::write(&key, [1_u8; 32])?;
    let mut genesis = manifest.genesis.clone();
    genesis.genesis_config_commitment = domain;
    let config = ResidentConfig {
        profile: ORIGINAL_PROFILE.into(),
        library,
        database: root.join("state.rocksdb"),
        rpc_addr: None,
        signing_key_file: key.clone(),
        udp_addr: "127.0.0.1:0".parse()?,
        peers: genesis
            .validators
            .iter()
            .skip(1)
            .enumerate()
            .map(|(i, v)| NativePeerConfig {
                public_key: v.public_key,
                address: SocketAddr::from(([127, 0, 0, 1], 32002 + i as u16)),
            })
            .collect(),
        native_client_limit: 0,
        genesis,
        uca_initialization: Some(material),
        workers: 2,
        execution_backend: Default::default(),
        batch_size: 2,
    };
    let public_config_path = root.join("public-initialization.json");
    let public_config = serde_json::json!({
        "profile": config.profile, "library": config.library,
        "database": config.database, "rpc_addr": config.rpc_addr,
        "signing_key_file": config.signing_key_file, "udp_addr": config.udp_addr,
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
    for case in 0..5 {
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
            _ => unreachable!(),
        }
        invalid.push(bad.clone());
        bad.database = root.join(format!("must-not-exist-{case}.rocksdb"));
        let path = bad.database.clone();
        assert!(
            ResidentNode::start(bad, StartMode::CreateNew).is_err(),
            "case {case}"
        );
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
        .uca_spend_signer_v1()?
        .sign(
            novovm_protocol::uca_spend::UcaSpendIntentV1 {
                chain_id: config.genesis.chain_id,
                genesis: domain,
                account_id: id.into(),
                authorization_epoch: 1,
                spend_key_ref: spend_key_ref_v1(&spend.0)?,
                recipient_uca: id.into(),
                amount: 1,
                max_fee: 50,
                nonce: 0,
                not_before_height: 1,
                expires_at_height: 100,
            },
            &spend.0,
            &spend.1,
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
    assert_eq!(std::fs::read(key)?, [1_u8; 32]);
    println!("UCA startup: real approvals + three pins before DB; fresh/reopen root retained; invalid fresh/Existing rejected; legacy Raw/APFL and HTTP asset carriers rejected");
    Ok(())
}
