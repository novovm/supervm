//! Structural transport tests with dummy signature bytes. These do NOT claim
//! ML-DSA verification, spending authorization, execution or network finality.
use super::*;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, SignedUcaSpendV1, UcaSpendIntentV1, MAX_SIGNED_SPEND_BYTES,
    MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
};

struct Fixture {
    context: BatchContext,
    policy: DirectNovFeePolicy,
    raw: Vec<Vec<u8>>,
    hints: Vec<(Vec<u8>, Vec<u8>)>,
    batch: Arc<UcaSpendBatch>,
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
            SignedUcaSpendV1 {
                intent: UcaSpendIntentV1 {
                    chain_id: context.chain_id,
                    genesis: context.genesis_config_commitment,
                    account_id: "uca:alice".into(),
                    authorization_epoch: 1,
                    spend_key_ref: spend_key_ref_v1(&public_key)?,
                    recipient_uca: "uca:bob".into(),
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
    let hints = vec![(vec![5; 20], vec![6; 32]); raw.len()];
    let batch = Arc::new(UcaSpendBatch::from_raw(&raw, &hints, batch_limits(limits))?);
    Ok(Fixture {
        context,
        policy,
        raw,
        hints,
        batch,
        limits,
    })
}

#[test]
fn nuas_body_uses_explicit_tag_and_existing_bounded_body_lane() -> Result<()> {
    let f = fixture()?;
    let message = Message::UcaBody {
        context: f.context,
        batch: f.batch.clone(),
    };
    let encoded = encode(&message, f.limits)?;
    assert_eq!(encoded[10], 14);
    assert_eq!(&encoded[11..13], &1u16.to_be_bytes());
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
fn nuas_body_rejects_versions_truncation_trailing_and_each_resource_limit() -> Result<()> {
    let f = fixture()?;
    let message = Message::UcaBody {
        context: f.context,
        batch: f.batch.clone(),
    };
    let encoded = encode(&message, f.limits)?;
    for end in [0, 10, 12, 13 + CONTEXT_BYTES, encoded.len() - 1] {
        assert!(super::super::decode(&encoded[..end], f.limits).is_err());
    }
    let mut bad = encoded.clone();
    bad[12] = 2;
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
    let single = Arc::new(UcaSpendBatch::from_raw(
        &f.raw[..1],
        &f.hints[..1],
        batch_limits(f.limits),
    )?);
    let message = Message::UcaBody {
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
        &Message::UcaBody {
            context: f.context,
            batch: f.batch.clone(),
        },
        f.limits,
    )?;
    // Context starts after transport prefix + NUAS payload version. Mutations
    // here test the receiving decoder, not just a local encoder's rejection.
    for (context, context_offset) in contexts.into_iter().zip([72usize, 252, 104]) {
        let message = Message::UcaBody {
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
        &Message::UcaBody {
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
fn nuas_body_identity_commits_order_signatures_hints_and_parent() -> Result<()> {
    let f = fixture()?;
    let original = uca_body_id(&f.context, &f.batch, f.limits)?;
    let mut raw = f.raw.clone();
    raw.swap(0, 1);
    let reordered = UcaSpendBatch::from_raw(&raw, &f.hints, batch_limits(f.limits))?;
    assert_ne!(original, uca_body_id(&f.context, &reordered, f.limits)?);
    let mut raw = f.raw.clone();
    *raw[0].last_mut().unwrap() ^= 1;
    let modified_signature = UcaSpendBatch::from_raw(&raw, &f.hints, batch_limits(f.limits))?;
    assert_ne!(
        original,
        uca_body_id(&f.context, &modified_signature, f.limits)?
    );
    for hint_column in 0..2 {
        let mut hints = f.hints.clone();
        if hint_column == 0 {
            hints[0].0[0] ^= 1;
        } else {
            hints[0].1[0] ^= 1;
        }
        let modified_hint = UcaSpendBatch::from_raw(&f.raw, &hints, batch_limits(f.limits))?;
        assert_ne!(original, uca_body_id(&f.context, &modified_hint, f.limits)?);
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
    let message = Message::UcaTransactions {
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
    let message = Message::UcaTransactions {
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
    let single = Arc::new(UcaSpendBatch::from_raw(
        &f.raw[..1],
        &f.hints[..1],
        batch_limits(f.limits),
    )?);
    let message = Message::UcaTransactions {
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
    let message = Message::UcaTransactions {
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
            &Message::UcaTransactions {
                scope,
                batch: f.batch.clone()
            },
            &domain
        )
        .is_err());
    }
    Ok(())
}
