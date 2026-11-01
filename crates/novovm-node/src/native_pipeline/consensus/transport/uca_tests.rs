//! Structural transport tests with dummy signature bytes. These do NOT claim
//! ML-DSA verification, spending authorization, execution or network finality.
use super::*;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV3, SignedAssetSpendV3, MAX_SIGNED_SPEND_BYTES,
    MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
};

struct Fixture {
    context: BatchContext,
    policy: DirectNovFeePolicy,
    raw: Vec<Vec<u8>>,
    batch: Arc<AssetSpendBatchV3>,
    limits: DecodeLimits,
}

fn fixture() -> Result<Fixture> {
    // Existing deterministic fee-policy fixture, not approved genesis parameters.
    let policy = DirectNovFeePolicy {
        quote_ttl_ms: 15000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    };
    let context = BatchContext {
        chain_id: 77,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: uca_program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: uca_effect_contract(&policy)?,
        parent_block_hash: [6; 32],
        parent_height: 1,
        parent_state_root: [8; 32],
        parent_receipt_root: [9; 32],
        parent_state_version: 10,
        receipt_codec: uca_receipt_codec(),
        height: 2,
        slot: 12,
        timestamp_unix_ms: 13,
    };
    let limits = DecodeLimits {
        transactions: 4,
        transaction_bytes: MAX_SIGNED_SPEND_BYTES,
        body_bytes: 4 * MAX_SIGNED_SPEND_BYTES,
        message_bytes: 128 * 1024,
    };
    let public_key = vec![7; MLDSA65_PUBLIC_KEY_BYTES];
    let raw = (0..4)
        .map(|nonce| {
            SignedAssetSpendV3 {
                intent: AssetSpendIntentV3 {
                    authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
                    asset: 1,
                    authority_update: None,
                    recipient_creation: None,
                    chain_id: context.chain_id,
                    genesis: context.genesis_config_commitment,
                    payer: vec![1; 20],
                    authorization_epoch: 1,
                    spend_key_ref: spend_key_ref_v1(&public_key)?,
                    recipient: vec![2; 32],
                    amount: 8 + u128::from(nonce),
                    max_fee: 20,
                    nonce,
                    not_before_height: 1,
                    expires_at_height: 20,
                },
                public_key: public_key.clone(),
                signature: vec![nonce as u8; MLDSA65_SIGNATURE_BYTES],
            }
            .encode()
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let batch = Arc::new(AssetSpendBatchV3::from_raw(&raw, batch_limits(limits))?);
    Ok(Fixture {
        context,
        policy,
        raw,
        batch,
        limits,
    })
}

pub(in super::super) fn recovery_specimen() -> Result<(Message, DecodeLimits)> {
    let f = fixture()?;
    Ok((
        Message::AssetBody {
            context: f.context,
            batch: f.batch,
        },
        f.limits,
    ))
}

#[test]
fn nuas_body_uses_explicit_tag_and_existing_bounded_body_lane() -> Result<()> {
    let f = fixture()?;
    let message = Message::AssetBody {
        context: f.context,
        batch: f.batch.clone(),
    };
    let encoded = encode(&message, f.limits)?;
    assert_eq!(encoded[10], 14);
    assert_eq!(&encoded[11..13], &2u16.to_be_bytes());
    assert_eq!(message_lane(&encoded)?, 1);
    assert_eq!(super::super::decode(&encoded, f.limits)?, message);
    assert_eq!(
        candidate_body_id(&message, f.limits)?,
        uca_body_id(&f.context, &f.batch, f.limits)?
    );
    assert!(candidate_request(&message, &f.policy).is_ok());
    let domain = fragment_domain(
        f.context.chain_id,
        f.context.genesis_config_commitment,
        f.context.protocol_commitment,
    );
    assert!(prepare_message(domain, &message, f.limits).is_ok());
    for (index, raw) in f.raw.iter().enumerate() {
        assert_eq!(&f.batch.raw_transaction(index)?, raw);
    }
    Ok(())
}

#[test]
fn asset_body_reference_roundtrip_keeps_exact_pool_order_and_body_identity() -> Result<()> {
    let f = fixture()?;
    let message = uca_body_reference(&f.context, &f.batch, f.limits)?;
    let Message::AssetBodyReference {
        context,
        body_id,
        transactions,
    } = &message
    else {
        anyhow::bail!("expected asset reference");
    };
    assert_eq!(*context, f.context);
    assert_eq!(*body_id, uca_body_id(&f.context, &f.batch, f.limits)?);
    let expected = f
        .raw
        .iter()
        .map(|raw| {
            let signed = SignedAssetSpendV3::decode(raw)?;
            Ok(Sha256::digest(signed.intent.signing_bytes()?).into())
        })
        .collect::<Result<Vec<Hash>>>()?;
    assert_eq!(*transactions, expected);
    let encoded = encode(&message, f.limits)?;
    assert_eq!(encoded[10], 20);
    assert_eq!(&encoded[11..13], &2u16.to_be_bytes());
    assert_eq!(encoded.len(), 357 + 4 * 32);
    assert_eq!(message_lane(&encoded)?, 1);
    assert_eq!(super::super::decode(&encoded, f.limits)?, message);
    assert!(candidate_body_id(&message, f.limits).is_err());
    assert!(candidate_request(&message, &f.policy).is_err());
    assert!(encode_recovery_reply(&message, f.limits).is_err());
    Ok(())
}

#[test]
fn asset_body_reference_keeps_commitment_distinct_from_intent_locators() -> Result<()> {
    let f = fixture()?;
    let original = uca_body_reference(&f.context, &f.batch, f.limits)?;
    let Message::AssetBodyReference {
        body_id,
        transactions,
        ..
    } = &original
    else {
        anyhow::bail!("expected asset reference");
    };
    let mut reordered = f.raw.clone();
    reordered.swap(0, 1);
    let reordered = AssetSpendBatchV3::from_raw(&reordered, batch_limits(f.limits))?;
    let Message::AssetBodyReference {
        body_id: reordered_id,
        transactions: reordered_hashes,
        ..
    } = uca_body_reference(&f.context, &reordered, f.limits)?
    else {
        anyhow::bail!("expected asset reference");
    };
    assert_ne!(*body_id, reordered_id);
    let mut expected = transactions.clone();
    expected.swap(0, 1);
    assert_eq!(expected, reordered_hashes);

    let mut raw = f.raw.clone();
    *raw[0].last_mut().unwrap() ^= 1;
    let modified = AssetSpendBatchV3::from_raw(&raw, batch_limits(f.limits))?;
    let Message::AssetBodyReference {
        body_id: modified_id,
        transactions: modified_hashes,
        ..
    } = uca_body_reference(&f.context, &modified, f.limits)?
    else {
        anyhow::bail!("expected asset reference");
    };
    assert_eq!(*transactions, modified_hashes);
    assert_ne!(
        *body_id, modified_id,
        "intent locator is not a signature fact"
    );

    let mut signed = SignedAssetSpendV3::decode(&f.raw[0])?;
    signed.public_key[0] ^= 1;
    signed.intent.spend_key_ref = spend_key_ref_v1(&signed.public_key)?;
    let mut raw = f.raw.clone();
    raw[0] = signed.encode()?;
    let rekeyed = AssetSpendBatchV3::from_raw(&raw, batch_limits(f.limits))?;
    assert_ne!(
        original,
        uca_body_reference(&f.context, &rekeyed, f.limits)?
    );

    // The decoder preserves an untrusted claimed commitment; only rebuilding
    // the exact full body can establish whether the claimed ID is truthful.
    let mut claimed = original;
    let Message::AssetBodyReference { body_id: claim, .. } = &mut claimed else {
        unreachable!();
    };
    claim[0] ^= 1;
    assert_ne!(*claim, uca_body_id(&f.context, &f.batch, f.limits)?);
    assert_eq!(
        super::super::decode(&encode(&claimed, f.limits)?, f.limits)?,
        claimed
    );
    assert!(candidate_request(&claimed, &f.policy).is_err());
    Ok(())
}

#[test]
fn asset_body_reference_carries_every_context_field_and_rejects_wrong_profile() -> Result<()> {
    let f = fixture()?;
    let original = uca_body_reference(&f.context, &f.batch, f.limits)?;
    let changes: [fn(&mut BatchContext); 12] = [
        |c| c.chain_id += 1,
        |c| c.genesis_config_commitment[0] ^= 1,
        |c| c.protocol_commitment[0] ^= 1,
        |c| c.effect_contract[0] ^= 1,
        |c| c.parent_block_hash[0] ^= 1,
        |c| c.parent_height += 1,
        |c| c.parent_state_root[0] ^= 1,
        |c| c.parent_receipt_root[0] ^= 1,
        |c| c.parent_state_version += 1,
        |c| c.height += 1,
        |c| c.slot += 1,
        |c| c.timestamp_unix_ms += 1,
    ];
    for change in changes {
        let mut context = f.context;
        change(&mut context);
        let changed = uca_body_reference(&context, &f.batch, f.limits)?;
        assert_ne!(changed, original);
        let Message::AssetBodyReference { body_id, .. } = &changed else {
            unreachable!();
        };
        assert_ne!(*body_id, uca_body_id(&f.context, &f.batch, f.limits)?);
        assert_eq!(
            super::super::decode(&encode(&changed, f.limits)?, f.limits)?,
            changed
        );
    }
    let encoded = encode(&original, f.limits)?;
    for offset in [72usize, 104, 252] {
        let mut bad = encoded.clone();
        bad[PREFIX_BYTES + 2 + offset] ^= 1;
        assert!(super::super::decode(&bad, f.limits).is_err());
    }
    Ok(())
}

#[test]
fn asset_body_reference_preflights_exact_length_version_and_resource_bounds() -> Result<()> {
    let f = fixture()?;
    let message = uca_body_reference(&f.context, &f.batch, f.limits)?;
    let encoded = encode(&message, f.limits)?;
    for end in 0..encoded.len() {
        assert!(super::super::decode(&encoded[..end], f.limits).is_err());
    }
    let mut bad = encoded.clone();
    bad.push(0);
    assert!(super::super::decode(&bad, f.limits).is_err());
    let mut bad = encoded.clone();
    bad[12] = 1;
    assert!(super::super::decode(&bad, f.limits).is_err());
    for tag in [14, 15] {
        let mut bad = encoded.clone();
        bad[10] = tag;
        assert!(super::super::decode(&bad, f.limits).is_err());
    }
    let count_offset = PREFIX_BYTES + 2 + CONTEXT_BYTES + 32;
    for count in [0u32, 3, 5, u32::MAX] {
        let mut bad = encoded.clone();
        bad[count_offset..count_offset + 4].copy_from_slice(&count.to_be_bytes());
        assert!(super::super::decode(&bad, f.limits).is_err());
    }
    for limits in [
        DecodeLimits {
            transactions: 3,
            ..f.limits
        },
        DecodeLimits {
            transaction_bytes: 1,
            body_bytes: 127,
            ..f.limits
        },
        DecodeLimits {
            transaction_bytes: 1,
            body_bytes: 128,
            message_bytes: encoded.len() - 1,
            ..f.limits
        },
    ] {
        assert!(encode(&message, limits).is_err());
        assert!(super::super::decode(&encoded, limits).is_err());
    }
    let exact = DecodeLimits {
        transaction_bytes: 1,
        body_bytes: 128,
        message_bytes: encoded.len(),
        ..f.limits
    };
    assert_eq!(super::super::decode(&encoded, exact)?, message);
    assert_eq!(encode(&message, exact)?, encoded);
    // Metadata can fit while the underlying signed body does not. Building a
    // reference from an actual batch still validates the full original budget.
    assert!(uca_body_reference(&f.context, &f.batch, exact).is_err());

    let mut empty = message;
    let Message::AssetBodyReference { transactions, .. } = &mut empty else {
        unreachable!();
    };
    transactions.clear();
    assert!(encode(&empty, f.limits).is_err());
    Ok(())
}

#[test]
fn nuas_body_identity_reuses_exact_wire_length_without_changing_transcript() -> Result<()> {
    let f = fixture()?;
    for count in [1, f.raw.len()] {
        let batch = Arc::new(AssetSpendBatchV3::from_raw(
            &f.raw[..count],
            batch_limits(f.limits),
        )?);
        let message = Message::AssetBody {
            context: f.context,
            batch: batch.clone(),
        };
        let encoded = encode(&message, f.limits)?;
        let (id, wire_len) = candidate_body_id_and_wire_len(&message, f.limits)?;
        assert_eq!(wire_len, Some(encoded.len()));
        assert_eq!(id, candidate_body_id(&message, f.limits)?);
        assert_eq!(id, uca_body_id(&f.context, &batch, f.limits)?);

        // Derive the identity independently from the public wire encoder. Its
        // transcript excludes the outer magic/version/tag, including its size.
        let payload = &encoded[PREFIX_BYTES..];
        let mut expected = Sha256::new();
        expected.update(b"novovm-round-bft-transport/v1/nuas-body/2\0");
        expected.update((payload.len() as u64).to_be_bytes());
        expected.update(payload);
        let expected: Hash = expected.finalize().into();
        assert_eq!(id, expected);
        let mut wrong_length = Sha256::new();
        wrong_length.update(b"novovm-round-bft-transport/v1/nuas-body/2\0");
        wrong_length.update((encoded.len() as u64).to_be_bytes());
        wrong_length.update(payload);
        let wrong_length: Hash = wrong_length.finalize().into();
        assert_ne!(id, wrong_length);
        let mut wrong_payload = Sha256::new();
        wrong_payload.update(b"novovm-round-bft-transport/v1/nuas-body/2\0");
        wrong_payload.update((encoded.len() as u64).to_be_bytes());
        wrong_payload.update(&encoded);
        let wrong_payload: Hash = wrong_payload.finalize().into();
        assert_ne!(id, wrong_payload);

        if count == 1 {
            // One row has enough column framing to isolate the wire ceiling
            // without violating the independent expanded-body budget first.
            let exact = DecodeLimits {
                transaction_bytes: batch.max_transaction_bytes(),
                body_bytes: batch.canonical_bytes(),
                message_bytes: encoded.len(),
                ..f.limits
            };
            exact.validate()?;
            assert_eq!(encode(&message, exact)?, encoded);
            assert_eq!(
                candidate_body_id_and_wire_len(&message, exact)?,
                (id, Some(encoded.len()))
            );
            let short = DecodeLimits {
                message_bytes: encoded.len() - 1,
                ..exact
            };
            short.validate()?;
            assert!(encode(&message, short).is_err());
            assert!(candidate_body_id_and_wire_len(&message, short).is_err());
            assert!(uca_body_id(&f.context, &batch, short).is_err());
        }
    }
    let legacy = Message::Body {
        context: super::super::tests::context(),
        raw_transactions: vec![vec![1, 2], vec![3, 4, 5]],
    };
    let legacy_limits = super::super::tests::limits();
    assert_eq!(
        candidate_body_id_and_wire_len(&legacy, legacy_limits)?,
        (candidate_body_id(&legacy, legacy_limits)?, None)
    );
    let non_body = Message::RequestBody { body_id: [1; 32] };
    assert!(candidate_body_id_and_wire_len(&non_body, f.limits).is_err());
    Ok(())
}

#[test]
fn nuas_body_rejects_versions_truncation_trailing_and_each_resource_limit() -> Result<()> {
    let f = fixture()?;
    let message = Message::AssetBody {
        context: f.context,
        batch: f.batch.clone(),
    };
    let encoded = encode(&message, f.limits)?;
    for end in [0, 10, 12, 13 + CONTEXT_BYTES, encoded.len() - 1] {
        assert!(super::super::decode(&encoded[..end], f.limits).is_err());
    }
    let mut bad = encoded.clone();
    bad[12] = 1; // The superseded UCA-addressed body is not a fallback.
    assert!(super::super::decode(&bad, f.limits).is_err());
    let mut bad = encoded.clone();
    bad.push(0);
    assert!(super::super::decode(&bad, f.limits).is_err());
    for restricted in [
        DecodeLimits {
            transactions: 3,
            ..f.limits
        },
        DecodeLimits {
            transaction_bytes: f.batch.max_transaction_bytes() - 1,
            ..f.limits
        },
        DecodeLimits {
            body_bytes: f.batch.canonical_bytes() - 1,
            ..f.limits
        },
    ] {
        assert!(encode(&message, restricted).is_err());
        assert!(super::super::decode(&encoded, restricted).is_err());
        assert!(uca_body_id(&f.context, &f.batch, restricted).is_err());
    }
    // A single row's explicit column framing is larger than its canonical
    // NUAS bytes, so isolate message_bytes without first violating body_bytes.
    let single = Arc::new(AssetSpendBatchV3::from_raw(
        &f.raw[..1],
        batch_limits(f.limits),
    )?);
    let message = Message::AssetBody {
        context: f.context,
        batch: single.clone(),
    };
    let encoded = encode(&message, f.limits)?;
    let restricted = DecodeLimits {
        transaction_bytes: single.max_transaction_bytes(),
        body_bytes: single.canonical_bytes(),
        message_bytes: encoded.len() - 1,
        ..f.limits
    };
    restricted.validate()?;
    assert!(encode(&message, restricted).is_err());
    assert!(super::super::decode(&encoded, restricted).is_err());
    assert!(uca_body_id(&f.context, &single, restricted).is_err());
    Ok(())
}

#[test]
fn nuas_body_profile_and_policy_cannot_reinterpret_v3() -> Result<()> {
    let f = fixture()?;
    let mut contexts = [f.context; 3];
    contexts[0].business_program = program_id();
    contexts[1].receipt_codec = receipt_codec();
    contexts[2].semantic_version = SEMANTIC_VERSION + 1;
    let original = encode(
        &Message::AssetBody {
            context: f.context,
            batch: f.batch.clone(),
        },
        f.limits,
    )?;
    // Context starts after transport prefix + NUAS payload version. Mutations
    // here test the receiving decoder, not just a local encoder's rejection.
    for (context, context_offset) in contexts.into_iter().zip([72usize, 252, 104]) {
        let message = Message::AssetBody {
            context,
            batch: f.batch.clone(),
        };
        assert!(encode(&message, f.limits).is_err());
        assert!(candidate_request(&message, &f.policy).is_err());
        let mut bad = original.clone();
        bad[PREFIX_BYTES + 2 + context_offset] ^= 1;
        assert!(super::super::decode(&bad, f.limits).is_err());
    }
    let mut wrong_policy = f.context;
    wrong_policy.effect_contract[0] ^= 1;
    assert!(candidate_request(
        &Message::AssetBody {
            context: wrong_policy,
            batch: f.batch.clone()
        },
        &f.policy
    )
    .is_err());
    let raw_body = Message::Body {
        context: f.context,
        raw_transactions: f.raw,
    };
    assert!(candidate_request(&raw_body, &f.policy).is_err());
    Ok(())
}

#[test]
fn nuas_body_identity_commits_order_signatures_signed_accounts_and_parent() -> Result<()> {
    let f = fixture()?;
    let original = uca_body_id(&f.context, &f.batch, f.limits)?;
    let mut raw = f.raw.clone();
    raw.swap(0, 1);
    let reordered = AssetSpendBatchV3::from_raw(&raw, batch_limits(f.limits))?;
    assert_ne!(original, uca_body_id(&f.context, &reordered, f.limits)?);
    let mut raw = f.raw.clone();
    *raw[0].last_mut().unwrap() ^= 1;
    let modified_signature = AssetSpendBatchV3::from_raw(&raw, batch_limits(f.limits))?;
    assert_ne!(
        original,
        uca_body_id(&f.context, &modified_signature, f.limits)?
    );
    for account_column in 0..2 {
        let mut raw = f.raw.clone();
        let mut signed = SignedAssetSpendV3::decode(&raw[0])?;
        if account_column == 0 {
            signed.intent.payer[0] ^= 1;
        } else {
            signed.intent.recipient[0] ^= 1;
        }
        raw[0] = signed.encode()?;
        let modified_account = AssetSpendBatchV3::from_raw(&raw, batch_limits(f.limits))?;
        assert_ne!(
            original,
            uca_body_id(&f.context, &modified_account, f.limits)?
        );
    }
    let mut parent = f.context;
    parent.parent_state_root[0] ^= 1;
    assert_ne!(original, uca_body_id(&parent, &f.batch, f.limits)?);
    Ok(())
}

fn input_scope(f: &Fixture) -> TransactionsScope {
    TransactionsScope {
        chain_id: f.context.chain_id,
        genesis: f.context.genesis_config_commitment,
        protocol: f.context.protocol_commitment,
        epoch: 1,
        validator_set_hash: [21; 32],
        session: [22; 32],
        sequence: 1,
    }
}

#[test]
fn nuas_application_input_uses_structural_tag15_not_candidate_authority() -> Result<()> {
    let f = fixture()?;
    let message = Message::AssetTransactions {
        scope: input_scope(&f),
        batch: f.batch.clone(),
    };
    let encoded = encode(&message, f.limits)?;
    assert_eq!(encoded[10], 15);
    assert_eq!(&encoded[11..13], &1u16.to_be_bytes());
    assert_eq!(message_lane(&encoded)?, 2);
    assert_eq!(super::super::decode(&encoded, f.limits)?, message);
    assert!(candidate_body_id(&message, f.limits).is_err());
    assert!(candidate_request(&message, &f.policy).is_err());
    // Neither legacy APFL input nor exact NUAS candidate body can interpret
    // application input as that other protocol merely by changing its tag.
    for tag in [13, 14] {
        let mut bad = encoded.clone();
        bad[10] = tag;
        assert!(super::super::decode(&bad, f.limits).is_err());
    }
    for end in [0, 10, 12, encoded.len() - 1] {
        assert!(super::super::decode(&encoded[..end], f.limits).is_err());
    }
    let mut bad = encoded.clone();
    bad[12] = 2;
    assert!(super::super::decode(&bad, f.limits).is_err());
    let mut bad = encoded;
    bad.push(0);
    assert!(super::super::decode(&bad, f.limits).is_err());
    Ok(())
}

#[test]
fn nuas_application_input_limits_charge_original_bytes_not_compressed_size() -> Result<()> {
    let f = fixture()?;
    let message = Message::AssetTransactions {
        scope: input_scope(&f),
        batch: f.batch.clone(),
    };
    let encoded = encode(&message, f.limits)?;
    assert!(
        encoded.len() < f.batch.canonical_bytes(),
        "fixture has shared keys"
    );
    for restricted in [
        DecodeLimits {
            transactions: 3,
            ..f.limits
        },
        DecodeLimits {
            transaction_bytes: f.batch.max_transaction_bytes() - 1,
            ..f.limits
        },
        DecodeLimits {
            body_bytes: f.batch.canonical_bytes() - 1,
            ..f.limits
        },
    ] {
        assert!(encode(&message, restricted).is_err());
        assert!(super::super::decode(&encoded, restricted).is_err());
    }
    let single = Arc::new(AssetSpendBatchV3::from_raw(
        &f.raw[..1],
        batch_limits(f.limits),
    )?);
    let message = Message::AssetTransactions {
        scope: input_scope(&f),
        batch: single.clone(),
    };
    let encoded = encode(&message, f.limits)?;
    let restricted = DecodeLimits {
        transaction_bytes: single.max_transaction_bytes(),
        body_bytes: single.canonical_bytes(),
        message_bytes: encoded.len() - 1,
        ..f.limits
    };
    restricted.validate()?;
    assert!(encode(&message, restricted).is_err());
    assert!(super::super::decode(&encoded, restricted).is_err());
    Ok(())
}

#[test]
fn nuas_application_input_pins_domain_without_granting_spend_authority() -> Result<()> {
    use crate::native_pipeline::consensus::{message_evidence, wire::ValidatorSet};
    let f = fixture()?;
    let keys = (101..105)
        .map(|seed| {
            novovm_consensus::round_bft::test_vectors::FixtureSigningKey::from_bytes(&[seed; 32])
        })
        .collect::<Vec<_>>();
    let validators = Arc::new(ValidatorSet::new_for_test(
        f.context.chain_id,
        1,
        1,
        keys.iter()
            .map(|key| {
                crate::native_pipeline::consensus::wire::Validator::new(
                    key.verifying_key().to_bytes(),
                    1,
                )
            })
            .collect::<Result<_>>()?,
    )?);
    let domain = message_evidence::MessageDomain {
        chain_id: f.context.chain_id,
        genesis: f.context.genesis_config_commitment,
        protocol: f.context.protocol_commitment,
        validators: validators.clone(),
    };
    let scope = TransactionsScope {
        validator_set_hash: validators.hash(),
        ..input_scope(&f)
    };
    let message = Message::AssetTransactions {
        scope,
        batch: f.batch.clone(),
    };
    assert!(matches!(
        message_evidence::verify(&message, &domain)?,
        message_evidence::VerifiedEvidence::None
    ));
    let mut wrong = [scope; 7];
    wrong[0].chain_id += 1;
    wrong[1].genesis[0] ^= 1;
    wrong[2].protocol[0] ^= 1;
    wrong[3].epoch += 1;
    wrong[4].validator_set_hash[0] ^= 1;
    wrong[5].sequence = 0;
    wrong[6].session = [0; 32];
    for scope in wrong {
        assert!(message_evidence::verify(
            &Message::AssetTransactions {
                scope,
                batch: f.batch.clone()
            },
            &domain
        )
        .is_err());
    }
    Ok(())
}
