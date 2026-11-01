//! Structural fixtures only: deliberately synthetic signatures cannot confer
//! signature-checked or executed capabilities. Actual AOEM tests use the public
//! PreparedCandidate::from_executed constructor with real verified signatures.
use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{
    balance_key, nonce_key, uca_program_id, uca_receipt_codec,
};
use novovm_adapter_api::uca_spend_policy::SpendPolicyError;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV3, SignedAssetSpendV3, MLDSA65_PUBLIC_KEY_BYTES,
    MLDSA65_SIGNATURE_BYTES,
};

struct UcaFixture {
    base: Fixture,
    signed: Vec<SignedAssetSpendV3>,
    outputs: Vec<SpendOutput>,
}

impl UcaFixture {
    fn new() -> Self {
        let mut base = Fixture::new(&[0, 0], 0, 1);
        let key = vec![7; MLDSA65_PUBLIC_KEY_BYTES];
        let mut signed = Vec::new();
        let payer = Account::try_from(vec![7; 32]).unwrap();
        let recipient = Account::try_from(vec![8; 32]).unwrap();
        for index in 0..2 {
            let intent = AssetSpendIntentV3 {
                authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
                asset: 1,
                authority_update: None,
                recipient_creation: None,
                chain_id: 9,
                genesis: [1; 32],
                payer: payer.as_bytes().to_vec(),
                authorization_epoch: index + 1,
                spend_key_ref: spend_key_ref_v1(&key).unwrap(),
                recipient: recipient.as_bytes().to_vec(),
                amount: 5,
                max_fee: 1,
                nonce: 0,
                not_before_height: 0,
                expires_at_height: 100,
            };
            let hash = Sha256::digest(intent.signing_bytes().unwrap()).into();
            let identity = intent.nonce_identity().unwrap();
            let receipt = &mut base.receipts[index as usize];
            receipt.tx_hash = hash;
            receipt.signer_identity = identity;
            receipt.delta.tx_hash = hash;
            receipt.delta.nonce_identity = identity_hex(&identity);
            receipt.delta.payer.account = payer.clone();
            receipt.delta.recipient.account = recipient.clone();
            if index == 0 {
                receipt.failure = Some(TransferFailure::Authorization("WrongEpoch".into()));
                receipt.fee_failure = None;
                receipt.delta.nonce_after = 0;
            }
            signed.push(SignedAssetSpendV3 {
                intent,
                public_key: key.clone(),
                signature: vec![11; MLDSA65_SIGNATURE_BYTES],
            });
        }
        let mut context = *base.plan.context();
        context.business_program = uca_program_id();
        context.receipt_codec = uca_receipt_codec();
        context.semantic_version = 1;
        let access = [
            balance_key(&payer),
            balance_key(&recipient),
            nonce_key(&signed[0].intent.nonce_identity().unwrap()),
        ]
        .into_iter()
        .map(|key| DeclaredAccess {
            key,
            may_put: true,
            may_delete: false,
        })
        .collect();
        base.plan = BatchPlan::new(
            context,
            signed.iter().map(|s| s.encode().unwrap()).collect(),
            access,
            PacketBudget::default().plan(),
        )
        .unwrap();
        Self {
            base,
            signed,
            outputs: vec![
                SpendOutput {
                    failure: Some(SpendPolicyError::WrongEpoch),
                    remaining: 100,
                },
                SpendOutput {
                    failure: None,
                    remaining: 100,
                },
            ],
        }
    }
    fn update_plan(&mut self) {
        self.base.plan = BatchPlan::new(
            *self.base.plan.context(),
            self.signed.iter().map(|s| s.encode().unwrap()).collect(),
            self.base.plan.declared_access().to_vec(),
            PacketBudget::default().plan(),
        )
        .unwrap();
    }
    fn packet(&self) -> Result<PreparedCandidate> {
        let receipts = self
            .base
            .receipts
            .iter()
            .map(|r| postcard::to_allocvec(r).unwrap())
            .collect::<Vec<_>>();
        let commitment = uca::receipt_hash_with_spend(
            self.base.plan.context().receipt_codec,
            &receipts,
            Some(&self.outputs),
        );
        prepare_with_spend(
            &self.base.plan,
            self.base.root,
            &receipts,
            &self.base.nodes,
            commitment,
            statement_hash(self.base.plan.commitment(), self.base.root, commitment),
            PacketBudget::default(),
            Some(&self.outputs),
        )
    }
}

#[test]
fn controller_packet_cannot_claim_a_delegated_budget() {
    let mut f = UcaFixture::new();
    for signed in &mut f.signed {
        signed.intent.authority = novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller;
    }
    for (signed, receipt) in f.signed.iter().zip(&mut f.base.receipts) {
        let hash = Sha256::digest(signed.intent.signing_bytes().unwrap()).into();
        receipt.tx_hash = hash;
        receipt.delta.tx_hash = hash;
    }
    f.update_plan();
    assert!(f.packet().is_err());
    for output in &mut f.outputs {
        output.remaining = 0;
    }
    assert!(f.packet().is_ok());
}

