use super::*;
use crate::native_pipeline::{
    business::uca_spend_state::identity_record_changes,
    state::{
        frontier::CaptureBudget,
        tree::{empty_root, stage_state_update, StateChange, StateNodeReader},
    },
};
use novovm_adapter_api::unified_account::{UcaKeyProofType, UcaPrimaryKeyBinding};
use std::collections::BTreeMap;

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn account() -> UcaAccount {
    let public_key = vec![7; novovm_protocol::uca_spend::MLDSA65_PUBLIC_KEY_BYTES];
    UcaAccount {
        uca_id: "uca-owner".into(),
        primary_key_ref: derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &public_key),
        primary_key_binding: Some(UcaPrimaryKeyBinding {
            key_algo: UcaKeyAlgo::Mldsa65,
            public_key,
            proof_type: UcaKeyProofType::GenesisInitializationV1,
            proof_payload: vec![8; 32],
            verified_at: 10,
        }),
        status: UcaStatus::Active,
        created_at: 10,
        updated_at: 20,
    }
}

fn captured(changes: &[StateChange], access: &IdentityAccessV1) -> (NodeHash, OwnedStateInput) {
    let staged = stage_state_update(&Memory::default(), empty_root(), changes).unwrap();
    let root = staged.root();
    let input = OwnedStateInput::capture(
        &Memory(staged.nodes().clone()),
        root,
        access.declarations(),
        CaptureBudget {
            keys: 33,
            nodes: 2048,
            bytes: 2 * 1024 * 1024,
        },
    )
    .unwrap();
    (root, input)
}

