use super::*;
use crate::native_pipeline::{
    business::uca_spend_state::identity_record_changes,
    state::{
        frontier::CaptureBudget,
        tree::{empty_root, stage_state_update, StateNodeReader},
    },
};
use novovm_adapter_api::{
    uca_key_binding::derive_primary_key_ref_from_binding_v1,
    unified_account::{UcaAccount, UcaKeyAlgo, UcaKeyProofType, UcaPrimaryKeyBinding, UcaStatus},
};
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::{
    native_query::UcaIdentityIdV1,
    uca_application::{ApplicationGrantIntentV1, SignedApplicationGrantV1},
};
use std::collections::BTreeMap;

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}
fn selector() -> ApplicationGrantSelectorV1 {
    ApplicationGrantSelectorV1 {
        chain_id: 7,
        genesis: [1; 32],
        identity: UcaIdentityIdV1::new("uca:alice").unwrap(),
        application: [2; 32],
        service: [3; 32],
        device: [4; 32],
    }
}
fn account(public: &[u8]) -> UcaAccount {
    UcaAccount {
        uca_id: "uca:alice".into(),
        primary_key_ref: derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, public),
        primary_key_binding: Some(UcaPrimaryKeyBinding {
            key_algo: UcaKeyAlgo::Mldsa65,
            public_key: public.to_vec(),
            proof_type: UcaKeyProofType::GenesisInitializationV1,
            proof_payload: vec![5; 32],
            verified_at: 10,
        }),
        status: UcaStatus::Active,
        created_at: 10,
        updated_at: 10,
    }
}
fn update(memory: &mut Memory, parent: NodeHash, changes: &[StateChange]) -> Result<NodeHash> {
    let staged = stage_state_update(memory, parent, changes)?;
    memory.0.extend(staged.nodes().clone());
    Ok(staged.root())
}
fn read(
    memory: &Memory,
    root: NodeHash,
    access: &ApplicationGrantAccessV1,
) -> Result<ApplicationGrantObservationV1> {
    let captured = OwnedStateInput::capture(
        memory,
        root,
        access.declarations(),
        CaptureBudget {
            keys: 35,
            nodes: 4096,
            bytes: 4 * 1024 * 1024,
        },
    )?;
    access.read(&captured, root, 10)
}

#[test]
fn application_state_missing_corrupt_and_orphan_records_never_authorize() -> Result<()> {
    let access = ApplicationGrantAccessV1::new(selector())?;
    assert_eq!(access.declarations().len(), 35);
    assert!(access.declarations()[..33]
        .iter()
        .all(|a| !a.may_put && !a.may_delete));
    let mut memory = Memory::default();
    let root = update(
        &mut memory,
        empty_root(),
        &identity_record_changes(7, &[1; 32], &account(&vec![7; 1952]))?,
    )?;
    let observation = read(&memory, root, &access)?;
    assert_eq!(observation.revision(), 0);
    assert_eq!(
        observation.query_state(10)?,
        ApplicationGrantStateV1::Missing {
            state_root: root,
            height: 10
        }
    );
    assert!(observation.query_state(11).is_err());
    let absent = read(&Memory::default(), empty_root(), &access)?;
    assert_eq!(
        absent.query_state(10)?,
        ApplicationGrantStateV1::IdentityMissing {
            state_root: empty_root(),
            height: 10
        }
    );
    assert!(absent.query_state(11).is_err());
    assert!(observation.check_at_root(root, 10, 0).is_err());
    for change in [
        StateChange::Put {
            key: access.declarations()[34].key.clone(),
            value: vec![1],
        },
        StateChange::Put {
            key: access.declarations()[33].key.clone(),
            value: vec![0; 4],
        },
    ] {
        let bad = update(&mut memory, root, &[change])?;
        assert!(read(&memory, bad, &access).is_err());
    }
    Ok(())
}