#[test]
fn uca_packet_roundtrip_preserves_authorization_budget_and_unchanged_nonce() {
    let f = UcaFixture::new();
    let packet = f.packet().unwrap();
    assert_eq!(&document(&packet)[..8], UCA_DOCUMENT_MAGIC);
    let cold = load(&packet, packet.records(), PacketBudget::default())
        .unwrap()
        .unwrap();
    assert!(packet.matches(&cold));
    assert_eq!(cold.spend_outputs(), Some(f.outputs.as_slice()));
    assert_eq!(cold.receipt_view(0).unwrap().nonce_after, 0);
    assert_eq!(cold.receipt_view(1).unwrap().nonce_after, 1);
    assert!(!cold.receipt_view(0).unwrap().success);
    assert_eq!(cold.raw_transactions()[0], f.signed[0].encode().unwrap());
}

#[test]
fn asset_structural_packet_reuses_plan_and_matches_independent_cold_decoder() {
    use crate::native_pipeline::ingress::batch::BatchSource;
    use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};
    use std::sync::Arc;

    let mut f = UcaFixture::new();
    let expected = f.packet().unwrap();
    let raw = f.base.plan.canonical_transactions_for_test();
    let batch = Arc::new(
        AssetSpendBatchV3::from_raw(
            &raw,
            AssetSpendBatchLimitsV3 {
                transactions: 1024,
                transaction_bytes: 8192,
                body_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap(),
    );
    f.base.plan = BatchPlan::from_source(
        *f.base.plan.context(),
        BatchSource::Uca(Arc::clone(&batch)),
        f.base.plan.declared_access().to_vec(),
        PacketBudget::default().plan(),
    )
    .unwrap();
    assert!(f.base.plan.raw_transactions_if_materialized().is_none());
    let actual = f.packet().unwrap();
    assert_eq!(actual.records(), expected.records());
    assert_eq!(actual.retained_bytes(), expected.retained_bytes());
    assert_eq!(actual.receipt_projection(), expected.receipt_projection());
    assert!(f.base.plan.raw_transactions_if_materialized().is_none());
    assert_eq!(Arc::strong_count(&batch), 2);

    let cold = load(&actual, actual.records(), PacketBudget::default())
        .unwrap()
        .unwrap();
    assert!(actual.matches(&cold));
    assert_eq!(cold.raw_transactions(), raw);
    assert_eq!(cold.spend_outputs(), Some(f.outputs.as_slice()));
    for (index, projection) in actual.receipt_projection().iter().enumerate() {
        let view = cold.receipt_view(index).unwrap();
        assert_eq!(projection.tx_hash, view.tx_hash);
        assert_eq!(projection.signer_identity, view.signer_identity);
        assert_eq!(projection.nonce_after, view.nonce_after);
        assert_eq!(projection.success, view.success);
        assert_eq!(projection.charged_fee.to_string(), view.charged_fee);
    }
}

#[test]
fn typed_asset_preparation_keeps_commitment_root_and_writable_access_checks() {
    let f = UcaFixture::new();
    let good = f.packet().unwrap();
    let receipts = f
        .base
        .receipts
        .iter()
        .map(|receipt| postcard::to_allocvec(receipt).unwrap())
        .collect::<Vec<_>>();
    let prepare = |root, receipt, statement| {
        prepare_with_spend(
            &f.base.plan,
            root,
            &receipts,
            &f.base.nodes,
            receipt,
            statement,
            PacketBudget::default(),
            Some(&f.outputs),
        )
    };
    assert!(prepare(
        [0; 32],
        good.receipt_batch_commitment(),
        good.statement_commitment()
    )
    .is_err());
    assert!(prepare(f.base.root, [91; 32], good.statement_commitment()).is_err());
    assert!(prepare(f.base.root, good.receipt_batch_commitment(), [92; 32]).is_err());

    let mut f = UcaFixture::new();
    let mut access = f.base.plan.declared_access().to_vec();
    for entry in &mut access {
        entry.may_put = false;
    }
    f.base.plan = BatchPlan::new(
        *f.base.plan.context(),
        f.base.plan.canonical_transactions_for_test(),
        access,
        PacketBudget::default().plan(),
    )
    .unwrap();
    assert!(f.packet().is_err());

    let mut f = UcaFixture::new();
    f.base.receipts[1].delta.recipient.after = 1;
    assert!(f.packet().is_err());
}

#[test]
fn uca_packet_rejects_authorization_effects_and_budget_prefix_changes() {
    let mut f = UcaFixture::new();
    f.base.receipts[0].delta.nonce_after = 1;
    assert!(f.packet().is_err());
    let mut f = UcaFixture::new();
    f.base.receipts[0].delta.fee_funding_delta = 1;
    assert!(f.packet().is_err());
    let mut f = UcaFixture::new();
    f.outputs[1].remaining = 99;
    assert!(f.packet().is_err());
    let mut f = UcaFixture::new();
    f.outputs[0].failure = Some(SpendPolicyError::Revoked);
    assert!(f.packet().is_err());
    let mut f = UcaFixture::new();
    f.base.receipts[1].delta.nonce_before = 1;
    f.base.receipts[1].delta.nonce_after = 2;
    assert!(f.packet().is_err());
}

#[test]
fn uca_packet_rejects_domain_relabel_old_document_and_missing_outputs() {
    let mut f = UcaFixture::new();
    f.signed[0].intent.genesis = [99; 32];
    f.update_plan();
    assert!(f.packet().is_err());
    let f = UcaFixture::new();
    let packet = f.packet().unwrap();
    let mut bytes = document(&packet);
    bytes[..8].copy_from_slice(DOCUMENT_MAGIC);
    assert!(decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err());
    bytes[..8].copy_from_slice(b"NVCAND02");
    assert!(decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err());
    let mut bytes = document(&packet);
    bytes.truncate(bytes.len() - 1);
    assert!(decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err());
    let mut f = UcaFixture::new();
    f.outputs.clear();
    assert!(f.packet().is_err());
}

#[test]
fn uca_packet_commitment_covers_effect_tail_and_rejects_unknown_codes() {
    let f = UcaFixture::new();
    let packet = f.packet().unwrap();
    let mut bytes = document(&packet);
    let n = bytes.len();
    // Change BOTH unchanged budgets identically: prefix checks still hold, so
    // this must fail specifically at the executed receipt commitment boundary.
    bytes[n - 33] ^= 1;
    bytes[n - 1] ^= 1;
    let error = match decode_document(&bytes, packet.candidate_id(), PacketBudget::default()) {
        Ok(_) => panic!("changed budget escaped receipt commitment"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("ordered receipt commitment"));
    let mut bytes = document(&packet);
    // Last row starts with its fixed u128 authorization code.
    bytes[n - 32..n - 16].copy_from_slice(&999u128.to_be_bytes());
    assert!(decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err());
    let mut bytes = document(&packet);
    bytes.extend_from_slice(&[0]);
    assert!(decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err());
}

#[test]
fn uca_packet_enforces_signed_reservation_even_when_actual_fee_is_zero() {
    let mut f = UcaFixture::new();
    f.outputs[0].remaining = 5;
    f.outputs[1].remaining = 5;
    assert!(f.packet().is_err()); // amount 5 + signed cap 1 needs reserve 6.
}

#[test]
fn asset_management_packet_rejects_budget_nonce_fee_and_signed_account_tampering() {
    use novovm_protocol::uca_spend::AssetAuthorityUpdateV2;
    let rejected_management = || {
        let mut f = UcaFixture::new();
        f.signed.truncate(1);
        f.outputs.truncate(1);
        f.base.receipts.truncate(1);
        let intent = &mut f.signed[0].intent;
        intent.recipient = intent.payer.clone();
        intent.amount = 0;
        intent.authority = novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller;
        intent.authority_update = Some(AssetAuthorityUpdateV2 {
            use_recovery: false,
            next_epoch: intent.authorization_epoch + 1,
            active: true,
            management_key_ref: [21; 32],
            recovery_key_ref: None,
            grants: vec![],
        });
        let hash = Sha256::digest(intent.signing_bytes().unwrap()).into();
        let receipt = &mut f.base.receipts[0];
        receipt.tx_hash = hash;
        receipt.delta.tx_hash = hash;
        receipt.delta.recipient = receipt.delta.payer.clone();
        f.outputs[0].remaining = 0;
        f.update_plan();
        f
    };
    // Structural fixture only: a rejected action has no grant/nonce/money
    // effects. Actual manager cryptography is tested by the original pipeline.
    assert!(rejected_management().packet().is_ok());
    let mut f = rejected_management();
    f.outputs[0].remaining = 1;
    assert!(f.packet().is_err());
    let mut f = rejected_management();
    f.base.receipts[0].delta.nonce_after += 1;
    assert!(f.packet().is_err());
    let mut f = rejected_management();
    f.base.receipts[0].delta.fee_funding_delta = 1;
    assert!(f.packet().is_err());
    let mut f = rejected_management();
    f.signed[0].intent.payer[0] ^= 1;
    f.signed[0].intent.recipient = f.signed[0].intent.payer.clone();
    f.update_plan();
    assert!(f.packet().is_err());
}
