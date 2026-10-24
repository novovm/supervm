//! Seeded immutable-tree boundary tests, NOT registration, funded mainchain
//! transfers, consensus publication or same-batch budget consumption evidence.
use super::*;
use crate::native_pipeline::state::frontier::CaptureBudget;
use crate::native_pipeline::state::tree::{empty_root, stage_state_update, StateNodeReader};
use novovm_adapter_api::unified_account::UcaStatus;

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}
fn budget() -> CaptureBudget {
    CaptureBudget {
        keys: 256,
        nodes: 8192,
        bytes: 2 * 1024 * 1024,
    }
}
fn intent() -> UcaSpendIntentV1 {
    UcaSpendIntentV1 {
        chain_id: 7,
        genesis: [2; 32],
        account_id: "uca:alice".into(),
        authorization_epoch: 3,
        spend_key_ref: [4; 32],
        recipient_uca: "uca:bob".into(),
        amount: 80,
        max_fee: 5,
        nonce: 9,
        not_before_height: 10,
        expires_at_height: 20,
    }
}
fn account(id: &str, address: &[u8]) -> UcaSpendAccountStateV1 {
    UcaSpendAccountStateV1 {
        account: UcaAccount {
            uca_id: id.into(),
            primary_key_ref: vec![8; 32],
            primary_key_binding: None,
            status: UcaStatus::Active,
            created_at: 1,
            updated_at: 2,
        },
        binding: PersonaBinding {
            uca_id: id.into(),
            persona_type: PersonaType::Web30,
            chain_id: 7,
            external_address: address.to_vec(),
            binding_state: BindingState::Bound,
            bound_at: 1,
            revoked_at: None,
            cooldown_until: None,
        },
        current_spend_epoch: 3,
    }
}
fn grant(i: &UcaSpendIntentV1) -> SuppliedSpendGrantV1 {
    SuppliedSpendGrantV1 {
        account_id: i.account_id.clone(),
        key_ref: i.spend_key_ref,
        authorization_epoch: i.authorization_epoch,
        permits_nov_transfer: true,
        revoked: false,
        not_before_height: 1,
        expires_at_height: 30,
        max_debit_per_intent: 100,
        remaining_debit_budget: 100,
    }
}
fn changes(i: &UcaSpendIntentV1) -> Vec<StateChange> {
    let mut out = Vec::new();
    for (id, address, balance) in [
        (&i.account_id, vec![11; 20], 1000u128),
        (&i.recipient_uca, vec![12; 32], 7),
    ] {
        out.extend(account_record_changes(i.chain_id, &i.genesis, &account(id, &address)).unwrap());
        out.push(StateChange::Put {
            key: nov_transfer_batch::balance_key(&address.try_into().unwrap()),
            value: balance.to_le_bytes().to_vec(),
        });
    }
    out.extend(grant_record_changes(i, &grant(i)).unwrap());
    out.push(StateChange::Put {
        key: nov_transfer_batch::nonce_key(&i.nonce_identity().unwrap()),
        value: i.nonce.to_le_bytes().to_vec(),
    });
    out
}
fn access(i: &UcaSpendIntentV1) -> UcaSpendStateAccessV1 {
    UcaSpendStateAccessV1::new(i.clone(), [11; 20].into(), [12; 32].into()).unwrap()
}
fn tree(changes: &[StateChange]) -> (Memory, NodeHash) {
    let staged = stage_state_update(&Memory::default(), empty_root(), changes).unwrap();
    (Memory(staged.nodes().clone()), staged.root())
}
fn capture(memory: &Memory, root: NodeHash, access: &UcaSpendStateAccessV1) -> OwnedStateInput {
    OwnedStateInput::capture(memory, root, access.declarations(), budget()).unwrap()
}

#[test]
fn one_parent_resolves_original_accounts_grant_nonce_and_balances() -> Result<()> {
    let i = intent();
    let a = access(&i);
    let (memory, root) = tree(&changes(&i));
    let input = capture(&memory, root, &a);
    drop(memory); // no fresh/live DB read after capture
    let state = a.read(&input, root)?;
    assert_eq!(state.parent_root(), root);
    assert_eq!(state.payer().account.uca_id, i.account_id);
    assert_eq!(state.recipient().binding.external_address, vec![12; 32]);
    assert_eq!(state.balances(), (1000, 7));
    assert_eq!(state.next_nonce(), 9);
    assert_eq!(state.grant().remaining_debit_budget, 100);
    assert!(a.declarations().iter().all(|d| !d.may_put && !d.may_delete));
    assert!(
        input
            .stage(&[StateChange::Put {
                key: nov_transfer_batch::nonce_key(&i.nonce_identity()?),
                value: 10u64.to_le_bytes().to_vec()
            }])
            .is_err(),
        "read projection is not a write capability"
    );
    assert!(a.read(&input, [99; 32]).is_err());
    assert!(a.read(&input, [0; 32]).is_err());
    Ok(())
}