#[test]
#[ignore = "requires explicitly qualified AOEM library"]
fn native_registration_creates_one_original_identity_without_overwriting() -> Result<()> {
    use crate::native_pipeline::{
        business::uca_registration::{
            self as registration, register_identity, registration_declarations, RegistrationPlan,
        },
        execution::plan::{BatchContext, PlanBudget},
        persistence::{PacketBudget, PreparedCandidate, StoredCandidate},
        pipeline::test_fee_policy,
    };
    use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade};
    use novovm_protocol::uca_registration::{
        SignedUcaRegistrationV1, UcaRegistrationIntentV1, REGISTRATION_CONTEXT_V1,
    };
    let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY").expect("set qualified library path");
    let owner = AoemExecFacade::open(std::path::Path::new(&library), Default::default())?;
    let runtime = owner.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let key = runtime.generate_keypair()?;
    let reference = derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &key.public_key);
    let hex = reference
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let name = format!("uca:{hex}");
    let access = IdentityAccessV1::new(7, &[2; 32], &name)?;
    let intent = UcaRegistrationIntentV1 {
        chain_id: 7,
        genesis: [2; 32],
        identity: novovm_protocol::native_query::UcaIdentityIdV1::new(&name).unwrap(),
        primary_key_ref: reference.try_into().unwrap(),
        valid_from_height: 2,
        valid_until_height: 4,
    };
    let signature = runtime.sign(
        &key.public_key,
        &key.expanded_secret_key,
        &intent.signing_bytes()?,
        REGISTRATION_CONTEXT_V1,
    )?;
    let signed = SignedUcaRegistrationV1 {
        intent,
        public_key: key.public_key.to_vec(),
        signature,
    };
    let proof = owner.verify_uca_registration_v1(signed.clone(), 7, [2; 32], 2)?;
    let existing = identity_record_changes(7, &[2; 32], &account())?;
    let original_tree = stage_state_update(&Memory::default(), empty_root(), &existing)?;
    let root = original_tree.root();
    let input = OwnedStateInput::capture(
        &Memory(original_tree.nodes().clone()),
        root,
        &registration_declarations(proof.intent())?,
        CaptureBudget {
            keys: 33,
            nodes: 2048,
            bytes: 2 * 1024 * 1024,
        },
    )?;
    let mut context = BatchContext {
        chain_id: 7,
        genesis_config_commitment: [2; 32],
        protocol_commitment: [3; 32],
        business_program: [4; 32],
        semantic_version: 1,
        effect_contract: [5; 32],
        parent_block_hash: [6; 32],
        parent_height: 1,
        parent_state_root: root,
        parent_receipt_root: [7; 32],
        parent_state_version: 1,
        receipt_codec: [8; 32],
        height: 2,
        slot: 0,
        timestamp_unix_ms: 30,
    };
    let changes = register_identity(&proof, &input, &context)?;
    assert!(!changes.is_empty());
    let staged = input.stage(&changes)?;
    let mut combined = existing;
    combined.extend(changes);
    let (published_root, published_input) = captured(&combined, &access);
    assert_eq!(staged.root(), published_root);
    let registered = access.read(&published_input, published_root, 2)?.unwrap();
    assert_eq!(registered.status, IdentityStatusV1::Active);
    assert_eq!(registered.primary_key_ref, proof.intent().primary_key_ref);
    let original = IdentityAccessV1::new(7, &[2; 32], "uca-owner")?;
    let (original_root, original_input) = captured(&combined, &original);
    assert_eq!(original_root, published_root);
    assert_eq!(
        original
            .read(&original_input, original_root, 2)?
            .unwrap()
            .updated_at_ms,
        20
    );
    let mut registration_context = context;
    let policy = test_fee_policy();
    registration_context.business_program = registration::program_id();
    registration_context.semantic_version = registration::REGISTRATION_SEMANTIC_VERSION;
    registration_context.receipt_codec = registration::receipt_codec();
    registration_context.effect_contract = registration::effect_contract(&policy)?;
    let budget = PlanBudget {
        transactions: 1,
        transaction_bytes: 8192,
        body_bytes: 8192,
        access_keys: 33,
    };
    let capture = CaptureBudget {
        keys: 33,
        nodes: 2048,
        bytes: 2 * 1024 * 1024,
    };
    let reader = Memory(original_tree.nodes().clone());
    let raw = signed.encode()?;
    assert!(RegistrationPlan::prepare(raw.clone(), context, &policy, budget, &owner).is_err());
    let plan = RegistrationPlan::prepare(raw, registration_context, &policy, budget, &owner)?;
    let candidate_input = plan.plan().capture(&reader, capture)?;
    let packet = PreparedCandidate::from_registration_executed(
        plan.execute(candidate_input)?,
        PacketBudget::default(),
    )?;
    assert_eq!(packet.state_root(), published_root);
    let stored = StoredCandidate::load(packet.candidate_id(), PacketBudget::default(), |keys| {
        Ok(keys
            .iter()
            .map(|key| packet.records().get(key).cloned())
            .collect())
    })?
    .expect("registration candidate archive");
    assert!(packet.matches(&stored));
    assert!(stored.receipt_view(0).is_err());
    let mut tampered = stored.raw_transactions()[0].clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(
        RegistrationPlan::prepare(tampered, *stored.context(), &policy, budget, &owner).is_err()
    );
    let replay_plan = RegistrationPlan::prepare(
        stored.raw_transactions()[0].clone(),
        *stored.context(),
        &policy,
        budget,
        &owner,
    )?;
    let replay_input = replay_plan.plan().capture(&reader, capture)?;
    let replayed = PreparedCandidate::from_registration_executed(
        replay_plan.execute(replay_input)?,
        PacketBudget::default(),
    )?;
    assert!(replayed.matches(&stored));
    context.parent_state_root = published_root;
    context.parent_height = 2;
    context.height = 3;
    let replay = owner.verify_uca_registration_v1(signed, 7, [2; 32], 3)?;
    assert!(register_identity(&replay, &published_input, &context).is_err());
    assert!(register_identity(&replay, &input, &context).is_err());
    Ok(())
}

