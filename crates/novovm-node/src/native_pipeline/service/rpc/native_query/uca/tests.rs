use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{balance_key, nonce_key};
use crate::native_pipeline::business::uca_spend_state::{
    account_record_changes, grant_record_changes, UcaSpendAccountStateV1,
};
use crate::native_pipeline::state::tree::StateChange;
use novovm_adapter_api::uca_spend_policy::SuppliedSpendGrantV1;
use novovm_adapter_api::unified_account::{BindingState, PersonaBinding, PersonaType, UcaAccount};
use novovm_protocol::native_query::{AccountId, UcaReadIntent};

fn intent() -> UcaSpendIntentV1 {
    UcaSpendIntentV1 {
        chain_id: 7,
        genesis: [2; 32],
        account_id: "uca:alice".into(),
        authorization_epoch: 3,
        spend_key_ref: [4; 32],
        recipient_uca: "uca:bob".into(),
        amount: 10,
        max_fee: 2,
        nonce: 9,
        not_before_height: 1,
        expires_at_height: 10,
    }
}
fn account(byte: u8) -> Account {
    Account::try_from(vec![byte; 20]).unwrap()
}
fn fixture() -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
    let intent = intent();
    let mut changes = Vec::new();
    for (id, address) in [
        (&intent.account_id, account(1)),
        (&intent.recipient_uca, account(2)),
    ] {
        changes.extend(
            account_record_changes(
                7,
                &intent.genesis,
                &UcaSpendAccountStateV1 {
                    account: UcaAccount {
                        uca_id: id.clone(),
                        primary_key_ref: vec![6; 32],
                        primary_key_binding: None,
                        status: UcaStatus::Active,
                        created_at: 1,
                        updated_at: 2,
                    },
                    binding: PersonaBinding {
                        uca_id: id.clone(),
                        persona_type: PersonaType::Web30,
                        chain_id: 7,
                        external_address: address.as_bytes().to_vec(),
                        binding_state: BindingState::Bound,
                        bound_at: 1,
                        revoked_at: None,
                        cooldown_until: None,
                    },
                    current_spend_epoch: 3,
                },
            )
            .unwrap(),
        );
        changes.push(StateChange::Put {
            key: balance_key(&address),
            value: u128::MAX.to_le_bytes().to_vec(),
        });
    }
    changes.extend(
        grant_record_changes(
            &intent,
            &SuppliedSpendGrantV1 {
                account_id: intent.account_id.clone(),
                key_ref: intent.spend_key_ref,
                authorization_epoch: 3,
                permits_nov_transfer: true,
                revoked: true,
                not_before_height: 1,
                expires_at_height: 100,
                max_debit_per_intent: 200,
                remaining_debit_budget: 123,
            },
        )
        .unwrap(),
    );
    changes.push(StateChange::Put {
        key: nonce_key(&intent.nonce_identity().unwrap()),
        value: 9u64.to_le_bytes().to_vec(),
    });
    changes
        .into_iter()
        .map(|change| match change {
            StateChange::Put { key, value } => (key, Some(value)),
            StateChange::Delete { key } => (key, None),
        })
        .collect()
}

#[test]
fn observation_uses_same_record_codec_and_never_turns_revoked_grant_into_permission() {
    let records = fixture();
    let access = UcaSpendStateAccessV1::new(intent(), account(1), account(2)).unwrap();
    let read = |key: &[u8]| Ok(records.get(key).cloned().flatten());
    let state = access.read_optional_with([9; 32], read).unwrap().unwrap();
    let snapshot = view(&state, 5);
    assert!(state.matches_intent(&intent()));
    let mut other_intent = intent();
    other_intent.nonce += 1;
    assert!(!state.matches_intent(&other_intent));
    assert!(snapshot.revoked);
    assert_eq!(snapshot.next_nonce, 9);
    assert_eq!(snapshot.remaining_debit_budget, 123);
    assert_eq!(snapshot.payer_balance, u128::MAX);
    assert_eq!(snapshot.recipient_balance, u128::MAX);
    assert_eq!(snapshot.state_root, [9; 32]);
    assert_eq!(
        state.static_failure(5),
        Some(novovm_adapter_api::uca_spend_policy::SpendPolicyError::Revoked)
    );
    let wrong = UcaSpendStateAccessV1::new(intent(), account(3), account(2)).unwrap();
    assert!(wrong.read_optional_with([9; 32], read).is_err());
    assert!(access.read_optional_with([0; 32], read).is_err());
}

#[test]
fn absent_records_are_not_malformed_or_unavailable_reads() {
    let access = UcaSpendStateAccessV1::new(intent(), account(1), account(2)).unwrap();
    assert!(access
        .read_optional_with([9; 32], |_| Ok(None))
        .unwrap()
        .is_none());
    assert!(access
        .read_optional_with([9; 32], |_| anyhow::bail!("IO unavailable"))
        .is_err());
    let mut records = fixture();
    let header = records
        .keys()
        .find(|key| key.ends_with(b"/len"))
        .unwrap()
        .clone();
    records.insert(header, None);
    assert!(access
        .read_optional_with([9; 32], |key| Ok(records.get(key).cloned().flatten()))
        .is_err());
}

#[test]
fn read_queue_bounds_deduplicates_and_reports_pending_not_absence() {
    let mut queries = UcaQueries::default();
    let now = Instant::now();
    for n in 0..8 {
        let mut intent = intent();
        intent.nonce = n;
        let query = Query::UcaSpendState {
            intent: UcaReadIntent::new(&intent).unwrap(),
            payer_hint: AccountId::Bytes20([1; 20]),
            recipient_hint: AccountId::Bytes20([2; 20]),
        };
        assert_eq!(
            queries.read(query.clone(), [9; 32], 5, now),
            Answer::UcaSpendState(UcaState::Pending)
        );
        assert_eq!(
            queries.read(query, [8; 32], 6, now),
            Answer::UcaSpendState(UcaState::Pending)
        );
        assert_eq!(queries.0.len(), n as usize + 1);
    }
    let query = Query::UcaSpendState {
        intent: UcaReadIntent::new(&intent()).unwrap(),
        payer_hint: AccountId::Bytes20([1; 20]),
        recipient_hint: AccountId::Bytes20([2; 20]),
    };
    assert_eq!(
        queries.read(query, [9; 32], 5, now),
        Answer::Error(QueryError::Busy)
    );
    assert!(queries
        .0
        .iter()
        .all(|entry| entry.root == [9; 32] && entry.height == 5));
}
