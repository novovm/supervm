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
