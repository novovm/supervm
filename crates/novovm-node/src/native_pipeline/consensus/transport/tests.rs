//! Codec-only checks. Synthetic body bytes are NOT authenticated transactions,
//! executed blocks or proof of mainchain network/finality integration.
use super::*;
use crate::native_pipeline::consensus::wire::{
    Context as ConsensusContext, Phase, Validator, ValidatorSet,
};
use ed25519_dalek::SigningKey;
use novovm_network::duplex::fragments::{FragmentAdmission, Reassembler, ReassemblyLimits};
use std::time::{Duration, Instant};

fn limits() -> DecodeLimits {
    DecodeLimits {
        transactions: 1024,
        transaction_bytes: 512 * 1024,
        body_bytes: 2 * 1024 * 1024,
        message_bytes: 3 * 1024 * 1024,
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

fn specimens() -> Vec<Message> {
    let key = SigningKey::from_bytes(&[7; 32]);
    let set = ValidatorSet::new(
        77,
        1,
        1,
        vec![Validator::new(key.verifying_key().to_bytes(), 1).unwrap()],
    )
    .unwrap();
    let context = ConsensusContext {
        chain_id: 77,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let value = [31; 32];
    let prevote = novovm_consensus::round_bft::test_vectors::sign_vote(
        context,
        0,
        Phase::Prevote,
        Some(value),
        &set,
        &key,
    )
    .unwrap();
    let precommit = novovm_consensus::round_bft::test_vectors::sign_vote(
        context,
        0,
        Phase::Precommit,
        Some(value),
        &set,
        &key,
    )
    .unwrap();
    let proposal = novovm_consensus::round_bft::test_vectors::sign_proposal(
        context, 0, value, None, &set, &key,
    )
    .unwrap();
    let later = novovm_consensus::round_bft::test_vectors::sign_proposal(
        context,
        1,
        value,
        Some(0),
        &set,
        &key,
    )
    .unwrap();
    let body_id = [45; 32];
    vec![
        Message::Body {
            context: super::tests::context(),
            raw_transactions: vec![vec![1, 2], vec![3, 4, 5]],
        },
        Message::Proposal {
            proposal: proposal.clone(),
            valid_quorum: None,
            body_id,
        },
        Message::Proposal {
            proposal: later,
            valid_quorum: Some(Quorum::from_votes(&set, vec![prevote.clone()]).unwrap()),
            body_id,
        },
        Message::Vote(prevote),
        Message::Decision {
            proposal,
            certificate: Quorum::from_votes(&set, vec![precommit]).unwrap(),
            body_id,
        },
        Message::RequestBody { body_id },
        Message::RequestDecision { context },
    ]
}

#[test]
fn decision_request_binds_exact_parent_and_rejects_unbounded_or_ambiguous_input() {
    let Message::RequestDecision { context } = specimens().pop().unwrap() else {
        panic!("missing request specimen");
    };
    let message = Message::RequestDecision { context };
    let bytes = encode(&message, limits()).unwrap();
    assert_eq!(bytes.len(), PREFIX_BYTES + 3 * 8 + 5 * 32);
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end], limits()).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode(&trailing, limits()).is_err());
    assert!(encode(
        &Message::RequestDecision {
            context: ConsensusContext {
                height: 0,
                ..context
            }
        },
        limits()
    )
    .is_err());
    let next = ConsensusContext {
        height: 2,
        parent_block_hash: [8; 32],
        parent_decision_hash: [9; 32],
        ..context
    };
    for changed in [
        next,
        ConsensusContext {
            parent_decision_hash: [10; 32],
            ..next
        },
    ] {
        let encoded = encode(&Message::RequestDecision { context: changed }, limits()).unwrap();
        assert_ne!(encoded, bytes);
        assert_eq!(
            decode(&encoded, limits()).unwrap(),
            Message::RequestDecision { context: changed }
        );
    }
    assert!(decode(
        &bytes,
        DecodeLimits {
            message_bytes: bytes.len() - 1,
            body_bytes: 1,
            transaction_bytes: 1,
            transactions: 1
        }
    )
    .is_err());
}

#[test]
fn all_development_message_variants_roundtrip_without_execution_authority() {
    for message in specimens() {
        let bytes = encode(&message, limits()).unwrap();
        assert_eq!(decode(&bytes, limits()).unwrap(), message);
        assert_eq!(
            encode(&decode(&bytes, limits()).unwrap(), limits()).unwrap(),
            bytes
        );
    }
    let context = context();
    let body = Message::Body {
        context,
        raw_transactions: vec![vec![1, 2], vec![3]],
    };
    assert_eq!(
        encode(&body, limits()).unwrap().len(),
        PREFIX_BYTES + CONTEXT_BYTES + 4 + 8 + 3
    );
    // Malformed claimed business context still transports losslessly: only the
    // local compiler/parent authority can approve it, not this byte parser.
    let body = Message::Body {
        context: BatchContext {
            semantic_version: 0,
            chain_id: 0,
            ..context
        },
        raw_transactions: vec![vec![9]],
    };
    assert_eq!(
        decode(&encode(&body, limits()).unwrap(), limits()).unwrap(),
        body
    );
}