#[test]
#[ignore = "explicit qualified AOEM; real ML-DSA and original captured-tree grant transitions, not publication/login"]
fn actual_mldsa_application_grant_revoke_and_replay_checks() -> Result<()> {
    let library = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM library required")?,
    )
    .canonicalize()?;
    let facade = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
    let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let key = runtime.generate_keypair()?;
    let other = runtime.generate_keypair()?;
    let identity = account(&key.public_key);
    let signer = facade.application_grant_signer_v1()?;
    let verifier = facade.application_grant_verifier_v1()?;
    let intent = ApplicationGrantIntentV1 {
        selector: selector(),
        issuer_key_ref: identity.primary_key_ref.as_slice().try_into()?,
        expected_revision: 0,
        valid_from_height: 10,
        valid_until_height: 20,
        action: ApplicationGrantActionV1::Authorize {
            not_before_height: 10,
            expires_at_height: 100,
        },
    };
    let signed = signer.sign(intent.clone(), &key.public_key, &key.expanded_secret_key)?;
    assert_eq!(SignedApplicationGrantV1::decode(&signed.encode()?)?, signed);
    let mut wrong_purpose = signed.clone();
    wrong_purpose.signature = runtime.sign(
        &key.public_key,
        &key.expanded_secret_key,
        &intent.signing_bytes()?,
        b"NOVOVM/asset/NOV-spend/v3",
    )?;
    assert!(verifier.verify(wrong_purpose).is_err());
    for case in 0..7 {
        let mut tampered = signed.clone();
        match case {
            0 => tampered.intent.selector.service[0] ^= 1,
            1 => tampered.intent.selector.device[0] ^= 1,
            2 => tampered.intent.expected_revision += 1,
            3 => tampered.intent.action = ApplicationGrantActionV1::Revoke,
            4 => tampered.public_key[0] ^= 1,
            5 => tampered.signature[0] ^= 1,
            6 => tampered.intent.selector.application[0] ^= 1,
            _ => unreachable!(),
        }
        assert!(verifier.verify(tampered).is_err(), "tamper case {case}");
    }
    let checked = verifier.verify(signed)?;
    let access = ApplicationGrantAccessV1::new(intent.selector.clone())?;
    let mut memory = Memory::default();
    let initial = update(
        &mut memory,
        empty_root(),
        &identity_record_changes(7, &[1; 32], &identity)?,
    )?;
    let missing = read(&memory, initial, &access)?;
    let captured = OwnedStateInput::capture(
        &memory,
        initial,
        access.declarations(),
        CaptureBudget {
            keys: 35,
            nodes: 4096,
            bytes: 4 * 1024 * 1024,
        },
    )?;
    assert!(access.read(&captured, [9; 32], 10).is_err());
    for case in 0..6 {
        let mut cross = intent.clone();
        match case {
            0 => cross.selector.chain_id += 1,
            1 => cross.selector.genesis[0] ^= 1,
            2 => cross.selector.identity = UcaIdentityIdV1::new("uca:bob").unwrap(),
            3 => cross.selector.application[0] ^= 1,
            4 => cross.selector.service[0] ^= 1,
            5 => cross.selector.device[0] ^= 1,
            _ => unreachable!(),
        }
        let cross =
            verifier.verify(signer.sign(cross, &key.public_key, &key.expanded_secret_key)?)?;
        assert!(
            access.transition(&missing, &cross, initial, 10).is_err(),
            "valid signature with wrong selector {case}"
        );
    }
    let mut revoke_missing = intent.clone();
    revoke_missing.action = ApplicationGrantActionV1::Revoke;
    let revoke_missing = verifier.verify(signer.sign(
        revoke_missing,
        &key.public_key,
        &key.expanded_secret_key,
    )?)?;
    assert!(access
        .transition(&missing, &revoke_missing, initial, 10)
        .is_err());
    let no_identity = read(&Memory::default(), empty_root(), &access)?;
    assert!(access
        .transition(&no_identity, &checked, empty_root(), 10)
        .is_err());
    let mut root_device = intent.clone();
    root_device.selector.device = root_device.issuer_key_ref;
    let root_device_access = ApplicationGrantAccessV1::new(root_device.selector.clone())?;
    let root_device_state = read(&memory, initial, &root_device_access)?;
    let root_device =
        verifier.verify(signer.sign(root_device, &key.public_key, &key.expanded_secret_key)?)?;
    assert!(root_device_access
        .transition(&root_device_state, &root_device, initial, 10)
        .is_err());
    assert!(root_device_access
        .transition(&missing, &checked, initial, 10)
        .is_err());
    assert!(access.transition(&missing, &checked, [9; 32], 10).is_err());
    assert!(access.transition(&missing, &checked, initial, 9).is_err());
    assert!(access.transition(&missing, &checked, initial, 20).is_err());
    let effects = access.transition(&missing, &checked, initial, 10)?;
    assert_eq!(
        effects.len(),
        2,
        "only the application grant record changes"
    );
    let active_root = update(&mut memory, initial, &effects)?;
    let active = read(&memory, active_root, &access)?;
    let ApplicationGrantStateV1::Present(view) = active.query_state(10)? else {
        anyhow::bail!("active grant projection missing")
    };
    assert_eq!(
        (view.state_root, view.height, view.revision, view.revoked),
        (active_root, 10, 1, false)
    );
    assert_eq!(view.issuer_key_ref, intent.issuer_key_ref);
    assert_eq!(view.identity_key_ref, intent.issuer_key_ref);
    assert_eq!(view.identity_status, IdentityStatusV1::Active);
    active.check_at_root(active_root, 10, 1)?;
    active.check_at_root(active_root, 99, 1)?;
    assert!(active.check_at_root(active_root, 9, 1).is_err());
    assert!(active.check_at_root(active_root, 100, 1).is_err());
    assert!(active.check_at_root(active_root, 10, 0).is_err());
    assert!(
        access
            .transition(&active, &checked, active_root, 10)
            .is_err(),
        "no duplicate issuance"
    );
    let mut revoke = intent.clone();
    revoke.expected_revision = 1;
    revoke.action = ApplicationGrantActionV1::Revoke;
    let revoke =
        verifier.verify(signer.sign(revoke, &key.public_key, &key.expanded_secret_key)?)?;
    let revoked_root = update(
        &mut memory,
        active_root,
        &access.transition(&active, &revoke, active_root, 11)?,
    )?;
    let revoked = read(&memory, revoked_root, &access)?;
    let ApplicationGrantStateV1::Present(view) = revoked.query_state(10)? else {
        anyhow::bail!("revoked grant must not be hidden as missing")
    };
    assert_eq!(
        (view.state_root, view.revision, view.revoked),
        (revoked_root, 2, true)
    );
    assert_eq!(revoked.revision(), 2);
    assert!(revoked.check_at_root(revoked_root, 11, 1).is_err());
    assert!(revoked.check_at_root(revoked_root, 11, 2).is_err());
    assert!(
        active.check_at_root(revoked_root, 11, 1).is_err(),
        "stale snapshot cannot be retargeted"
    );
    assert!(
        access
            .transition(&revoked, &checked, revoked_root, 11)
            .is_err(),
        "old authorize cannot undo revoke"
    );
    assert!(access
        .transition(&revoked, &revoke, revoked_root, 11)
        .is_err());
    let mut renewed = intent.clone();
    renewed.expected_revision = 2;
    let renewed =
        verifier.verify(signer.sign(renewed, &key.public_key, &key.expanded_secret_key)?)?;
    let renewed_root = update(
        &mut memory,
        revoked_root,
        &access.transition(&revoked, &renewed, revoked_root, 11)?,
    )?;
    let renewed = read(&memory, renewed_root, &access)?;
    renewed.check_at_root(renewed_root, 11, 3)?;
    assert!(renewed.check_at_root(renewed_root, 11, 1).is_err());
    for status in [
        UcaStatus::Suspended,
        UcaStatus::Recovering,
        UcaStatus::Revoked,
    ] {
        let mut disabled = identity.clone();
        disabled.status = status;
        let root = update(
            &mut memory,
            renewed_root,
            &identity_record_changes(7, &[1; 32], &disabled)?,
        )?;
        let disabled = read(&memory, root, &access)?;
        let ApplicationGrantStateV1::Present(view) = disabled.query_state(10)? else {
            anyhow::bail!("inactive identity grant must remain observable")
        };
        assert_ne!(view.identity_status, IdentityStatusV1::Active);
        assert!(disabled.check_at_root(root, 11, 3).is_err());
        for action in [intent.action.clone(), ApplicationGrantActionV1::Revoke] {
            let mut command = intent.clone();
            command.expected_revision = 3;
            command.action = action;
            let command = verifier.verify(signer.sign(
                command,
                &key.public_key,
                &key.expanded_secret_key,
            )?)?;
            assert!(access.transition(&disabled, &command, root, 11).is_err());
        }
    }
    let rotated = account(&other.public_key);
    let rotated_root = update(
        &mut memory,
        renewed_root,
        &identity_record_changes(7, &[1; 32], &rotated)?,
    )?;
    let rotated_state = read(&memory, rotated_root, &access)?;
    assert!(
        rotated_state.check_at_root(rotated_root, 11, 3).is_err(),
        "root rotation invalidates old issuer"
    );
    let mut old_owner = intent.clone();
    old_owner.expected_revision = 3;
    let old_owner =
        verifier.verify(signer.sign(old_owner, &key.public_key, &key.expanded_secret_key)?)?;
    assert!(access
        .transition(&rotated_state, &old_owner, rotated_root, 11)
        .is_err());
    // A legitimate rotation has a new SignatureV1 binding. With no monotonic
    // identity generation yet, even A -> B -> A remains unavailable, not revived.
    for public in [&other.public_key, &key.public_key] {
        let mut rotated = account(public);
        rotated.primary_key_binding.as_mut().unwrap().proof_type = UcaKeyProofType::SignatureV1;
        let root = update(
            &mut memory,
            renewed_root,
            &identity_record_changes(7, &[1; 32], &rotated)?,
        )?;
        assert!(read(&memory, root, &access).is_err());
    }
    let near_end = GrantRecordV1 {
        grant_id: intent.selector.grant_id()?,
        issuer_key_ref: intent.issuer_key_ref,
        revision: u64::MAX - 1,
        revoked: false,
        not_before_height: 10,
        expires_at_height: 100,
    };
    let near_root = update(
        &mut memory,
        initial,
        &record_pages::record_changes(
            &access.prefix,
            &postcard::to_allocvec(&near_end)?,
            GRANT_BYTES,
        )?,
    )?;
    let near_state = read(&memory, near_root, &access)?;
    let mut command = intent.clone();
    command.expected_revision = u64::MAX - 1;
    assert!(
        signer
            .sign(command.clone(), &key.public_key, &key.expanded_secret_key)
            .is_err(),
        "last revision reserved for revocation"
    );
    command.action = ApplicationGrantActionV1::Revoke;
    let command =
        verifier.verify(signer.sign(command, &key.public_key, &key.expanded_secret_key)?)?;
    let end_root = update(
        &mut memory,
        near_root,
        &access.transition(&near_state, &command, near_root, 11)?,
    )?;
    let end = read(&memory, end_root, &access)?;
    assert_eq!(end.revision(), u64::MAX);
    assert!(end.check_at_root(end_root, 11, u64::MAX).is_err());
    let mut impostor = intent;
    impostor.issuer_key_ref = rotated.primary_key_ref.as_slice().try_into()?;
    let impostor =
        verifier.verify(signer.sign(impostor, &other.public_key, &other.expanded_secret_key)?)?;
    assert!(
        access.transition(&missing, &impostor, initial, 10).is_err(),
        "valid signature is not account authority"
    );
    Ok(())
}
