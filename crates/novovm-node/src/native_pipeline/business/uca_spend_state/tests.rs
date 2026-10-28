//! Seeded immutable-tree boundary tests, NOT registration, funded mainchain
//! transfers, consensus publication or same-batch budget consumption evidence.
use super::*;
use crate::native_pipeline::state::frontier::CaptureBudget;
use crate::native_pipeline::state::tree::{empty_root, stage_state_update, StateNodeReader};

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
fn intent() -> AssetSpendIntentV2 {
    AssetSpendIntentV2 {
        chain_id: 7,
        genesis: [2; 32],
        payer: vec![11; 20],
        recipient: vec![12; 32],
        asset: 1,
        authorization_epoch: 3,
        spend_key_ref: [4; 32],
        amount: 80,
        max_fee: 5,
        nonce: 9,
        not_before_height: 10,
        expires_at_height: 20,
        authority_update: None,
    }
}
fn account(address: &[u8]) -> AssetAccountAuthorityV2 {
    AssetAccountAuthorityV2 {
        account: address.to_vec(),
        asset: 1,
        active: true,
        current_spend_epoch: 3,
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
    }
}
fn grant(i: &AssetSpendIntentV2) -> SuppliedAssetSpendGrantV2 {
    SuppliedAssetSpendGrantV2 {
        account: i.payer.clone(),
        asset: i.asset,
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
fn changes(i: &AssetSpendIntentV2) -> Vec<StateChange> {
    let mut out = Vec::new();
    for (address, balance) in [(i.payer.clone(), 1000u128), (i.recipient.clone(), 7)] {
        out.extend(account_record_changes(i.chain_id, &i.genesis, &account(&address)).unwrap());
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
fn access(i: &AssetSpendIntentV2) -> AssetSpendStateAccessV2 {
    AssetSpendStateAccessV2::new(i.clone(), [11; 20].into(), [12; 32].into()).unwrap()
}
fn tree(changes: &[StateChange]) -> (Memory, NodeHash) {
    let staged = stage_state_update(&Memory::default(), empty_root(), changes).unwrap();
    (Memory(staged.nodes().clone()), staged.root())
}
fn capture(memory: &Memory, root: NodeHash, access: &AssetSpendStateAccessV2) -> OwnedStateInput {
    OwnedStateInput::capture(memory, root, access.declarations(), budget()).unwrap()
}
fn batch_read(
    access: &AssetSpendStateAccessV2,
    input: &OwnedStateInput,
    root: NodeHash,
) -> Result<Vec<CapturedAssetSpendStateV2>> {
    AssetSpendStateAccessV2::read_captured_batch(std::slice::from_ref(access), input, root)?
        .collect()
}

#[test]
fn independent_1024_asset_accounts_capture_and_stage_above_old_key_limit() -> Result<()> {
    use crate::native_pipeline::state::frontier::{BulkCapture, CaptureStep};

    const ROWS: usize = 1024;
    let intents = (0..ROWS)
        .map(|row| {
            let mut value = intent();
            value.payer = vec![0; 32];
            value.payer[..8].copy_from_slice(&(row as u64).to_be_bytes());
            value.payer[31] = 1;
            value.recipient = value.payer.clone();
            value.recipient[31] = 2;
            value
        })
        .collect::<Vec<_>>();
    let accesses = intents
        .iter()
        .map(|value| {
            AssetSpendStateAccessV2::new(
                value.clone(),
                Account::try_from(value.payer.as_slice()).map_err(anyhow::Error::msg)?,
                Account::try_from(value.recipient.as_slice()).map_err(anyhow::Error::msg)?,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut declarations = BTreeMap::new();
    for (value, access) in intents.iter().zip(&accesses) {
        for declaration in access.declarations() {
            declarations.insert(declaration.key.clone(), declaration.clone());
        }
        for declaration in grant_write_declarations(value)? {
            declarations.insert(declaration.key.clone(), declaration);
        }
        for key in [
            nov_transfer_batch::balance_key(&access.payer),
            nov_transfer_batch::balance_key(&access.recipient),
            nov_transfer_batch::nonce_key(&value.nonce_identity()?),
        ] {
            declarations.get_mut(&key).unwrap().may_put = true;
        }
    }
    let declarations = declarations.into_values().collect::<Vec<_>>();
    assert_eq!(declarations.len(), 10 * ROWS);
    assert!(declarations.len() > 4096);
    let capture_budget = CaptureBudget {
        keys: MAX_BATCH_READ_KEYS,
        nodes: 65_536,
        bytes: 16 * 1024 * 1024,
    };

    // Initialize with the real record codec/tree, independently of the batch
    // under test. Keep each seeding operation within the previous key ceiling.
    let mut memory = Memory::default();
    let mut root = empty_root();
    for group in intents.chunks(128) {
        let writes = group.iter().flat_map(changes).collect::<Vec<_>>();
        let staged = stage_state_update(&memory, root, &writes)?;
        root = staged.root();
        memory.0.extend(staged.nodes().clone());
    }
    assert!(BulkCapture::new(
        root,
        &declarations,
        CaptureBudget {
            keys: declarations.len() - 1,
            ..capture_budget
        },
    )
    .is_err());
    let capture_batch = |memory: &Memory, parent| -> Result<OwnedStateInput> {
        let mut capture = BulkCapture::new(parent, &declarations, capture_budget)?;
        loop {
            match capture.advance(64)? {
                CaptureStep::More => {}
                CaptureStep::NeedRead => {
                    let request = capture.next_request()?.context("capture read absent")?;
                    assert!(!request.is_empty() && request.len() <= 64);
                    capture.accept(
                        request
                            .iter()
                            .map(|hash| memory.read_node(hash))
                            .collect::<Result<Vec<_>>>()?,
                    )?;
                }
                CaptureStep::Complete => return capture.finish(),
            }
        }
    };
    let input = capture_batch(&memory, root)?;
    assert!(input.captured_nodes() <= capture_budget.nodes);
    assert!(input.captured_bytes() <= capture_budget.bytes);
    let states = AssetSpendStateAccessV2::read_captured_batch(&accesses, &input, root)?
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(states.len(), ROWS);
    for (state, value) in states.iter().zip(&intents) {
        assert!(state.matches_intent(value));
        assert_eq!(state.balances(), (1000, 7));
        assert_eq!(state.next_nonce(), 9);
        assert_eq!(state.static_failure(12), None);
        assert_eq!(state.grant().unwrap().remaining_debit_budget, 100);
    }

    // Exercise real frontier permissions and tree staging with more than 4096
    // writes, including required absent tail deletes. This is storage capacity
    // evidence, not a substitute for executing signed transfers or fee rules.
    let mut writes = Vec::new();
    for value in &intents {
        let mut record = grant(value);
        record.remaining_debit_budget = 15;
        writes.extend(grant_record_changes(value, &record)?);
    }
    for access in &accesses {
        for (account, balance) in [(&access.payer, 920u128), (&access.recipient, 87)] {
            writes.push(StateChange::Put {
                key: nov_transfer_batch::balance_key(account),
                value: balance.to_le_bytes().to_vec(),
            });
        }
    }
    for value in &intents {
        writes.push(StateChange::Put {
            key: nov_transfer_batch::nonce_key(&value.nonce_identity()?),
            value: 10u64.to_le_bytes().to_vec(),
        });
    }
    assert_eq!(writes.len(), 6 * ROWS);
    let staged = input.stage(&writes)?;
    assert_ne!(staged.root(), root);
    let next_root = staged.root();
    memory.0.extend(staged.nodes().clone());
    let successor = capture_batch(&memory, next_root)?;
    assert!(AssetSpendStateAccessV2::read_captured_batch(&accesses, &input, next_root).is_err());
    let after = AssetSpendStateAccessV2::read_captured_batch(&accesses, &successor, next_root)?
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(after.len(), ROWS);
    for state in after {
        assert_eq!(state.balances(), (920, 87));
        assert_eq!(state.next_nonce(), 10);
        assert_eq!(state.grant().unwrap().remaining_debit_budget, 15);
    }
    Ok(())
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
    assert_eq!(state.payer().account, i.payer);
    assert_eq!(state.recipient().account, vec![12; 32]);
    assert_eq!(state.balances(), (1000, 7));
    assert_eq!(state.next_nonce(), 9);
    assert_eq!(state.grant().unwrap().remaining_debit_budget, 100);
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
fn shared_identity_does_not_share_spend_keys_balance_or_nonce() -> Result<()> {
    let quick = intent();
    let mut savings = quick.clone();
    savings.payer = vec![13; 32];
    savings.spend_key_ref = [14; 32];
    savings.nonce = 47;
    let mut writes = changes(&quick);
    writes.extend(account_record_changes(
        7,
        &quick.genesis,
        &account(&savings.payer),
    )?);
    writes.extend(grant_record_changes(&savings, &grant(&savings))?);
    let savings_account =
        Account::try_from(savings.payer.as_slice()).map_err(anyhow::Error::msg)?;
    writes.push(StateChange::Put {
        key: nov_transfer_batch::balance_key(&savings_account),
        value: 50_000u128.to_le_bytes().to_vec(),
    });
    writes.push(StateChange::Put {
        key: nov_transfer_batch::nonce_key(&savings.nonce_identity()?),
        value: savings.nonce.to_le_bytes().to_vec(),
    });
    for address in [&quick.payer, &savings.payer] {
        writes.extend(identity_association_changes(
            7,
            &quick.genesis,
            "uca:same-person",
            &Account::try_from(address.as_slice()).map_err(anyhow::Error::msg)?,
        )?);
    }
    let (memory, root) = tree(&writes);
    let quick_access = access(&quick);
    let quick_state = quick_access.read(&capture(&memory, root, &quick_access), root)?;
    let savings_access =
        AssetSpendStateAccessV2::new(savings.clone(), savings_account.clone(), [12; 32].into())?;
    let savings_state = savings_access.read(&capture(&memory, root, &savings_access), root)?;
    assert_eq!(
        (quick_state.balances().0, savings_state.balances().0),
        (1000, 50_000)
    );
    assert_eq!(
        (quick_state.next_nonce(), savings_state.next_nonce()),
        (9, 47)
    );
    assert_ne!(quick.nonce_identity()?, savings.nonce_identity()?);
    let mut stolen = savings;
    stolen.spend_key_ref = quick.spend_key_ref;
    let access = AssetSpendStateAccessV2::new(stolen, savings_account, [12; 32].into())?;
    let rejected = access.read(&capture(&memory, root, &access), root)?;
    assert_eq!(
        rejected.static_failure(12),
        Some(novovm_adapter_api::uca_spend_policy::SpendPolicyError::StateUnavailable)
    );
    Ok(())
}

#[test]
fn identity_status_and_binding_changes_are_outside_asset_authority() -> Result<()> {
    use novovm_adapter_api::unified_account::{UcaAccount, UcaStatus};
    let i = intent();
    let mut writes = changes(&i);
    writes.extend(identity_record_changes(
        7,
        &i.genesis,
        &UcaAccount {
            uca_id: "uca:alice".into(),
            primary_key_ref: vec![71; 32],
            primary_key_binding: None,
            status: UcaStatus::Recovering,
            created_at: 1,
            updated_at: 2,
        },
    )?);
    writes.extend(identity_association_changes(
        7,
        &i.genesis,
        "uca:other",
        &[11; 20].into(),
    )?);
    let (memory, root) = tree(&writes);
    let a = access(&i);
    let state = a.read(&capture(&memory, root, &a), root)?;
    assert!(state.static_failure(12).is_none());
    assert_eq!(state.next_nonce(), 9);
    assert_eq!(state.payer().management_key_ref, [8; 32]);
    Ok(())
}

#[test]
fn captured_batch_shares_material_but_checks_each_intent_and_parent() -> Result<()> {
    let first = intent();
    let mut second = first.clone();
    second.nonce += 1;
    second.authorization_epoch += 1;
    let accesses = [access(&first), access(&second)];
    let (memory, root) = tree(&changes(&first));
    let input = capture(&memory, root, &accesses[0]);
    drop(memory);
    let states = AssetSpendStateAccessV2::read_captured_batch(&accesses, &input, root)?
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(states.len(), 2);
    for (index, state) in states.iter().enumerate() {
        assert_eq!(state.parent_root(), root);
        assert_eq!(state.balances(), (1000, 7));
        assert_eq!(state.next_nonce(), 9);
        assert!(state.matches_intent(if index == 0 { &first } else { &second }));
    }
    assert!(states[0].static_failure(10).is_none());
    assert!(
        states[1].static_failure(10).is_some(),
        "shared leaves must not share authority verdicts"
    );
    assert!(AssetSpendStateAccessV2::read_captured_batch(&accesses, &input, [9; 32]).is_err());
    let mut other = first;
    other.spend_key_ref = [91; 32];
    assert!(
        AssetSpendStateAccessV2::read_captured_batch(&[access(&other)], &input, root).is_err(),
        "shared material cannot grant undeclared access"
    );
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
        assert!(batch_read(&a, &capture(&memory, root, &a), root).is_err());
    }
    let (memory, root) = tree(&original);
    let partial =
        OwnedStateInput::capture(&memory, root, &a.declarations()[1..], budget()).unwrap();
    assert!(a.read(&partial, root).is_err());
    assert!(batch_read(&a, &partial, root).is_err());
}

#[test]
fn unsigned_location_changes_rejected_and_identity_associations_do_not_authorize() {
    let i = intent();
    let a = access(&i);
    let (memory, root) = tree(&changes(&i));
    assert!(AssetSpendStateAccessV2::new(i.clone(), [13; 20].into(), [12; 32].into()).is_err());
    assert!(AssetSpendStateAccessV2::new(i.clone(), [11; 20].into(), [11; 20].into()).is_err());
    let mut altered = changes(&i);
    altered.push(StateChange::Put {
        key: owner_key(7, &i.genesis, &[11; 20].into()).unwrap(),
        value: b"uca:mallory".to_vec(),
    });
    let (m, r) = tree(&altered);
    assert!(a
        .read(&capture(&m, r, &a), r)
        .unwrap()
        .static_failure(10)
        .is_none());
    assert!(batch_read(&a, &capture(&m, r, &a), r).is_ok());
    let mut other = i.clone();
    other.genesis = [9; 32];
    let other = access(&other);
    assert!(other.read(&capture(&memory, root, &other), root).is_err());
}

#[test]
fn canonical_pages_tails_nonce_shape_and_account_identity_are_checked() {
    let i = intent();
    let a = access(&i);
    for case in 0..5 {
        let mut altered = changes(&i);
        let prefix =
            account_prefix(7, &i.genesis, &i.payer.as_slice().try_into().unwrap()).unwrap();
        match case {
            0 => {
                let mut bytes = postcard::to_allocvec(&account(&[11; 20])).unwrap();
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
                let mut record = account(&[11; 20]);
                if case == 3 {
                    record.account = vec![91; 20];
                } else {
                    record.asset = 9;
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
        assert!(
            batch_read(&a, &capture(&m, r, &a), r).is_err(),
            "batch case {case}"
        );
    }
}

#[test]
fn compact_accounts_fit_existing_tree_pages_without_identity_reads() {
    let i = intent();
    let a = access(&i);
    let (m, r) = tree(&changes(&i));
    assert_eq!(
        a.read(&capture(&m, r, &a), r).unwrap().balances(),
        (1000, 7)
    );
    assert!(a.declarations().iter().all(|d| d.key.len() <= 256));
    assert_eq!(a.declarations().len(), 10);
    assert!(a.declarations().iter().all(|d| !d
        .key
        .windows(b"binding-owner".len())
        .any(|s| s == b"binding-owner")));
}

#[test]
fn oversized_account_encoding_fails_in_fixed_buffer() {
    let i = intent();
    let mut record = account(&[11; 20]);
    record.account = vec![1; ACCOUNT_BYTES + 1];
    assert!(account_record_changes(7, &i.genesis, &record).is_err());
}

#[test]
fn self_binding_is_one_account_and_tampered_tree_is_not_trusted() -> Result<()> {
    let mut i = intent();
    let (mut m, r) = tree(&changes(&i));
    i.recipient = i.payer.clone();
    let a = AssetSpendStateAccessV2::new(i, [11; 20].into(), [11; 20].into())?;
    let state = a.read(&capture(&m, r, &a), r)?;
    assert_eq!(state.balances(), (1000, 1000));
    assert_eq!(state.payer().account, state.recipient().account);
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
    let signer = facade.asset_spend_signer_v2()?;
    let verifier = facade.asset_spend_verifier_v2()?;
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
                let mut record = account(&[11; 20]);
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
                let mut record = account(&[11; 20]);
                record.active = false;
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