#[test]
fn body_identity_binds_every_context_field_original_bytes_and_order_not_budgets() {
    let original = context();
    let raw = vec![vec![1, 2], vec![3, 4]];
    let expected = body_id(&original, &raw, limits()).unwrap();
    let variants = [
        BatchContext {
            chain_id: 78,
            ..original
        },
        BatchContext {
            genesis_config_commitment: [2; 32],
            ..original
        },
        BatchContext {
            protocol_commitment: [3; 32],
            ..original
        },
        BatchContext {
            business_program: [4; 32],
            ..original
        },
        BatchContext {
            semantic_version: 5,
            ..original
        },
        BatchContext {
            effect_contract: [6; 32],
            ..original
        },
        BatchContext {
            parent_block_hash: [7; 32],
            ..original
        },
        BatchContext {
            parent_height: 8,
            ..original
        },
        BatchContext {
            parent_state_root: [9; 32],
            ..original
        },
        BatchContext {
            parent_receipt_root: [10; 32],
            ..original
        },
        BatchContext {
            parent_state_version: 11,
            ..original
        },
        BatchContext {
            receipt_codec: [12; 32],
            ..original
        },
        BatchContext {
            height: 9,
            ..original
        },
        BatchContext {
            slot: 13,
            ..original
        },
        BatchContext {
            timestamp_unix_ms: 14,
            ..original
        },
    ];
    for changed in variants {
        assert_ne!(body_id(&changed, &raw, limits()).unwrap(), expected);
    }
    assert_ne!(
        body_id(&original, &[raw[1].clone(), raw[0].clone()], limits()).unwrap(),
        expected
    );
    assert_ne!(
        body_id(&original, &[vec![1, 2, 3], vec![4]], limits()).unwrap(),
        expected
    );
    assert_eq!(
        body_id(
            &original,
            &raw,
            DecodeLimits {
                transactions: 2048,
                ..limits()
            }
        )
        .unwrap(),
        expected
    );
}

#[test]
fn bounded_preflight_rejects_huge_count_lengths_empty_total_and_trailing_input() {
    let body = Message::Body {
        context: context(),
        raw_transactions: vec![vec![1, 2], vec![3, 4]],
    };
    let original = encode(&body, limits()).unwrap();
    let count_offset = PREFIX_BYTES + CONTEXT_BYTES;
    for count in [0u32, u32::MAX] {
        let mut bad = original.clone();
        bad[count_offset..count_offset + 4].copy_from_slice(&count.to_be_bytes());
        assert!(decode(&bad, limits()).is_err());
    }
    for length in [0u32, u32::MAX] {
        let mut bad = original.clone();
        bad[count_offset + 4..count_offset + 8].copy_from_slice(&length.to_be_bytes());
        assert!(decode(&bad, limits()).is_err());
    }
    for limited in [
        DecodeLimits {
            transactions: 1,
            ..limits()
        },
        DecodeLimits {
            transaction_bytes: 1,
            ..limits()
        },
        DecodeLimits {
            body_bytes: 3,
            transaction_bytes: 3,
            ..limits()
        },
        DecodeLimits {
            message_bytes: 64,
            body_bytes: 32,
            transaction_bytes: 32,
            ..limits()
        },
    ] {
        assert!(decode(&original, limited).is_err());
        assert!(encode(&body, limited).is_err());
    }
    for message in specimens() {
        let bytes = encode(&message, limits()).unwrap();
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end], limits()).is_err());
        }
        let mut tail = bytes;
        tail.push(0);
        assert!(decode(&tail, limits()).is_err());
    }
}

#[test]
fn rejects_unknown_version_kind_optional_tag_and_oversized_signed_blob() {
    let original = encode(&specimens()[1], limits()).unwrap();
    for (offset, value) in [(0, b'X'), (9, 2), (10, 255)] {
        let mut bad = original.clone();
        bad[offset] = value;
        assert!(decode(&bad, limits()).is_err());
    }
    let proposal_len = u32::from_be_bytes(original[11..15].try_into().unwrap()) as usize;
    let mut bad = original.clone();
    bad[15 + proposal_len] = 2;
    assert!(decode(&bad, limits()).is_err());
    let mut bad = original;
    bad[11..15].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode(&bad, limits()).is_err());
}

#[test]
fn prepared_large_body_fragments_reassemble_exact_bytes_under_local_domain() {
    // Carrier/codec composition only, not a second chain or a pipeline test.
    let context = context();
    let message = Message::Body {
        context,
        raw_transactions: vec![vec![0x51; 300_000], vec![0x52; 300_000]],
    };
    let domain = fragment_domain(
        context.chain_id,
        context.genesis_config_commitment,
        context.protocol_commitment,
    );
    let prepared = prepare_message(domain, &message, limits()).unwrap();
    assert!(prepared.frame_count() > 1);
    let mut receiver = Reassembler::new(
        domain,
        vec!["test-peer".into()],
        ReassemblyLimits {
            max_message_bytes: limits().message_bytes,
            messages: 2,
            bytes: 4 * 1024 * 1024,
            peer_messages: 2,
            peer_bytes: 4 * 1024 * 1024,
            ttl: Duration::from_secs(2),
        },
    )
    .unwrap();
    let now = Instant::now();
    for index in (0..prepared.frame_count()).rev() {
        assert_eq!(
            receiver
                .push("test-peer", &prepared.frame(index).unwrap(), now)
                .unwrap(),
            FragmentAdmission::Accepted
        );
    }
    let completed = loop {
        if let Some(message) = receiver.poll_complete(now, 1).unwrap() {
            break message;
        }
    };
    let bytes = completed.chunks.concat(); // ingress-owner work in this fixture
    assert_eq!(decode(&bytes, limits()).unwrap(), message);
    assert_eq!(receiver.reserved_bytes(), 0);
    assert_ne!(
        fragment_domain(
            78,
            context.genesis_config_commitment,
            context.protocol_commitment
        ),
        domain
    );
    assert_ne!(
        fragment_domain(77, [3; 32], context.protocol_commitment),
        domain
    );
    assert_ne!(
        fragment_domain(77, context.genesis_config_commitment, [3; 32]),
        domain
    );
}
