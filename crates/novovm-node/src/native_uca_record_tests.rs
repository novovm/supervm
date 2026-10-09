//! Isolated native record tests, never production UCA registration or finality.
use super::native_store_records::{self as physical, NativeRecordAccessV1, RawPathChangeV1};
use super::*;
use crate::native_state_records::{RecordChange, RecordOverlayV1};
use crate::native_state_storage::{AoemStateNodesV1, AoemStateReaderV1};
use crate::native_state_tree::empty_root;
use ed25519_dalek::{Signer, SigningKey};
use novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1;
use novovm_adapter_api::unified_account::{UcaAccount, UcaKeyProofType, UcaPrimaryKeyBinding};
use novovm_adapter_novovm::uca_state::*;
use novovm_exec::{AoemSemanticGraphStoreV1, AoemStorageProviderConfigV1};
use novovm_protocol::uca_delegation::{
    SignedUcaDelegationV1, UcaDelegationClaimsV1, UcaDelegationPurposeV1,
};

const ACCOUNT: &str = "uca:isolated-native-record";
const SCOPE: [u8; 32] = [91; 32];
fn record() -> NativeUcaAccountRecordV1 {
    let key = SigningKey::from_bytes(&[41; 32])
        .verifying_key()
        .to_bytes()
        .to_vec();
    NativeUcaAccountRecordV1 {
        version: 1,
        delegation_epoch: 1,
        next_nonce: 0,
        account: UcaAccount {
            uca_id: ACCOUNT.into(),
            primary_key_ref: derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Ed25519, &key),
            primary_key_binding: Some(UcaPrimaryKeyBinding {
                key_algo: UcaKeyAlgo::Ed25519,
                public_key: key,
                proof_type: UcaKeyProofType::SignatureV1,
                proof_payload: vec![],
                verified_at: 1,
            }),
            status: novovm_adapter_api::UcaStatus::Active,
            created_at: 1,
            updated_at: 1,
        },
        authorizations: BTreeMap::from([(
            "01".repeat(32),
            UcaAuthorizationCommitmentV1 {
                app_scope: [9; 32],
                issued_at_height: 1,
                expires_at_height: 30,
                revoked: false,
            },
        )]),
    }
}
fn store() -> NovNativeExecutionStoreV1 {
    let mut store = NovNativeExecutionStoreV1::default();
    store
        .module_state
        .native_uca_accounts_v1
        .insert(ACCOUNT.into(), record());
    store
}
fn path() -> Vec<String> {
    vec![
        "module_state".into(),
        "native_uca_accounts_v1".into(),
        ACCOUNT.into(),
    ]
}
fn revoke_wire(parent: &NativeUcaAccountRecordV1) -> Vec<u8> {
    use novovm_protocol::*;
    let device = SigningKey::from_bytes(&[42; 32]);
    let command = UcaDeviceCommandV1::Revoke {
        commitment: [1; 32],
        app_scope: [9; 32],
    };
    let mut tx = NovNativeTxWireV1 {
        chain_id: 70001,
        signature: vec![],
        kind: NovTxKindV1::Execute(NovExecuteTxV1 {
            caller: device.verifying_key().to_bytes().to_vec(),
            account_id: Some(ACCOUNT.into()),
            fee_owner_account_id: Some(ACCOUNT.into()),
            nonce_owner_account_id: Some(ACCOUNT.into()),
            target: NovExecutionTargetV1::NativeModule("unified_account".into()),
            method: command.method().into(),
            args: command.encode().unwrap(),
            execution_mode: NovExecutionModeV1::Standard,
            execution_policy: NovExecutionPolicyV1::Standard,
            privacy_mode: NovPrivacyModeV1::Public,
            verification_mode: NovVerificationModeV1::Standard,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: 1,
                slippage_bps: 0,
            },
            gas_like_limit: Some(1000),
            nonce: parent.next_nonce,
        }),
    };
    let message = novovm_adapter_api::native_signing::tx_signing_message_v1(
        &nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap(),
    );
    tx.signature
        .extend_from_slice(&device.verifying_key().to_bytes());
    tx.signature
        .extend_from_slice(&device.sign(&message).to_bytes());
    let claims = UcaDelegationClaimsV1 {
        chain_id: 70001,
        genesis: [8; 32],
        account_id: ACCOUNT.into(),
        account_epoch: parent.delegation_epoch,
        app_scope: [9; 32],
        delegate_public_key: device.verifying_key().to_bytes(),
        native_intent: native_tx_unsigned_commitment_v3(&tx).unwrap(),
        not_before_height: 10,
        expires_at_height: 20,
        purpose: UcaDelegationPurposeV1::RevokeDevice,
    };
    let signature = SigningKey::from_bytes(&[41; 32])
        .sign(&claims.signing_bytes().unwrap())
        .to_bytes();
    novovm_protocol::uca_transaction::encode_uca_transaction_v4(
        &tx,
        &SignedUcaDelegationV1 { claims, signature },
    )
    .unwrap()
}
fn domain() -> UcaParentDomainV1 {
    UcaParentDomainV1 {
        chain_id: 70001,
        genesis: [8; 32],
        parent_height: 10,
        app_scope: [9; 32],
    }
}