#[test]
fn missing_records_and_undeclared_input_are_not_empty_authority() {
    let i = intent();
    let a = access(&i);
    let original = changes(&i);
    for access in a.declarations() {
        if !original
            .iter()
            .any(|c| matches!(c, StateChange::Put {key,..} if key == &access.key))
        {
            continue;
        }
        let mut missing = original.clone();
        missing.push(StateChange::Delete {
            key: access.key.clone(),
        });
        let (memory, root) = tree(&missing);
        assert!(
            a.read(&capture(&memory, root, &a), root).is_err(),
            "missing key {:?}",
            access.key
        );
    }
    let (memory, root) = tree(&original);
    let partial =
        OwnedStateInput::capture(&memory, root, &a.declarations()[1..], budget()).unwrap();
    assert!(a.read(&partial, root).is_err());
}

#[test]
fn false_location_alias_reverse_owner_and_other_chain_are_rejected() {
    let i = intent();
    let a = access(&i);
    let (memory, root) = tree(&changes(&i));
    let wrong = UcaSpendStateAccessV1::new(i.clone(), [13; 20].into(), [12; 32].into()).unwrap();
    assert!(wrong.read(&capture(&memory, root, &wrong), root).is_err());
    assert!(UcaSpendStateAccessV1::new(i.clone(), [11; 20].into(), [11; 20].into()).is_err());
    let mut altered = changes(&i);
    altered.push(StateChange::Put {
        key: owner_key(7, &i.genesis, &[11; 20].into()).unwrap(),
        value: b"uca:mallory".to_vec(),
    });
    let (m, r) = tree(&altered);
    assert!(a.read(&capture(&m, r, &a), r).is_err());
    let mut other = i.clone();
    other.genesis = [9; 32];
    let other = access(&other);
    assert!(other.read(&capture(&memory, root, &other), root).is_err());
}

#[test]
fn canonical_pages_tails_nonce_shape_and_binding_state_are_checked() {
    let i = intent();
    let a = access(&i);
    for case in 0..5 {
        let mut altered = changes(&i);
        let prefix = account_prefix(7, &i.genesis, &i.account_id).unwrap();
        match case {
            0 => {
                let mut bytes = postcard::to_allocvec(&account(&i.account_id, &[11; 20])).unwrap();
                bytes.push(0);
                altered
                    .extend(record_pages::record_changes(&prefix, &bytes, ACCOUNT_BYTES).unwrap());
            }
            1 => {
                let tail = record_pages::declarations(&prefix, ACCOUNT_BYTES, false)
                    .unwrap()
                    .pop()
                    .unwrap()
                    .key;
                altered.push(StateChange::Put {
                    key: tail,
                    value: vec![0],
                });
            }
            2 => altered.push(StateChange::Put {
                key: nov_transfer_batch::nonce_key(&i.nonce_identity().unwrap()),
                value: vec![0; 9],
            }),
            3 | 4 => {
                let mut record = account(&i.account_id, &[11; 20]);
                if case == 3 {
                    record.binding.binding_state = BindingState::Revoked;
                } else {
                    record.binding.persona_type = PersonaType::Evm;
                }
                altered.extend(
                    record_pages::record_changes(
                        &prefix,
                        &postcard::to_allocvec(&record).unwrap(),
                        ACCOUNT_BYTES,
                    )
                    .unwrap(),
                );
            }
            _ => unreachable!(),
        }
        let (m, r) = tree(&altered);
        assert!(a.read(&capture(&m, r, &a), r).is_err(), "case {case}");
    }
}

#[test]
fn maximum_identifiers_fit_existing_tree_pages_without_a_new_address_scheme() {
    let mut i = intent();
    i.account_id = "a".repeat(MAX_UCA_ID_BYTES);
    i.recipient_uca = "b".repeat(MAX_UCA_ID_BYTES);
    let a = access(&i);
    let (m, r) = tree(&changes(&i));
    assert_eq!(
        a.read(&capture(&m, r, &a), r).unwrap().balances(),
        (1000, 7)
    );
    assert!(a.declarations().iter().all(|d| d.key.len() <= 256));
}

