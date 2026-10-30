//! Structural input/budget ownership only; dummy signatures are never accepted
//! as cryptographic or asset authority by these channel-level regressions.
use super::*;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV2, SignedAssetSpendV2, MAX_SIGNED_SPEND_BYTES,
    MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
};
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};

#[test]
fn asset_authentication_retains_signed_accounts_and_charges_them_once() -> Result<()> {
    let public_key = vec![7; MLDSA65_PUBLIC_KEY_BYTES];
    let raw = SignedAssetSpendV2 {
        intent: AssetSpendIntentV2 {
            asset: 1,
            authority_update: None,
            recipient_creation: None,
            chain_id: domain().chain_id,
            genesis: domain().genesis_config_commitment,
            payer: vec![1; 20],
            authorization_epoch: 1,
            spend_key_ref: spend_key_ref_v1(&public_key)?,
            recipient: vec![2; 32],
            amount: 10,
            max_fee: 50,
            nonce: 0,
            not_before_height: 1,
            expires_at_height: 20,
        },
        public_key,
        signature: vec![9; MLDSA65_SIGNATURE_BYTES],
    }
    .encode()?;
    let batch = Arc::new(AssetSpendBatchV2::from_raw(
        std::slice::from_ref(&raw),
        AssetSpendBatchLimitsV2 {
            transactions: 8,
            transaction_bytes: MAX_SIGNED_SPEND_BYTES,
            body_bytes: 65536,
        },
    )?);
    let request = AuthenticationRequest::from_uca_batch(batch.clone(), policy())?;
    assert_eq!(request.body_bytes, raw.len());
    assert_eq!(request.max_transaction_bytes, raw.len());
    assert_eq!(request.transaction_count, 1);
    let mut config = cfg();
    config.authentication.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    config.plan.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    config.authentication.body_bytes = request.body_bytes - 1;
    assert!(
        request.reservation(&config).is_err(),
        "signed input bypassed quota"
    );
    config.authentication.body_bytes = 65536;
    config.plan.body_bytes = 65536;
    config.max_batches = 1;
    let (pipeline, receiver) = inert_pipeline(config);
    let AuthenticationSubmission::Backpressured(returned) =
        pipeline.try_authenticate_owned(request).unwrap()
    else {
        panic!("optional UCA authentication took the only current-height permit")
    };
    let compute::CandidateInput::Uca {
        source: BatchSource::Uca(retained),
    } = returned.request.raw_transactions
    else {
        panic!("NUAS was reinterpreted as unverified V3 input")
    };
    assert!(Arc::ptr_eq(&retained, &batch));
    assert_eq!(retained.signed_accounts(0)?.0, &[1; 20]);
    assert_eq!(retained.signed_accounts(0)?.1, &[2; 32]);
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(usage(&pipeline), (0, 0, 0));
    Ok(())
}
