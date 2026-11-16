use super::*;
use crate::native_pipeline::execution::plan::BatchContext;

fn context() -> BatchContext {
    BatchContext {
        chain_id: 71,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [4; 32],
        height: 1,
        slot: 1,
        timestamp_unix_ms: 1,
        parent_height: 0,
        parent_block_hash: [0; 32],
        parent_state_root: [2; 32],
        parent_receipt_root: [0; 32],
        parent_state_version: 0,
        business_program: uca_application::application_program_id_v1(),
        receipt_codec: uca_application::application_receipt_codec_v1(),
        semantic_version: uca_application::APPLICATION_SEMANTIC_VERSION_V1,
        effect_contract: [3; 32],
    }
}

#[test]
fn application_dispatch_requires_all_three_native_v4_pins() {
    let original = context();
    assert!(is_application_context(&original));
    let mut changed = original;
    changed.business_program[0] ^= 1;
    assert!(!is_application_context(&changed));
    changed = original;
    changed.semantic_version = 3;
    assert!(!is_application_context(&changed));
    changed = original;
    changed.receipt_codec[0] ^= 1;
    assert!(!is_application_context(&changed));
}

#[test]
fn nuap_dispatch_never_relabels_legacy_or_nuas_bytes() {
    for raw in [b"NUAP\x01\x41".to_vec(), b"NUAPbad".to_vec()] {
        assert!(
            matches!(raw_input(false, raw.clone()), AdmissionInput::Raw(bytes) if bytes == raw)
        );
        assert!(
            matches!(raw_input(true, raw.clone()), AdmissionInput::Application(bytes) if bytes == raw)
        );
    }
    for raw in [b"NUAS".to_vec(), b"NUA".to_vec(), Vec::new()] {
        assert!(matches!(raw_input(true, raw.clone()), AdmissionInput::Raw(bytes) if bytes == raw));
    }
}

#[test]
fn application_retention_uses_same_capacity_and_metadata_limit() -> Result<()> {
    let mut raw = Vec::with_capacity(8192);
    raw.extend_from_slice(b"NUAPbad");
    let input = AdmissionInput::Application(raw);
    let bytes = crate::native_pipeline::ingress::batch::admission_retained_bytes(
        std::slice::from_ref(&input),
    )?;
    let mut retention = PoolRetention::default();
    assert!(retention.reserve(&input, 128, bytes + 127).is_err());
    assert_eq!(retention.bytes, 0);
    retention.reserve(&input, 128, bytes + 128)?;
    assert_eq!(retention.bytes, bytes + 128);
    retention.release(&input, 128);
    assert_eq!(retention.bytes, 0);
    assert!(retention.owners.is_empty());
    Ok(())
}

#[test]
fn application_revision_cannot_become_asset_nonce_credit() {
    let hash = [1; 32];
    let grant_id = [2; 32];
    let pending = BTreeMap::from([(
        hash,
        Pending {
            input: AdmissionInput::Application(b"NUAPbad".to_vec()),
            admitted: None,
            canonical_len: 7,
            position: PendingPosition::Application(ApplicationPosition {
                grant_id,
                expected_revision: 9,
            }),
            gossip_missing: 0,
        },
    )]);
    assert_eq!(pending[&hash].asset_position(), None);
    let (selected, full, _) = super::super::select_pending_batch_with_budget(
        &VecDeque::from([hash]),
        &pending,
        &BTreeMap::from([(grant_id, 9)]),
        BTreeMap::from([(grant_id, 9)]),
        1,
    );
    assert!(selected.is_empty());
    assert!(!full);
}

#[test]
fn future_asset_nonce_cannot_block_bounded_application_revoke_window() {
    // Selection-only shapes, not fabricated signature facts or authorizations.
    let asset = [1; 32];
    let revoke = [2; 32];
    let other_application = [3; 32];
    let signer = [4; 32];
    let order = VecDeque::from([asset, revoke, other_application]);
    let mut pending = BTreeMap::new();
    pending.insert(
        asset,
        Pending {
            input: AdmissionInput::Raw(vec![0; 8]),
            admitted: None,
            canonical_len: 8,
            position: PendingPosition::Asset { signer, nonce: 77 },
            gossip_missing: 0,
        },
    );
    for (hash, revision) in [(revoke, 1), (other_application, 8)] {
        pending.insert(
            hash,
            Pending {
                input: AdmissionInput::Application(b"NUAPbad".to_vec()),
                admitted: None,
                canonical_len: 7,
                position: PendingPosition::Application(ApplicationPosition {
                    grant_id: hash,
                    expected_revision: revision,
                }),
                gossip_missing: 0,
            },
        );
    }
    let known = BTreeMap::from([(signer, 0)]);
    let (asset_window, no_ready) = super::super::asset_nonce_selection_window(
        &order,
        &pending,
        &known,
        &BTreeMap::new(),
        0,
        3,
    );
    assert!(no_ready);
    assert_eq!(asset_window, VecDeque::from([asset]));
    assert_eq!(
        selection_window(&order, &pending, 3, ACCESS_KEYS_PER_ROW),
        VecDeque::from([revoke])
    );
    assert_eq!(
        selection_window(&order, &pending, 1, 1024),
        VecDeque::from([revoke])
    );
    assert_eq!(
        selection_window(&order, &pending, 3, 1024),
        VecDeque::from([revoke, other_application])
    );
    assert_eq!(pending.len(), 3);
    assert_eq!(order.front(), Some(&asset));
    assert_eq!(known[&signer], 0);
}

#[test]
fn application_selection_access_bound_matches_original_grant_compiler() -> Result<()> {
    let access = ApplicationGrantAccessV1::new(
        novovm_protocol::uca_application::ApplicationGrantSelectorV1 {
            chain_id: 71,
            genesis: [1; 32],
            identity: novovm_protocol::native_query::UcaIdentityIdV1::new("uca:alice")
                .map_err(anyhow::Error::msg)?,
            application: [2; 32],
            service: [3; 32],
            device: [4; 32],
        },
    )?;
    assert_eq!(access.declarations().len(), ACCESS_KEYS_PER_ROW);
    Ok(())
}