#[test]
fn oversized_account_encoding_fails_in_fixed_buffer() {
    use novovm_adapter_api::unified_account::{UcaKeyAlgo, UcaKeyProofType, UcaPrimaryKeyBinding};
    let i = intent();
    let mut record = account(&i.account_id, &[11; 20]);
    record.account.primary_key_binding = Some(UcaPrimaryKeyBinding {
        key_algo: UcaKeyAlgo::Mldsa87,
        public_key: vec![1; ACCOUNT_BYTES + 1],
        proof_type: UcaKeyProofType::SignatureV1,
        proof_payload: vec![],
        verified_at: 1,
    });
    assert!(account_record_changes(7, &i.genesis, &record).is_err());
}

#[test]
fn self_binding_is_one_account_and_tampered_tree_is_not_trusted() -> Result<()> {
    let mut i = intent();
    let (mut m, r) = tree(&changes(&i));
    i.recipient_uca = i.account_id.clone();
    let a = UcaSpendStateAccessV1::new(i, [11; 20].into(), [11; 20].into())?;
    let state = a.read(&capture(&m, r, &a), r)?;
    assert_eq!(state.balances(), (1000, 1000));
    assert_eq!(
        state.payer().binding.external_address,
        state.recipient().binding.external_address
    );
    m.0.get_mut(&r).context("fixture root absent")?[0] ^= 1;
    assert!(OwnedStateInput::capture(&m, r, a.declarations(), budget()).is_err());
    Ok(())
}

#[test]
#[ignore = "explicit trusted AOEM signature runtime; captured-tree foundation, not native spend"]
fn actual_mldsa_signature_and_same_parent_preflight_reject_stale_authority() -> Result<()> {
    use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
    use novovm_protocol::uca_spend::spend_key_ref_v1;
    let (platform, library) = if cfg!(target_os = "windows") {
        ("windows", "aoem_ffi.dll")
    } else if cfg!(target_os = "macos") {
        ("macos", "libaoem_ffi.dylib")
    } else {
        ("linux", "libaoem_ffi.so")
    };
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../aoem")
        .join(platform)
        .join("core/bin")
        .join(library);
    // Reuse the existing safe bindings loader for ephemeral fixture keys.
    // Never accept an ambient override to a different library or silently skip.
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == path.canonicalize()?,
        "fixture key generator must use the same packaged AOEM library"
    );
    let facade = AoemExecFacade::open(&path, AoemExecOpenOptions::default())?;
    let signer = facade.uca_spend_signer_v1()?;
    let verifier = facade.uca_spend_verifier_v1()?;
    let (public, mut secret) = aoem_bindings::mldsa_keygen_v1_auto(65)?
        .context("packaged ML-DSA-65 fixture key generation unavailable")?;
    let mut i = intent();
    i.spend_key_ref = spend_key_ref_v1(&public)?;
    let signed = signer.sign(i.clone(), &public, &secret);
    secret.fill(0); // ephemeral fixture only, not a platform keystore claim
    let checked = verifier.verify(signed?)?;
    let a = access(&i);
    let original = changes(&i);
    let (m, r) = tree(&original);
    let before = a.read(&capture(&m, r, &a), r)?;
    before.preflight(&checked, r, 12)?;
    assert!(before.preflight(&checked, r, 20).is_err());
    for case in 0..5 {
        let mut delta = Vec::new();
        match case {
            0 => {
                let mut record = account(&i.account_id, &[11; 20]);
                record.current_spend_epoch = 4;
                delta.extend(account_record_changes(7, &i.genesis, &record)?);
            }
            1 | 2 => {
                let mut g = grant(&i);
                if case == 1 {
                    g.revoked = true;
                } else {
                    g.remaining_debit_budget = 84;
                }
                delta.extend(grant_record_changes(&i, &g)?);
            }
            3 => delta.push(StateChange::Put {
                key: nov_transfer_batch::nonce_key(&i.nonce_identity()?),
                value: 10u64.to_le_bytes().to_vec(),
            }),
            4 => {
                let mut record = account(&i.account_id, &[11; 20]);
                record.account.status = UcaStatus::Recovering;
                delta.extend(account_record_changes(7, &i.genesis, &record)?);
            }
            _ => unreachable!(),
        }
        let update = stage_state_update(&m, r, &delta)?;
        let mut all = m.0.clone();
        all.extend(update.nodes().clone());
        let after = a.read(&capture(&Memory(all), update.root(), &a), update.root())?;
        assert!(
            after.preflight(&checked, update.root(), 12).is_err(),
            "case {case}"
        );
        assert!(
            before.preflight(&checked, update.root(), 12).is_err(),
            "old snapshot not reusable at successor"
        );
        assert_eq!(
            after.balances(),
            (1000, 7),
            "no implicit spending/authority migration"
        );
    }
    Ok(())
}
