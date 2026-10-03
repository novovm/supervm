//! Codec-only fixtures: opaque bytes and even well-formed references are not
//! authenticated transactions, execution permits, current parents or votes.

use super::*;
use crate::native_pipeline::consensus::wire::{Phase, Validator, ValidatorSet};
use ed25519_dalek::SigningKey;

fn limits() -> DecodeLimits {
    DecodeLimits {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 4096,
        message_bytes: 8192,
    }
}

fn scope() -> EarlyBodyScope {
    EarlyBodyScope {
        source: wire::Context {
            chain_id: 77,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            epoch: 3,
            validator_set_hash: [4; 32],
            height: 7,
            parent_block_hash: [5; 32],
            parent_decision_hash: [6; 32],
        },
        source_round: 9,
        target_height: 8,
    }
}

fn context() -> BatchContext {
    BatchContext {
        chain_id: 77,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: [3; 32],
        semantic_version: 4,
        effect_contract: [5; 32],
        parent_block_hash: [6; 32],
        parent_height: 7,
        parent_state_root: [8; 32],
        parent_receipt_root: [9; 32],
        parent_state_version: 10,
        receipt_codec: [11; 32],
        height: 8,
        slot: 12,
        timestamp_unix_ms: 13,
    }
}

fn raw() -> Vec<Vec<u8>> {
    vec![vec![1, 2], vec![3, 4, 5]]
}

fn specimens() -> [Message; 2] {
    [
        Message::EarlyBody {
            scope: scope(),
            raw_transactions: raw(),
        },
        Message::BindBody {
            scope: scope(),
            announcement_id: early_body_id(&scope(), &raw(), limits()).unwrap(),
            context: context(),
        },
    ]
}

// Independent fixed-width fixture encoders, not the production append helpers.
fn scope_bytes(scope: EarlyBodyScope) -> Vec<u8> {
    let source = scope.source;
    [
        source.chain_id.to_be_bytes().as_slice(),
        &source.genesis_config_commitment,
        &source.protocol_commitment,
        &source.epoch.to_be_bytes(),
        &source.validator_set_hash,
        &source.height.to_be_bytes(),
        &source.parent_block_hash,
        &source.parent_decision_hash,
        &scope.source_round.to_be_bytes(),
        &scope.target_height.to_be_bytes(),
    ]
    .concat()
}

fn context_bytes(context: BatchContext) -> Vec<u8> {
    [
        context.chain_id.to_be_bytes().as_slice(),
        &context.genesis_config_commitment,
        &context.protocol_commitment,
        &context.business_program,
        &context.semantic_version.to_be_bytes(),
        &context.effect_contract,
        &context.parent_block_hash,
        &context.parent_height.to_be_bytes(),
        &context.parent_state_root,
        &context.parent_receipt_root,
        &context.parent_state_version.to_be_bytes(),
        &context.receipt_codec,
        &context.height.to_be_bytes(),
        &context.slot.to_be_bytes(),
        &context.timestamp_unix_ms.to_be_bytes(),
    ]
    .concat()
}

fn frame_raw(out: &mut Vec<u8>, raw: &[Vec<u8>]) {
    out.extend_from_slice(&(raw.len() as u32).to_be_bytes());
    for bytes in raw {
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(bytes);
    }
}

#[test]
fn versioned_early_and_binding_bytes_roundtrip_with_exact_lane_and_header_sizes() {
    let [early, bind] = specimens();
    let mut expected = b"NVHOSTN1\0\x01\x07\0\x01".to_vec();
    expected.extend_from_slice(&scope_bytes(scope()));
    frame_raw(&mut expected, &raw());
    assert_eq!(scope_bytes(scope()).len(), 200);
    assert_eq!(context_bytes(context()).len(), 308);
    assert_eq!(expected.len(), 230);
    assert_eq!(encode(&early, limits()).unwrap(), expected);
    assert!(body_prefix(&expected[..11]).unwrap());
    let mut bind_expected = b"NVHOSTN1\0\x01\x08\0\x01".to_vec();
    bind_expected.extend_from_slice(&scope_bytes(scope()));
    bind_expected.extend_from_slice(&early_body_id(&scope(), &raw(), limits()).unwrap());
    bind_expected.extend_from_slice(&context_bytes(context()));
    assert_eq!(bind_expected.len(), 553);
    assert_eq!(encode(&bind, limits()).unwrap(), bind_expected);
    assert!(!body_prefix(&bind_expected[..11]).unwrap());
    for (message, expected) in [(early, expected), (bind, bind_expected)] {
        assert_eq!(decode(&expected, limits()).unwrap(), message);
        assert_eq!(
            encode(&decode(&expected, limits()).unwrap(), limits()).unwrap(),
            expected
        );
    }
}

