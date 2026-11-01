//! A brand-new asset account through the same running native node processes.
use super::*;
use novovm_protocol::uca_spend::AssetAccountCreationV2;

pub(super) fn run(
    nodes: &mut ProductNodes,
    clients: &mut Vec<NativeApplicationClient>,
    set: Arc<ValidatorSet>,
    runtime: &AoemExecFacade,
    alice_key: (&[u8], &[u8]),
    mut alice_intent: AssetSpendIntentV3,
    evidence: &mut Value,
) -> Result<()> {
    ensure!(
        clients.len() == 4,
        "opening requires all four original processes"
    );
    let configs = nodes
        .configs
        .iter()
        .map(fs::read)
        .collect::<std::io::Result<Vec<_>>>()?;
    let alice_before = state(&mut clients[0], &alice_intent, 0)?;
    ensure!(
        alice_before.height == OFFLINE_HEIGHTS + 1,
        "opening starts after original recovery assertions"
    );
    let keygen = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    // Generated only now, after the chain and original recovery have completed.
    // Expanded secrets remain local Zeroizing values; only references are reported.
    let manager = keygen.generate_keypair()?;
    let descriptor = AssetAccountCreationV2 {
        management_key_ref: spend_key_ref_v1(&manager.public_key)?,
        recovery_key_ref: None,
        grants: vec![],
    };
    let account = descriptor
        .account(alice_intent.chain_id, &alice_intent.genesis)?
        .to_vec();
    let signer = runtime.asset_spend_signer_v3()?;
    let mut respend_intent = alice_intent.clone();
    respend_intent.payer = account.clone();
    respend_intent.recipient = ADDRESS[0].to_vec();
    respend_intent.authority = AssetSpendAuthorityV3::Controller;
    respend_intent.spend_key_ref = descriptor.management_key_ref;
    respend_intent.authorization_epoch = 1;
    respend_intent.nonce = 0;
    respend_intent.amount = 10;
    respend_intent.recipient_creation = None;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        nodes.alive()?;
        match query(
            &mut clients[0],
            Query::AssetSpendState {
                intent: AssetReadIntentV2::new(&respend_intent).map_err(anyhow::Error::msg)?,
            },
        )? {
            Answer::AssetSpendState(AssetState::Missing { height, .. }) => {
                ensure!(
                    height == OFFLINE_HEIGHTS + 1,
                    "missing observation at wrong height"
                );
                break;
            }
            Answer::AssetSpendState(AssetState::Pending) => {}
            other => anyhow::bail!("new account already present or unavailable: {other:?}"),
        }
        ensure!(
            Instant::now() < deadline,
            "new account absence query deadline"
        );
    }
    *evidence = json!({
        "completed":false,"real_processes":4,"real_machines":1,
        "transport":"original_native_novorudp_apfl","http_listeners":0,
        "keys_generated_after_height":OFFLINE_HEIGHTS + 1,
        "account":hex(&account),"management_key_ref":hex(&descriptor.management_key_ref),
        "spend_key_ref":hex(&respend_intent.spend_key_ref),
        "spend_authority":"Controller","restricted_grants":false,
        "absent_before_opening":true,"node_configuration_changed":false,
        "genesis_allocation_to_new_account":false,"opening_amount":"100","respend_amount":"10",
        "completed_original_recovery_height":OFFLINE_HEIGHTS + 1,
        "cold_reopen_verified":false,
    });
    alice_intent.recipient = account.clone();
    alice_intent.amount = 100;
    alice_intent.recipient_creation = Some(descriptor.clone());
    let opening = signer.sign(alice_intent.clone(), alice_key.0, alice_key.1)?;
    let opening_id: [u8; 32] = Sha256::digest(opening.intent.signing_bytes()?).into();
    send_uca(&mut clients[0], &opening, 0)?;
    let opening_receipts = receipts(nodes, clients, &[opening_id])?;
    let opening_fee = match opening_receipts.as_slice() {
        [ReceiptState::Finalized {
            success: true,
            nonce_after,
            charged_fee,
            height,
            ..
        }] if *nonce_after == OFFLINE_TRANSACTIONS + 1
            && *height == OFFLINE_HEIGHTS + 2
            && *charged_fee <= opening.intent.max_fee =>
        {
            *charged_fee
        }
        other => anyhow::bail!("runtime opening did not finalize correctly: {other:?}"),
    };
    let opening_heads = recovery::wait_height(nodes, clients, OFFLINE_HEIGHTS + 2)?;
    let mut opened_states = Vec::new();
    for client in clients.iter_mut() {
        let view = state(client, &respend_intent, 0)?;
        ensure!(
            view.state_root == opening_heads[0].state_root
                && view.height == OFFLINE_HEIGHTS + 2
                && view.account_active
                && view.current_spend_epoch == 1
                && view.management_key_ref == descriptor.management_key_ref
                && view.recovery_key_ref.is_none()
                && view.authority == AssetSpendAuthorityV3::Controller
                && !view.grant_present
                && view.grant_epoch == 0
                && view.permits_nov_transfer
                && view.next_nonce == 0
                && view.payer_balance == 100
                && view.recipient_balance == alice_before.payer_balance - 100 - opening_fee
                && view.remaining_debit_budget == 0
                && view.max_debit_per_intent == 0,
            "new account was not initialized atomically: {view:?}"
        );
        opened_states.push(view);
    }
    ensure!(
        opened_states.iter().all(|state| *state == opened_states[0]),
        "four opening views differ"
    );
    evidence["opening_heads"] = json!(opening_heads);
    evidence["opening_receipts"] = json!(opening_receipts);
    evidence["opening_states"] = json!(opened_states);

    let respend = signer.sign(
        respend_intent.clone(),
        &manager.public_key,
        &manager.expanded_secret_key,
    )?;
    let respend_id: [u8; 32] = Sha256::digest(respend.intent.signing_bytes()?).into();
    send_uca(&mut clients[0], &respend, 0)?;
    let respend_receipts = receipts(nodes, clients, &[respend_id])?;
    let respend_fee = match respend_receipts.as_slice() {
        [ReceiptState::Finalized {
            success: true,
            nonce_after: 1,
            charged_fee,
            height,
            ..
        }] if *height == OFFLINE_HEIGHTS + 3 && *charged_fee <= respend.intent.max_fee => {
            *charged_fee
        }
        other => anyhow::bail!("new account cannot spend credited NOV: {other:?}"),
    };
    let final_heads = recovery::wait_height(nodes, clients, OFFLINE_HEIGHTS + 3)?;
    let mut final_states = Vec::new();
    for client in clients.iter_mut() {
        let new_account = state(client, &respend_intent, 0)?;
        let alice = state(client, &alice_intent, 0)?;
        ensure!(
            new_account.state_root == final_heads[0].state_root
                && alice.state_root == new_account.state_root
                && new_account.height == OFFLINE_HEIGHTS + 3
                && new_account.current_spend_epoch == 1
                && new_account.management_key_ref == descriptor.management_key_ref
                && new_account.authority == AssetSpendAuthorityV3::Controller
                && !new_account.grant_present
                && new_account.grant_epoch == 0
                && new_account.permits_nov_transfer
                && new_account.next_nonce == 1
                && new_account.payer_balance == 100 - 10 - respend_fee
                && new_account.remaining_debit_budget == 0
                && alice.payer_balance == alice_before.payer_balance - 100 - opening_fee + 10
                && alice.authority == AssetSpendAuthorityV3::Controller
                && !alice.grant_present
                && alice.remaining_debit_budget == 0
                && alice.next_nonce == OFFLINE_TRANSACTIONS + 1
                && alice.recipient_balance == new_account.payer_balance
                && new_account.recipient_balance == alice.payer_balance,
            "runtime opening/respend economic state differs: {new_account:?} {alice:?}"
        );
        final_states.push((new_account, alice));
    }
    ensure!(
        final_states.iter().all(|state| *state == final_states[0]),
        "four respend views differ"
    );
    let hashes = [opening_id, respend_id];
    let expected_receipts = [opening_receipts[0], respend_receipts[0]];
    ensure!(
        receipts(nodes, clients, &hashes)? == expected_receipts,
        "runtime receipts differ"
    );
    evidence["final_heads"] = json!(final_heads);
    evidence["final_states"] = json!(final_states);
    evidence["receipts"] = json!(expected_receipts);
    evidence["opening_fee"] = json!(opening_fee.to_string());
    evidence["respend_fee"] = json!(respend_fee.to_string());
    nodes.stop_all()?;
    clients.clear();
    for index in 0..4 {
        nodes.start(index, "existing")?;
    }
    *clients = (0..4)
        .map(|index| connect(nodes, index, set.clone()))
        .collect::<Result<Vec<_>>>()?;
    let cold_heads = recovery::wait_height(nodes, clients, OFFLINE_HEIGHTS + 3)?;
    ensure!(
        cold_heads
            .iter()
            .zip(&final_heads)
            .all(|(cold, before)| cold.height == before.height
                && cold.block_hash == before.block_hash
                && cold.state_root == before.state_root
                && cold.receipt_batch_commitment == before.receipt_batch_commitment
                && cold.state_version == before.state_version),
        "runtime opening cold published heads differ"
    );
    ensure!(
        receipts(nodes, clients, &hashes)? == expected_receipts,
        "runtime opening cold receipts differ"
    );
    for client in clients.iter_mut() {
        ensure!(
            (
                state(client, &respend_intent, 0)?,
                state(client, &alice_intent, 0)?
            ) == final_states[0],
            "runtime opening cold state differs"
        );
    }
    ensure!(
        nodes
            .configs
            .iter()
            .map(fs::read)
            .collect::<std::io::Result<Vec<_>>>()?
            == configs,
        "runtime account opening rewrote node configuration"
    );
    evidence["cold_reopen_verified"] = json!(true);
    evidence["completed"] = json!(true);
    Ok(())
}
