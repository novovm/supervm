//! Synthetic archive content only, not checked signatures or executed capability.
use super::*;
use crate::native_pipeline::business::{
    direct_nov_fee::FeeFailureCode,
    nov_transfer_batch::{
        balance_key, nonce_key, uca_program_id, uca_receipt_codec, NovTransferReceipt,
    },
    quoted_transfer::{Account, BalanceDelta, TransferDelta, TransferFailure},
    uca_application::{
        application_command_id, application_program_id_v1, application_receipt_codec_v1,
        ApplicationGrantAccessV1, APPLICATION_SEMANTIC_VERSION_V1,
    },
};
use crate::native_pipeline::ingress::wire::{
    encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use crate::native_pipeline::state::tree::empty_root;
use ed25519_dalek::{Signer, SigningKey};
use novovm_adapter_api::uca_spend_policy::SpendPolicyError;
use novovm_protocol::{
    native_query::UcaIdentityIdV1,
    uca_application::{
        ApplicationGrantActionV1, ApplicationGrantIntentV1, ApplicationGrantReceiptV1,
        ApplicationGrantSelectorV1, SignedApplicationGrantV1,
    },
    uca_spend::{
        spend_key_ref_v1, AssetSpendAuthorityV3, AssetSpendIntentV3, SignedAssetSpendV3,
        MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
    },
};

struct CodecFixture {
    plan: BatchPlan,
    receipts: Vec<Vec<u8>>,
    outputs: Option<Vec<SpendOutput>>,
}

impl CodecFixture {
    fn packet(&self, budget: PacketBudget) -> Result<PreparedCandidate> {
        let receipt = uca::receipt_hash_with_spend(
            self.plan.context().receipt_codec,
            &self.receipts,
            self.outputs.as_deref(),
        );
        prepare_with_spend(
            &self.plan,
            empty_root(),
            &self.receipts,
            &BTreeMap::new(),
            receipt,
            statement_hash(self.plan.commitment(), empty_root(), receipt),
            budget,
            self.outputs.as_deref(),
        )
    }

    fn rebind(&mut self, context: BatchContext) -> Result<()> {
        self.plan = BatchPlan::new(
            context,
            self.plan.canonical_transactions_for_test(),
            self.plan.declared_access().to_vec(),
            PacketBudget::default().plan(),
        )?;
        Ok(())
    }
}

fn context() -> BatchContext {
    BatchContext {
        chain_id: 9,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: application_program_id_v1(),
        semantic_version: APPLICATION_SEMANTIC_VERSION_V1,
        // Like the existing packet fixtures, this is content, not a policy proof.
        effect_contract: [3; 32],
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: empty_root(),
        parent_receipt_root: [4; 32],
        parent_state_version: 0,
        receipt_codec: application_receipt_codec_v1(),
        height: 1,
        slot: 1,
        timestamp_unix_ms: 1234,
    }
}

fn rejected_receipt(
    hash: NodeHash,
    identity: NodeHash,
    payer: Account,
    recipient: Account,
) -> NovTransferReceipt {
    NovTransferReceipt {
        tx_hash: hash,
        signer_identity: identity,
        delta: TransferDelta {
            tx_hash: hash,
            payer: BalanceDelta {
                account: payer,
                before: 100,
                after: 100,
            },
            recipient: BalanceDelta {
                account: recipient,
                before: 0,
                after: 0,
            },
            nonce_identity: identity_hex(&identity),
            nonce_before: 0,
            nonce_after: 0,
            fee_funding_delta: 0,
        },
        failure: Some(TransferFailure::Authorization("WrongEpoch".into())),
        quote: None,
        fee_failure: None,
        journal: None,
        clear_clearing_candidates: false,
    }
}

fn asset_fixture(legacy: bool) -> Result<CodecFixture> {
    let public_key = vec![7; MLDSA65_PUBLIC_KEY_BYTES];
    let intent = AssetSpendIntentV3 {
        authority: AssetSpendAuthorityV3::Controller,
        asset: 1,
        authority_update: None,
        recipient_creation: None,
        chain_id: 9,
        genesis: [1; 32],
        payer: vec![7; 32],
        authorization_epoch: 1,
        spend_key_ref: spend_key_ref_v1(&public_key)?,
        recipient: vec![8; 32],
        amount: 5,
        max_fee: 1,
        nonce: 0,
        not_before_height: 0,
        expires_at_height: 100,
    };
    let hash = Sha256::digest(intent.signing_bytes()?).into();
    let identity = intent.nonce_identity()?;
    let payer = Account::try_from(intent.payer.clone()).map_err(anyhow::Error::msg)?;
    let recipient = Account::try_from(intent.recipient.clone()).map_err(anyhow::Error::msg)?;
    let receipt = rejected_receipt(hash, identity, payer.clone(), recipient.clone());
    let signed = SignedAssetSpendV3 {
        intent,
        public_key,
        signature: vec![11; MLDSA65_SIGNATURE_BYTES],
    };
    let mut context = context();
    if legacy {
        context.business_program = uca_program_id();
        context.receipt_codec = uca_receipt_codec();
        context.semantic_version = SEMANTIC_VERSION;
    }
    Ok(CodecFixture {
        plan: BatchPlan::new(
            context,
            vec![signed.encode()?],
            [
                balance_key(&payer),
                balance_key(&recipient),
                nonce_key(&identity),
            ]
            .into_iter()
            .map(|key| DeclaredAccess {
                key,
                may_put: true,
                may_delete: false,
            })
            .collect(),
            PacketBudget::default().plan(),
        )?,
        receipts: vec![postcard::to_allocvec(&receipt)?],
        outputs: Some(vec![SpendOutput {
            failure: Some(SpendPolicyError::WrongEpoch),
            remaining: 0,
        }]),
    })
}

fn application_fixture() -> Result<CodecFixture> {
    let public_key = vec![7; MLDSA65_PUBLIC_KEY_BYTES];
    let issuer_key_ref =
        novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1(
            novovm_adapter_api::UcaKeyAlgo::Mldsa65,
            &public_key,
        )
        .try_into()
        .map_err(|_| anyhow::anyhow!("key reference length"))?;
    let selector = ApplicationGrantSelectorV1 {
        chain_id: 9,
        genesis: [1; 32],
        identity: UcaIdentityIdV1::new("uca:archive").map_err(anyhow::Error::msg)?,
        application: [2; 32],
        service: [3; 32],
        device: [4; 32],
    };
    let intent = ApplicationGrantIntentV1 {
        selector: selector.clone(),
        issuer_key_ref,
        expected_revision: 0,
        valid_from_height: 1,
        valid_until_height: 20,
        action: ApplicationGrantActionV1::Authorize {
            not_before_height: 1,
            expires_at_height: 50,
        },
    };
    let receipt = ApplicationGrantReceiptV1 {
        command_id: application_command_id(&intent)?,
        grant_id: selector.grant_id()?,
        issuer_key_ref,
        previous_revision: 0,
        revision: 1,
        revoked: false,
        not_before_height: 1,
        expires_at_height: 50,
    };
    let signed = SignedApplicationGrantV1 {
        intent,
        public_key,
        signature: vec![9; MLDSA65_SIGNATURE_BYTES],
    };
    Ok(CodecFixture {
        plan: BatchPlan::new(
            context(),
            vec![signed.encode()?],
            ApplicationGrantAccessV1::new(selector)?
                .declarations()
                .to_vec(),
            PacketBudget::default().plan(),
        )?,
        receipts: vec![receipt.encode()?],
        outputs: None,
    })
}

fn legacy_v3_fixture() -> Result<CodecFixture> {
    let signer = SigningKey::from_bytes(&[7; 32]);
    let public = signer.verifying_key().to_bytes();
    let mut tx = TransferV3 {
        chain_id: 9,
        from: public.to_vec(),
        to: vec![8; 32],
        asset: "NOV".into(),
        amount: 5,
        nonce: 0,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 1,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    tx.signature = [
        public.as_slice(),
        signer.sign(&signing_message(&tx)?).to_bytes().as_slice(),
    ]
    .concat();
    let mut identity = Sha256::new();
    identity.update(b"novovm-native-auth-nonce-identity-v1");
    identity.update(9u64.to_be_bytes());
    identity.update(b"novovm-native-auth/ed25519-public-key/v2\0");
    identity.update(public);
    let identity: NodeHash = identity.finalize().into();
    let hash = canonical_tx_hash(&tx)?;
    let mut fixture = asset_fixture(true)?;
    let mut receipt = rejected_receipt(
        hash,
        identity,
        Account::try_from(tx.from.clone()).map_err(anyhow::Error::msg)?,
        Account::try_from(tx.to.clone()).map_err(anyhow::Error::msg)?,
    );
    receipt.delta.nonce_after = 1;
    receipt.failure = Some(TransferFailure::Fee("fixture fee cap".into()));
    receipt.fee_failure = Some(FeeFailure {
        code: FeeFailureCode::MaxPayExceeded,
        reason: "fixture fee cap".into(),
    });
    let mut context = *fixture.plan.context();
    context.business_program = program_id();
    context.receipt_codec = receipt_codec();
    fixture.plan = BatchPlan::new(
        context,
        vec![encode_transfer_v3(&tx)?],
        fixture.plan.declared_access().to_vec(),
        PacketBudget::default().plan(),
    )?;
    fixture.receipts = vec![postcard::to_allocvec(&receipt)?];
    fixture.outputs = None;
    Ok(fixture)
}

fn document(packet: &PreparedCandidate) -> Result<Vec<u8>> {
    let marker = Marker::decode(
        &packet.records()[&marker_key(packet.candidate_id())],
        packet.candidate_id(),
        PacketBudget::default(),
    )?;
    Ok((0..marker.chunks)
        .flat_map(|index| packet.records()[&document_key(packet.candidate_id(), index)].clone())
        .collect())
}

fn cold(packet: &PreparedCandidate) -> Result<StoredCandidate> {
    StoredCandidate::load(packet.candidate_id(), PacketBudget::default(), |keys| {
        Ok(keys
            .iter()
            .map(|key| packet.records().get(key).cloned())
            .collect())
    })?
    .context("missing archive")
}

#[test]
fn v4_asset_cold_codec_preserves_typed_outputs_and_asset_projections() -> Result<()> {
    let fixture = asset_fixture(false)?;
    let packet = fixture.packet(PacketBudget::default())?;
    assert_eq!(&document(&packet)?[..8], V4_ASSET_DOCUMENT_MAGIC);
    let stored = cold(&packet)?;
    assert!(packet.matches(&stored));
    assert_eq!(stored.context(), fixture.plan.context());
    assert_eq!(
        stored.raw_transactions(),
        fixture.plan.canonical_transactions_for_test()
    );
    assert_eq!(stored.receipt_bytes(), fixture.receipts);
    assert_eq!(stored.spend_outputs(), fixture.outputs.as_deref());
    assert!(stored.application_receipt(0).is_err());
    assert_eq!(packet.receipt_projection().len(), 1);
    let hot = &packet.receipt_projection()[0];
    let view = stored.receipt_view(0)?;
    assert_eq!(hot.tx_hash, view.tx_hash);
    assert_eq!(hot.signer_identity, view.signer_identity);
    assert_eq!(hot.nonce_after, view.nonce_after);
    assert_eq!(hot.success, view.success);
    assert_eq!(hot.charged_fee.to_string(), view.charged_fee);
    assert_eq!(
        packet.nonce_projection().get(&hot.signer_identity),
        Some(&0)
    );
    assert!(!view.success);
    let exact = PacketBudget {
        max_receipt_bytes: fixture.receipts[0].len() + 32,
        ..PacketBudget::default()
    };
    fixture.packet(exact)?;
    decode_document(&document(&packet)?, packet.candidate_id(), exact)?;
    let too_small = PacketBudget {
        max_receipt_bytes: exact.max_receipt_bytes - 1,
        ..exact
    };
    assert!(fixture.packet(too_small).is_err());
    assert!(packet.validate_budget(too_small).is_err());
    assert!(decode_document(&document(&packet)?, packet.candidate_id(), too_small).is_err());
    Ok(())
}

#[test]
fn v5_asset_archive_preserves_the_original_receipt_and_nonce() -> Result<()> {
    use crate::native_pipeline::business::uca_registration;
    let mut fixture = asset_fixture(false)?;
    fixture.plan = BatchPlan::new(
        BatchContext {
            business_program: uca_registration::program_id(),
            semantic_version: uca_registration::REGISTRATION_SEMANTIC_VERSION,
            receipt_codec: uca_registration::receipt_codec(),
            ..*fixture.plan.context()
        },
        fixture.plan.canonical_transactions_for_test(),
        fixture.plan.declared_access().to_vec(),
        PacketBudget::default().plan(),
    )?;
    let packet = fixture.packet(PacketBudget::default())?;
    let stored = cold(&packet)?;
    assert!(packet.matches(&stored));
    assert_eq!(stored.spend_outputs(), fixture.outputs.as_deref());
    let receipt = stored.receipt_view(0)?;
    assert_eq!(
        packet.nonce_projection().get(&receipt.signer_identity),
        Some(&receipt.nonce_after)
    );
    Ok(())
}

#[test]
fn v4_application_keeps_no_asset_outputs_or_projections() -> Result<()> {
    let fixture = application_fixture()?;
    let packet = fixture.packet(PacketBudget::default())?;
    assert_eq!(&document(&packet)?[..8], APPLICATION_DOCUMENT_MAGIC);
    let stored = cold(&packet)?;
    assert!(packet.matches(&stored));
    assert_eq!(
        stored.application_receipt(0)?,
        ApplicationGrantReceiptV1::decode(&fixture.receipts[0])?
    );
    assert!(stored.receipt_view(0).is_err());
    assert!(stored.spend_outputs().is_none());
    assert!(packet.receipt_projection().is_empty());
    assert!(packet.nonce_projection().is_empty());
    Ok(())
}

#[test]
fn all_document_headers_reject_false_profile_or_branch_relabels() -> Result<()> {
    for (fixture, magic) in [
        (legacy_v3_fixture()?, DOCUMENT_MAGIC),
        (asset_fixture(true)?, UCA_DOCUMENT_MAGIC),
        (application_fixture()?, APPLICATION_DOCUMENT_MAGIC),
        (asset_fixture(false)?, V4_ASSET_DOCUMENT_MAGIC),
    ] {
        let packet = fixture.packet(PacketBudget::default())?;
        let bytes = document(&packet)?;
        assert_eq!(&bytes[..8], magic);
        assert!(packet.matches(&cold(&packet)?));
        for other in [
            DOCUMENT_MAGIC,
            UCA_DOCUMENT_MAGIC,
            APPLICATION_DOCUMENT_MAGIC,
            V4_ASSET_DOCUMENT_MAGIC,
            b"NVCAND02",
        ] {
            if other == magic {
                continue;
            }
            let mut relabeled = bytes.clone();
            relabeled[..8].copy_from_slice(other);
            assert!(
                decode_document(&relabeled, packet.candidate_id(), PacketBudget::default())
                    .is_err(),
                "{magic:?} -> {other:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn v4_branch_selection_and_homogeneity_do_not_follow_relabelled_bodies() -> Result<()> {
    let mut asset = asset_fixture(false)?;
    asset.outputs = None;
    assert!(asset.packet(PacketBudget::default()).is_err());
    let mut application = application_fixture()?;
    application.outputs = Some(vec![SpendOutput {
        failure: None,
        remaining: 0,
    }]);
    assert!(application.packet(PacketBudget::default()).is_err());
    let mut v3 = legacy_v3_fixture()?;
    v3.rebind(context())?;
    assert!(v3.packet(PacketBudget::default()).is_err());
    v3.outputs = Some(vec![SpendOutput {
        failure: None,
        remaining: 0,
    }]);
    assert!(v3.packet(PacketBudget::default()).is_err());
    for asset_branch in [false, true] {
        let mut fixture = if asset_branch {
            asset_fixture(false)?
        } else {
            application_fixture()?
        };
        let other = if asset_branch {
            application_fixture()?
        } else {
            asset_fixture(false)?
        };
        let mut raw = fixture.plan.canonical_transactions_for_test();
        raw.extend(other.plan.canonical_transactions_for_test());
        fixture.plan = BatchPlan::new(
            *fixture.plan.context(),
            raw,
            fixture.plan.declared_access().to_vec(),
            PacketBudget::default().plan(),
        )?;
        fixture.receipts.extend(other.receipts);
        if let Some(outputs) = &mut fixture.outputs {
            outputs.push(SpendOutput {
                failure: None,
                remaining: 0,
            });
        }
        assert!(fixture.packet(PacketBudget::default()).is_err());
    }
    Ok(())
}

#[test]
fn v4_asset_tail_requires_exact_count_known_codes_and_bound_effects() -> Result<()> {
    let packet = asset_fixture(false)?.packet(PacketBudget::default())?;
    let original = document(&packet)?;
    let len = original.len();
    for cut in [1, 32, 40] {
        assert!(decode_document(
            &original[..len - cut],
            packet.candidate_id(),
            PacketBudget::default()
        )
        .is_err());
    }
    for case in 0..5 {
        let mut bytes = original.clone();
        match case {
            0 => bytes[len - 40..len - 32].copy_from_slice(&0u64.to_be_bytes()),
            1 => bytes[len - 40..len - 32].copy_from_slice(&2u64.to_be_bytes()),
            2 => bytes[len - 32..len - 16].copy_from_slice(&999u128.to_be_bytes()),
            3 => bytes[len - 1] = 1,
            _ => bytes.push(0),
        }
        assert!(
            decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err(),
            "tail case {case}"
        );
    }
    // Even deleting the entire asset tail cannot turn NUAS into NUAP.
    let mut stripped = original[..len - 40].to_vec();
    stripped[..8].copy_from_slice(APPLICATION_DOCUMENT_MAGIC);
    assert!(decode_document(&stripped, packet.candidate_id(), PacketBudget::default()).is_err());
    Ok(())
}