#[test]
fn announcement_identity_binds_complete_scope_order_framing_and_bytes_not_limits() {
    let original = scope();
    let raw = raw();
    let id = early_body_id(&original, &raw, limits()).unwrap();
    let encoded = encode(&specimens()[0], limits()).unwrap();
    let mut digest = Sha256::new();
    digest.update(b"novovm-round-bft-transport/v1/early-body/v1\0");
    digest.update(((encoded.len() - 11) as u64).to_be_bytes());
    digest.update(&encoded[11..]);
    assert_eq!(id, <Hash>::from(digest.finalize()));
    let mutations: &[fn(&mut EarlyBodyScope)] = &[
        |scope| scope.source.chain_id += 1,
        |scope| scope.source.genesis_config_commitment[0] ^= 1,
        |scope| scope.source.protocol_commitment[0] ^= 1,
        |scope| scope.source.epoch += 1,
        |scope| scope.source.validator_set_hash[0] ^= 1,
        |scope| {
            scope.source.height += 1;
            scope.target_height += 1;
        },
        |scope| scope.source.parent_block_hash[0] ^= 1,
        |scope| scope.source.parent_decision_hash[0] ^= 1,
        |scope| scope.source_round += 1,
    ];
    for mutation in mutations {
        let mut changed = original;
        mutation(&mut changed);
        assert_ne!(early_body_id(&changed, &raw, limits()).unwrap(), id);
    }
    for changed in [
        vec![raw[1].clone(), raw[0].clone()],
        vec![vec![1, 2, 3], vec![4, 5]],
        vec![vec![1, 2], vec![3, 4, 6]],
        vec![vec![1, 2, 3, 4, 5]],
    ] {
        assert_ne!(early_body_id(&original, &changed, limits()).unwrap(), id);
    }
    assert_eq!(
        id,
        early_body_id(
            &original,
            &raw,
            DecodeLimits {
                transactions: 2,
                transaction_bytes: 3,
                body_bytes: 5,
                message_bytes: 230,
            }
        )
        .unwrap()
    );
    assert_ne!(id, body_id(&context(), &raw, limits()).unwrap());
}

#[test]
fn scopes_reject_zero_domains_bad_parent_shapes_nonadjacent_and_exhausted_heights() {
    let mutations: &[fn(&mut EarlyBodyScope)] = &[
        |scope| scope.source.chain_id = 0,
        |scope| scope.source.epoch = 0,
        |scope| scope.source.genesis_config_commitment = [0; 32],
        |scope| scope.source.protocol_commitment = [0; 32],
        |scope| scope.source.validator_set_hash = [0; 32],
        |scope| scope.source.parent_block_hash = [0; 32],
        |scope| scope.source.parent_decision_hash = [0; 32],
        |scope| scope.target_height = 0,
        |scope| scope.target_height += 1,
        |scope| {
            scope.source.height = 0;
            scope.target_height = 1;
        },
        |scope| {
            scope.source.height = 1;
            scope.target_height = 2;
        },
        |scope| {
            scope.source.height = u64::MAX;
            scope.target_height = 0;
        },
    ];
    for mutation in mutations {
        let mut invalid = scope();
        mutation(&mut invalid);
        assert!(invalid.validate_shape().is_err());
        assert!(early_body_id(&invalid, &raw(), limits()).is_err());
        assert!(encode(
            &Message::EarlyBody {
                scope: invalid,
                raw_transactions: raw()
            },
            limits()
        )
        .is_err());
        let mut encoded = encode(&specimens()[0], limits()).unwrap();
        encoded[13..213].copy_from_slice(&scope_bytes(invalid));
        assert!(decode(&encoded, limits()).is_err());
    }
    let first = EarlyBodyScope {
        source: wire::Context {
            height: 1,
            parent_block_hash: [0; 32],
            parent_decision_hash: [0; 32],
            ..scope().source
        },
        target_height: 2,
        ..scope()
    };
    assert!(first.validate_shape().is_ok());
}