#[test]
fn native_uca_record_layout_preserves_inactive_images_and_commits_every_account_field() {
    let inactive = NovNativeExecutionStoreV1::default();
    assert!(!String::from_utf8(serde_json::to_vec(&inactive).unwrap())
        .unwrap()
        .contains("native_uca_accounts_v1"));
    assert_eq!(
        physical::decode(physical::encode(&inactive).unwrap()).unwrap(),
        inactive
    );
    let active = store();
    assert_eq!(
        physical::decode(physical::encode(&active).unwrap()).unwrap(),
        active
    );
    let root = native_record_commitment::consensus_state_root_v1(&active.module_state).unwrap();
    for case in 0..5 {
        let mut changed = active.clone();
        let account = changed
            .module_state
            .native_uca_accounts_v1
            .get_mut(ACCOUNT)
            .unwrap();
        match case {
            0 => account.next_nonce += 1,
            1 => account.delegation_epoch += 1,
            2 => account.account.status = novovm_adapter_api::UcaStatus::Revoked,
            3 => account.authorizations.values_mut().next().unwrap().revoked = true,
            _ => {
                account
                    .authorizations
                    .values_mut()
                    .next()
                    .unwrap()
                    .expires_at_height += 1
            }
        }
        assert_ne!(
            root,
            native_record_commitment::consensus_state_root_v1(&changed.module_state).unwrap()
        );
    }
    assert!(native_module_state_shard_value_v1(&active.module_state, "native_execution").is_err());
    assert!(native_module_state_shard_value_v1(&inactive.module_state, "native_execution").is_ok());
}

#[test]
fn native_uca_raw_records_reject_wrong_owner_schema_unknown_fields_and_oversize() {
    let parent = record();
    let mut cases = vec![vec![b' '; MAX_ACCOUNT_RECORD_BYTES_V1 + 1]];
    let mut wrong = parent.clone();
    wrong.version = 2;
    cases.push(serde_json::to_vec(&wrong).unwrap());
    let mut wrong = parent.clone();
    wrong.account.uca_id.push('x');
    cases.push(serde_json::to_vec(&wrong).unwrap());
    let mut wrong = parent.clone();
    wrong.delegation_epoch = 0;
    cases.push(serde_json::to_vec(&wrong).unwrap());
    let mut unknown = serde_json::to_value(&parent).unwrap();
    unknown["account"]["ignored_root"] = serde_json::json!("must reject");
    cases.push(serde_json::to_vec(&unknown).unwrap());
    for raw in cases {
        assert!(physical::physical_path_value_v1(&path(), &raw).is_err());
        assert!(
            native_record_commitment::consensus_change_v1(&RawPathChangeV1::Put {
                path: path(),
                value: raw
            })
            .is_err()
        );
    }
    let raw = serde_json::to_vec(&parent).unwrap();
    assert!(physical::physical_path_value_v1(&path(), &raw).is_ok());
    assert!(
        native_record_commitment::consensus_change_v1(&RawPathChangeV1::Put {
            path: path(),
            value: raw
        })
        .unwrap()
        .is_some()
    );
}