#[test]
fn identity_read_uses_existing_mldsa_record_and_preserves_all_statuses_at_one_root() {
    let access = IdentityAccessV1::new(7, &[2; 32], "uca-owner").unwrap();
    assert_eq!(access.declarations().len(), 33);
    assert!(access
        .declarations()
        .iter()
        .all(|d| !d.may_put && !d.may_delete));
    for (status, expected) in [
        (UcaStatus::Active, IdentityStatusV1::Active),
        (UcaStatus::Suspended, IdentityStatusV1::Suspended),
        (UcaStatus::Recovering, IdentityStatusV1::Recovering),
        (UcaStatus::Revoked, IdentityStatusV1::Revoked),
    ] {
        let mut record = account();
        record.status = status;
        let changes = identity_record_changes(7, &[2; 32], &record).unwrap();
        let (root, input) = captured(&changes, &access);
        let view = access.read(&input, root, 42).unwrap().unwrap();
        assert_eq!(view.status, expected);
        assert_eq!(view.state_root, root);
        assert_eq!(view.height, 42);
        assert_eq!(view.primary_key_ref.as_slice(), record.primary_key_ref);
        assert_eq!((view.created_at_ms, view.updated_at_ms), (10, 20));
        assert!(access.read(&input, [9; 32], 42).is_err());
        assert!(access.read(&input, [0; 32], 42).is_err());
    }
}

#[test]
fn identity_missing_partial_orphan_and_wrong_domain_are_distinct() {
    let access = IdentityAccessV1::new(7, &[2; 32], "uca-owner").unwrap();
    let (root, input) = captured(&[], &access);
    assert!(access.read(&input, root, 1).unwrap().is_none());
    let changes = identity_record_changes(7, &[2; 32], &account()).unwrap();
    let (root, input) = captured(&changes, &access);
    let other = IdentityAccessV1::new(8, &[2; 32], "uca-owner").unwrap();
    assert!(
        other.read(&input, root, 1).is_err(),
        "other domain was not captured"
    );
    for remove in [0, 1, 2] {
        let mut partial = changes.clone();
        partial.remove(remove);
        let (root, input) = captured(&partial, &access);
        assert!(access.read(&input, root, 1).is_err());
    }
    let mut orphan = changes;
    orphan[32] = StateChange::Put {
        key: access.declarations()[32].key.clone(),
        value: vec![1],
    };
    let (root, input) = captured(&orphan, &access);
    assert!(access.read(&input, root, 1).is_err());
}

#[test]
fn identity_read_rejects_bad_binding_and_old_local_ed25519_not_a_fallback() {
    let access = IdentityAccessV1::new(7, &[2; 32], "uca-owner").unwrap();
    for variant in 0..5 {
        let mut record = account();
        match variant {
            0 => record.primary_key_ref[0] ^= 1,
            1 => record.primary_key_binding = None,
            2 => record
                .primary_key_binding
                .as_mut()
                .unwrap()
                .public_key
                .pop()
                .map(|_| ())
                .unwrap(),
            3 => record.primary_key_binding.as_mut().unwrap().key_algo = UcaKeyAlgo::Ed25519,
            4 => record.uca_id = "different-identity".into(),
            _ => unreachable!(),
        }
        // Corruption is placed under the requested identity prefix intentionally.
        let changes = record_pages::record_changes(
            &access.prefix,
            &postcard::to_allocvec(&record).unwrap(),
            8192,
        )
        .unwrap();
        let (root, input) = captured(&changes, &access);
        assert!(
            access.read(&input, root, 1).is_err(),
            "bad binding variant {variant}"
        );
    }
}

#[test]
fn identity_read_rejects_trailing_serialization_and_rollback_timestamps() {
    let access = IdentityAccessV1::new(7, &[2; 32], "uca-owner").unwrap();
    let mut bad_time = account();
    bad_time.updated_at = 9;
    for bytes in [
        {
            let mut raw = postcard::to_allocvec(&account()).unwrap();
            raw.push(0);
            raw
        },
        postcard::to_allocvec(&bad_time).unwrap(),
    ] {
        let changes = record_pages::record_changes(&access.prefix, &bytes, 8192).unwrap();
        let (root, input) = captured(&changes, &access);
        assert!(access.read(&input, root, 1).is_err());
    }
    for id in ["", "not/valid", &"x".repeat(129)] {
        assert!(IdentityAccessV1::new(7, &[2; 32], id).is_err());
    }
    assert!(IdentityAccessV1::new(0, &[2; 32], "uca-owner").is_err());
    assert!(IdentityAccessV1::new(7, &[0; 32], "uca-owner").is_err());
}