#[test]
fn binding_checks_announced_domain_and_height_but_does_not_approve_roots_or_program() {
    let mutations: &[fn(&mut BatchContext)] = &[
        |context| context.chain_id += 1,
        |context| context.genesis_config_commitment[0] ^= 1,
        |context| context.protocol_commitment[0] ^= 1,
        |context| context.height += 1,
        |context| context.parent_height += 1,
    ];
    for mutation in mutations {
        let mut wrong = context();
        mutation(&mut wrong);
        assert!(encode(
            &Message::BindBody {
                scope: scope(),
                announcement_id: [8; 32],
                context: wrong
            },
            limits()
        )
        .is_err());
        let mut encoded = encode(&specimens()[1], limits()).unwrap();
        encoded[245..].copy_from_slice(&context_bytes(wrong));
        assert!(decode(&encoded, limits()).is_err());
    }
    assert!(encode(
        &Message::BindBody {
            scope: scope(),
            announcement_id: [0; 32],
            context: context()
        },
        limits()
    )
    .is_err());
    let mut zero_id = encode(&specimens()[1], limits()).unwrap();
    zero_id[213..245].fill(0);
    assert!(decode(&zero_id, limits()).is_err());
    // Lossless untrusted fields: the codec must not fabricate their approval.
    let untrusted = Message::BindBody {
        scope: scope(),
        announcement_id: [8; 32],
        context: BatchContext {
            parent_block_hash: [0; 32],
            parent_state_root: [0; 32],
            parent_receipt_root: [0; 32],
            business_program: [0; 32],
            semantic_version: 0,
            effect_contract: [0; 32],
            receipt_codec: [0; 32],
            ..context()
        },
    };
    assert_eq!(
        decode(&encode(&untrusted, limits()).unwrap(), limits()).unwrap(),
        untrusted
    );
}

#[test]
fn raw_count_lengths_aggregate_and_actual_envelope_are_bounded_before_retention() {
    let message = specimens()[0].clone();
    let exact = DecodeLimits {
        transactions: 2,
        transaction_bytes: 3,
        body_bytes: 5,
        message_bytes: 230,
    };
    let bytes = encode(&message, exact).unwrap();
    assert_eq!(decode(&bytes, exact).unwrap(), message);
    for short in [
        DecodeLimits {
            transactions: 1,
            ..exact
        },
        DecodeLimits {
            transaction_bytes: 2,
            ..exact
        },
        DecodeLimits {
            body_bytes: 4,
            ..exact
        },
        DecodeLimits {
            message_bytes: 229,
            ..exact
        },
    ] {
        assert!(encode(&message, short).is_err());
        assert!(decode(&bytes, short).is_err());
        assert!(early_body_id(&scope(), &raw(), short).is_err());
    }
    for raw in [vec![], vec![vec![]]] {
        assert!(encode(
            &Message::EarlyBody {
                scope: scope(),
                raw_transactions: raw.clone()
            },
            limits()
        )
        .is_err());
        assert!(early_body_id(&scope(), &raw, limits()).is_err());
    }
    for (offset, number) in [
        (213, 0),
        (213, u32::MAX),
        (217, 0),
        (217, u32::MAX),
        (223, u32::MAX),
    ] {
        let mut invalid = bytes.clone();
        invalid[offset..offset + 4].copy_from_slice(&number.to_be_bytes());
        assert!(decode(&invalid, limits()).is_err());
    }
    // EarlyBody has a smaller real header than old Body. It must not inherit
    // the latter's 308-byte context reservation; binding has its own 553 bytes.
    let single = Message::EarlyBody {
        scope: scope(),
        raw_transactions: vec![vec![42]],
    };
    let one = DecodeLimits {
        transactions: 1,
        transaction_bytes: 1,
        body_bytes: 1,
        message_bytes: 222,
    };
    assert_eq!(encode(&single, one).unwrap().len(), 222);
    let bind_limits = DecodeLimits {
        message_bytes: 553,
        ..one
    };
    assert_eq!(encode(&specimens()[1], bind_limits).unwrap().len(), 553);
    assert!(encode(
        &specimens()[1],
        DecodeLimits {
            message_bytes: 552,
            ..bind_limits
        }
    )
    .is_err());
}