fn read_account(reader: &dyn NativeRecordAccessV1) -> NativeUcaAccountRecordV1 {
    let bytes = reader
        .read_path(&["module_state", "native_uca_accounts_v1", ACCOUNT])
        .unwrap()
        .unwrap();
    physical::validate_uca_record_value_v1(&path(), &bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
#[ignore = "invoked by real_aoem_native_uca_records_survive_process_restart"]
fn native_uca_record_restart_worker() {
    let db_path =
        std::env::var_os("NOVOVM_UCA_RECORD_RESTART_PATH").expect("isolated parent test path");
    let runtime = AoemRuntimeConfig::from_env().unwrap();
    let graph = AoemSemanticGraphStoreV1::open(
        &runtime,
        std::path::Path::new(&db_path),
        &AoemStorageProviderConfigV1::default(),
    )
    .unwrap();
    let mut nodes = AoemStateNodesV1::new(&graph, SCOPE).unwrap();
    let parent_root = nodes.load_record_prepared([1; 32]).unwrap().unwrap().root();
    let reader = RecordOverlayV1::new(&nodes, parent_root);
    let parent = read_account(&reader);
    assert_eq!(parent, record());
    let update = prepare_uca_account_update_v1(&parent, &revoke_wire(&parent), &domain()).unwrap();
    update
        .require_unchanged_parent(&read_account(&reader))
        .unwrap();
    let changes = [RawPathChangeV1::Put {
        path: path(),
        value: serde_json::to_vec(update.next_record()).unwrap(),
    }];
    let mut overlay = RecordOverlayV1::new(&nodes, parent_root);
    physical::apply_raw_path_changes_v1(&mut overlay, &changes).unwrap();
    assert_eq!(read_account(&overlay), *update.next_record());
    // One leaf carries BOTH nonce and revoked state. This is isolated candidate
    // persistence, not a cross-tree transaction or authority publication.
    let staged = overlay.finish();
    nodes
        .persist_record_candidate([2; 32], *update.transaction_id(), [3; 32], &staged)
        .unwrap();
    assert_eq!(
        read_account(&RecordOverlayV1::new(&nodes, parent_root)),
        parent
    );
}

#[test]
#[ignore = "requires bundled AOEM; runs a fresh child process for account recovery"]
fn real_aoem_native_uca_records_survive_process_restart() {
    let runtime = AoemRuntimeConfig::from_env().unwrap();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/uca-state-tests")
        .join(format!("{}-{nonce}", std::process::id()));
    let config = AoemStorageProviderConfigV1::default();
    {
        let graph = AoemSemanticGraphStoreV1::open(&runtime, &db_path, &config).unwrap();
        let mut nodes = AoemStateNodesV1::new(&graph, SCOPE).unwrap();
        let mut overlay = RecordOverlayV1::new(&nodes, empty_root());
        overlay
            .stage(
                &physical::encode(&store())
                    .unwrap()
                    .into_iter()
                    .map(|(key, value)| RecordChange::Put { key, value })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let staged = overlay.finish();
        nodes
            .persist_record_candidate([1; 32], [2; 32], [3; 32], &staged)
            .unwrap();
    }
    let worker = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tx_ingress::native_uca_record_tests::native_uca_record_restart_worker",
            "--ignored",
            "--nocapture",
        ])
        .env("NOVOVM_UCA_RECORD_RESTART_PATH", &db_path)
        .output()
        .unwrap();
    assert!(
        worker.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&worker.stdout),
        String::from_utf8_lossy(&worker.stderr)
    );
    {
        let graph = AoemSemanticGraphStoreV1::open(&runtime, &db_path, &config).unwrap();
        let nodes = AoemStateReaderV1::new(&graph, SCOPE);
        let root = nodes.load_record_prepared([2; 32]).unwrap().unwrap().root();
        let reopened = read_account(&RecordOverlayV1::new(&nodes, root));
        assert_eq!(reopened.next_nonce, 1);
        assert!(reopened.authorizations.values().next().unwrap().revoked);
        assert!(
            prepare_uca_account_update_v1(&reopened, &revoke_wire(&record()), &domain()).is_err()
        );
        let old = nodes.load_record_prepared([1; 32]).unwrap().unwrap().root();
        assert_eq!(read_account(&RecordOverlayV1::new(&nodes, old)), record());
        // Incremental consensus projection must equal cold typed-state import.
        let mut consensus = RecordOverlayV1::new(&nodes, empty_root());
        consensus
            .stage(
                &native_record_commitment::consensus_records_v1(&store().module_state)
                    .unwrap()
                    .into_iter()
                    .map(|(key, value)| RecordChange::Put { key, value })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        native_record_commitment::apply_consensus_changes_v1(
            &mut consensus,
            &[RawPathChangeV1::Put {
                path: path(),
                value: serde_json::to_vec(&reopened).unwrap(),
            }],
        )
        .unwrap();
        let mut expected = store();
        expected
            .module_state
            .native_uca_accounts_v1
            .insert(ACCOUNT.into(), reopened);
        assert_eq!(
            consensus.root(),
            native_record_commitment::consensus_state_root_v1(&expected.module_state).unwrap()
        );
    }
    eprintln!("isolated UCA AOEM evidence: {}", db_path.display());
}
