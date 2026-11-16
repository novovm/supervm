//! Archive-content fixtures do not manufacture checked signatures or execution.
use super::*;
use crate::native_pipeline::state::tree::empty_root;
use novovm_protocol::{native_query::UcaIdentityIdV1, uca_application::*};

fn fixture() -> Result<(
    BatchContext,
    SignedApplicationGrantV1,
    ApplicationGrantReceiptV1,
    Vec<DeclaredAccess>,
)> {
    let public_key = vec![7; novovm_protocol::uca_spend::MLDSA65_PUBLIC_KEY_BYTES];
    let issuer_key_ref =
        novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1(
            novovm_adapter_api::UcaKeyAlgo::Mldsa65,
            &public_key,
        )
        .try_into()
        .map_err(|_| anyhow::anyhow!("key reference length"))?;
    let selector = ApplicationGrantSelectorV1 {
        chain_id: 7,
        genesis: [1; 32],
        identity: UcaIdentityIdV1::new("uca:archive").unwrap(),
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
    let access = ApplicationGrantAccessV1::new(selector)?
        .declarations()
        .to_vec();
    let context = BatchContext {
        chain_id: 7,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: application_program_id_v1(),
        semantic_version: APPLICATION_SEMANTIC_VERSION_V1,
        effect_contract: [3; 32],
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: empty_root(),
        parent_receipt_root: [4; 32],
        parent_state_version: 0,
        receipt_codec: application_receipt_codec_v1(),
        height: 1,
        slot: 1,
        timestamp_unix_ms: 100,
    };
    Ok((
        context,
        SignedApplicationGrantV1 {
            intent,
            public_key,
            signature: vec![9; novovm_protocol::uca_spend::MLDSA65_SIGNATURE_BYTES],
        },
        receipt,
        access,
    ))
}

fn plan(
    context: BatchContext,
    signed: &SignedApplicationGrantV1,
    access: Vec<DeclaredAccess>,
) -> Result<BatchPlan> {
    BatchPlan::new(
        context,
        vec![signed.encode()?],
        access,
        PacketBudget::default().plan(),
    )
}

#[test]
fn application_archive_binds_receipt_body_and_exact_access_without_asset_projection() -> Result<()>
{
    let (context, signed, receipt, access) = fixture()?;
    let plan = plan(context, &signed, access)?;
    let encoded = vec![receipt.encode()?];
    validate_receipts(&plan, &encoded, PacketBudget::default())?;
    let receipt_hash = receipt_hash(context.receipt_codec, &encoded);
    let packet = super::super::prepare(
        &plan,
        empty_root(),
        &encoded,
        &BTreeMap::new(),
        receipt_hash,
        statement_hash(plan.commitment(), empty_root(), receipt_hash),
        PacketBudget::default(),
    )?;
    assert!(packet.receipt_projection().is_empty());
    assert!(packet.nonce_projection().is_empty());
    let stored = StoredCandidate::load(packet.candidate_id(), PacketBudget::default(), |keys| {
        Ok(keys
            .iter()
            .map(|key| packet.records().get(key).cloned())
            .collect())
    })?
    .context("archive missing")?;
    assert!(packet.matches(&stored));
    assert_eq!(stored.application_receipt(0)?, receipt);
    assert!(stored.application_receipt(1).is_err());
    assert!(stored.receipt_view(0).is_err());
    assert!(stored.spend_outputs().is_none());
    let marker = super::super::Marker::decode(
        &packet.records()[&super::super::marker_key(packet.candidate_id())],
        packet.candidate_id(),
        PacketBudget::default(),
    )?;
    let document: Vec<_> = (0..marker.chunks)
        .flat_map(|index| {
            packet.records()[&super::super::document_key(packet.candidate_id(), index)].clone()
        })
        .collect();
    assert_eq!(&document[..8], super::super::APPLICATION_DOCUMENT_MAGIC);
    for old_magic in [
        super::super::DOCUMENT_MAGIC,
        super::super::UCA_DOCUMENT_MAGIC,
    ] {
        let mut relabeled = document.clone();
        relabeled[..8].copy_from_slice(old_magic);
        assert!(super::super::decode_document(
            &relabeled,
            packet.candidate_id(),
            PacketBudget::default()
        )
        .is_err());
    }
    let mut tail = document.clone();
    tail.push(0);
    assert!(
        super::super::decode_document(&tail, packet.candidate_id(), PacketBudget::default())
            .is_err()
    );
    for end in [0, 1, 7, 8, 40, document.len() - 1] {
        assert!(super::super::decode_document(
            &document[..end],
            packet.candidate_id(),
            PacketBudget::default()
        )
        .is_err());
    }
    Ok(())
}

#[test]
fn application_archive_rejects_rebound_receipts_and_widened_permissions() -> Result<()> {
    let (context, signed, receipt, access) = fixture()?;
    let original = plan(context, &signed, access.clone())?;
    for case in 0..7 {
        let mut changed = receipt.clone();
        match case {
            0 => changed.command_id[0] ^= 1,
            1 => changed.grant_id[0] ^= 1,
            2 => changed.issuer_key_ref[0] ^= 1,
            3 => {
                changed.previous_revision += 1;
                changed.revision += 1;
            }
            4 => changed.revoked = true,
            5 => changed.not_before_height += 1,
            _ => changed.expires_at_height += 1,
        }
        assert!(
            validate_receipts(&original, &[changed.encode()?], PacketBudget::default()).is_err(),
            "receipt case {case}"
        );
    }
    for case in 0..5 {
        let mut changed = access.clone();
        match case {
            0 => changed[0].may_put = true,
            1 => changed[34].may_put = false,
            2 => changed[34].may_delete = !changed[34].may_delete,
            3 => {
                changed.pop();
            }
            _ => changed.push(DeclaredAccess {
                key: b"arbitrary-write".to_vec(),
                may_put: true,
                may_delete: false,
            }),
        }
        let plan = plan(context, &signed, changed)?;
        assert!(
            validate_receipts(&plan, &[receipt.encode()?], PacketBudget::default()).is_err(),
            "access case {case}"
        );
    }
    let mut wrong = signed.clone();
    wrong.intent.selector.service[0] ^= 1;
    assert!(validate_receipts(
        &plan(context, &wrong, access.clone())?,
        &[receipt.encode()?],
        PacketBudget::default()
    )
    .is_err());
    let mut wrong = signed.clone();
    wrong.intent.selector.chain_id += 1;
    assert!(validate_receipts(
        &plan(context, &wrong, access.clone())?,
        &[receipt.encode()?],
        PacketBudget::default()
    )
    .is_err());
    let mut expired = context;
    expired.height = 20;
    expired.parent_height = 19;
    expired.parent_block_hash = [8; 32];
    assert!(validate_receipts(
        &plan(expired, &signed, access)?,
        &[receipt.encode()?],
        PacketBudget::default()
    )
    .is_err());
    Ok(())
}