#[test]
fn both_new_payloads_reject_every_truncation_trailing_data_and_unknown_versions() {
    for message in specimens() {
        let bytes = encode(&message, limits()).unwrap();
        for end in 0..bytes.len() {
            assert!(
                decode(&bytes[..end], limits()).is_err(),
                "accepted truncation at {end}"
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode(&trailing, limits()).is_err());
        for version in [0u16, 2, u16::MAX] {
            let mut changed = bytes.clone();
            changed[11..13].copy_from_slice(&version.to_be_bytes());
            assert!(decode(&changed, limits()).is_err());
        }
        let mut wrong_outer = bytes;
        wrong_outer[9] = 2;
        assert!(body_prefix(&wrong_outer).is_err());
        assert!(decode(&wrong_outer, limits()).is_err());
    }
}

fn legacy_specimens() -> Vec<Message> {
    let key = SigningKey::from_bytes(&[7; 32]);
    let set = ValidatorSet::new(
        77,
        3,
        1,
        vec![Validator::new(key.verifying_key().to_bytes(), 1).unwrap()],
    )
    .unwrap();
    let source = wire::Context {
        validator_set_hash: set.hash(),
        ..scope().source
    };
    let prevote = novovm_consensus::round_bft::test_vectors::sign_vote(
        source,
        0,
        Phase::Prevote,
        Some([31; 32]),
        &set,
        &key,
    )
    .unwrap();
    let precommit = novovm_consensus::round_bft::test_vectors::sign_vote(
        source,
        0,
        Phase::Precommit,
        Some([31; 32]),
        &set,
        &key,
    )
    .unwrap();
    let proposal = novovm_consensus::round_bft::test_vectors::sign_proposal(
        source, 0, [31; 32], None, &set, &key,
    )
    .unwrap();
    vec![
        Message::Body {
            context: context(),
            raw_transactions: raw(),
        },
        Message::Proposal {
            proposal: proposal.clone(),
            valid_quorum: None,
            body_id: [45; 32],
        },
        Message::Proposal {
            proposal: novovm_consensus::round_bft::test_vectors::sign_proposal(
                source,
                1,
                [31; 32],
                Some(0),
                &set,
                &key,
            )
            .unwrap(),
            valid_quorum: Some(Quorum::from_votes(&set, vec![prevote.clone()]).unwrap()),
            body_id: [45; 32],
        },
        Message::Vote(prevote),
        Message::Decision {
            proposal,
            certificate: Quorum::from_votes(&set, vec![precommit]).unwrap(),
            body_id: [45; 32],
        },
        Message::RequestBody { body_id: [45; 32] },
        Message::RequestDecision { context: source },
    ]
}

// Original outer encoding from 1fb606f, isolated from the new encode dispatch.
fn legacy_encode(message: &Message) -> Vec<u8> {
    let mut out = b"NVHOSTN1\0\x01".to_vec();
    let blob = |out: &mut Vec<u8>, bytes: Vec<u8>| {
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&bytes);
    };
    match message {
        Message::Body {
            context,
            raw_transactions,
        } => {
            out.push(1);
            out.extend_from_slice(&context_bytes(*context));
            frame_raw(&mut out, raw_transactions);
        }
        Message::Proposal {
            proposal,
            valid_quorum,
            body_id,
        } => {
            out.push(2);
            blob(&mut out, wire::encode_proposal(proposal).unwrap());
            out.push(u8::from(valid_quorum.is_some()));
            if let Some(quorum) = valid_quorum {
                blob(&mut out, wire::encode_quorum(quorum).unwrap());
            }
            out.extend_from_slice(body_id);
        }
        Message::Vote(vote) => {
            out.push(3);
            blob(&mut out, wire::encode_vote(vote).unwrap());
        }
        Message::Decision {
            proposal,
            certificate,
            body_id,
        } => {
            out.push(4);
            blob(&mut out, wire::encode_proposal(proposal).unwrap());
            blob(&mut out, wire::encode_quorum(certificate).unwrap());
            out.extend_from_slice(body_id);
        }
        Message::RequestBody { body_id } => {
            out.push(5);
            out.extend_from_slice(body_id);
        }
        Message::RequestDecision { context } => {
            out.push(6);
            out.extend_from_slice(
                &scope_bytes(EarlyBodyScope {
                    source: *context,
                    ..scope()
                })[..184],
            );
        }
        Message::EarlyBody { .. }
        | Message::BindBody { .. }
        | Message::Transactions { .. }
        | Message::ApflBody { .. }
        | Message::ApflEarlyBody { .. }
        | Message::ApflTransactions { .. }
        | Message::TransactionsTaken { .. } => {
            panic!("new tag sent to old codec oracle")
        }
    }
    out
}

#[test]
fn original_six_tags_and_full_body_hash_keep_their_exact_original_bytes() {
    for message in legacy_specimens() {
        let expected = legacy_encode(&message);
        assert_eq!(encode(&message, limits()).unwrap(), expected);
        assert_eq!(decode(&expected, limits()).unwrap(), message);
        assert_eq!(body_prefix(&expected).unwrap(), expected[10] == 1);
    }
    let original = legacy_encode(&Message::Body {
        context: context(),
        raw_transactions: raw(),
    });
    let mut digest = Sha256::new();
    digest.update(b"novovm-round-bft-transport/v1/body\0");
    digest.update(((original.len() - 11) as u64).to_be_bytes());
    digest.update(&original[11..]);
    assert_eq!(
        body_id(&context(), &raw(), limits()).unwrap(),
        <Hash>::from(digest.finalize())
    );
}
