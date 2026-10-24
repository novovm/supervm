//! No native library needed: verify admission accounting before authentication.
use super::*;
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::state::tree::empty_root;

#[test]
fn uca_request_charges_raw_bytes_and_all_locator_hints() {
    let reference = tests::request(empty_root());
    let raw = vec![vec![0x55; 4000]];
    let hints = vec![(Account::from([1; 32]), Account::from([2; 32]))];
    let request = BatchRequest::from_uca(
        raw,
        hints,
        reference.request.context,
        reference.request.policy,
    )
    .unwrap();
    assert_eq!(request.body_bytes, 4064);
    assert_eq!(request.max_transaction_bytes, 4000);
    assert_eq!(request.transaction_count, 1);
    let mut config = tests::config("unused-uca-budget".into());
    config.authentication.body_bytes = 4063;
    assert!(request.reservation(&config).is_err());
    config.authentication.body_bytes = 4064;
    assert_eq!(
        request.reservation(&config).unwrap(),
        BatchRequest::retained_reservation(4064, &config).unwrap()
    );
    // Accounting creates no signature, authority, permit or durable result.
    assert!(matches!(
        request.request.raw_transactions,
        compute::CandidateInput::Uca { .. }
    ));
}

#[test]
fn uca_request_rejects_missing_hints_and_oversized_wire_before_acceptance() {
    let reference = tests::request(empty_root());
    assert!(BatchRequest::from_uca(
        vec![vec![1]],
        vec![],
        reference.request.context,
        reference.request.policy.clone(),
    )
    .is_err());
    let request = BatchRequest::from_uca(
        vec![vec![1; 4097]],
        vec![(Account::from([1; 20]), Account::from([2; 20]))],
        reference.request.context,
        reference.request.policy,
    )
    .unwrap();
    let config = tests::config("unused-uca-oversized".into());
    assert!(request.reservation(&config).is_err());
}
