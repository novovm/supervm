//! Local codec/identity tests only; synthetic records are not executed blocks.
use super::*;
use crate::consensus::wire::Validator;
use ed25519_dalek::SigningKey;

fn record() -> ChainRecord {
    let member = Validator::new(
        SigningKey::from_bytes(&[1; 32]).verifying_key().to_bytes(),
        1,
    )
    .unwrap();
    let validator = member.id();
    let set = ValidatorSet::new(1, 1, 1, vec![member]).unwrap();
    let context = ConsensusContext {
        chain_id: 1,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: [3; 32],
        receipt_batch_commitment: [4; 32],
        state_version: 0,
        decision_hash: [0; 32],
    };
    ChainRecord {
        context,
        parent,
        point: ParentPoint {
            height: 1,
            block_hash: [5; 32],
            state_root: [6; 32],
            receipt_batch_commitment: [7; 32],
            state_version: 2,
            decision_hash: decision_id(&context, [5; 32]),
        },
        candidate_id: [8; 32],
        document_digest: [9; 32],
        outbox_validator: validator,
        outbox_revision: 4,
        outbox_digest: [10; 32],
    }
}

#[test]
fn archive_encoding_is_bounded_canonical_and_all_bytes_bound() {
    let record = record();
    let bytes = record.encode().unwrap();
    assert_eq!(ChainRecord::decode(&bytes).unwrap(), record);
    for n in 0..bytes.len() {
        assert!(ChainRecord::decode(&bytes[..n]).is_err());
    }
    for n in 0..bytes.len() {
        let mut bad = bytes.clone();
        bad[n] ^= 1;
        assert!(ChainRecord::decode(&bad).is_err());
    }
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(ChainRecord::decode(&bad).is_err());
    assert!(ChainRecord::decode(&vec![0; MAX_RECORD_BYTES + 1]).is_err());
    let head = Head::decode(&record.head_bytes().unwrap()).unwrap();
    head.verify_record(&record).unwrap();
    let mut other = record.clone();
    other.outbox_revision += 1;
    assert!(head.verify_record(&other).is_err());
}

#[test]
fn decision_identity_binds_value_and_all_context_but_not_local_proof_locator() {
    let original = record();
    let mut other = original.clone();
    other.outbox_validator = [90; 32];
    other.outbox_revision = 77;
    other.outbox_digest = [91; 32];
    assert_eq!(original.point(), other.point());
    assert_ne!(original.head_bytes().unwrap(), other.head_bytes().unwrap());
    let expected = decision_id(&original.context, original.point.block_hash);
    let mutate: [fn(&mut ConsensusContext); 8] = [
        |c| c.chain_id += 1,
        |c| c.genesis_config_commitment[0] ^= 1,
        |c| c.protocol_commitment[0] ^= 1,
        |c| c.epoch += 1,
        |c| c.validator_set_hash[0] ^= 1,
        |c| c.height += 1,
        |c| c.parent_block_hash[0] ^= 1,
        |c| c.parent_decision_hash[0] ^= 1,
    ];
    for f in mutate {
        let mut c = original.context;
        f(&mut c);
        assert_ne!(decision_id(&c, original.point.block_hash), expected);
    }
    assert_ne!(decision_id(&original.context, [99; 32]), expected);
}

#[test]
fn malformed_parent_height_version_and_decision_are_not_repaired() {
    let original = record();
    let mutate: [fn(&mut ChainRecord); 6] = [
        |r| r.parent.height = 1,
        |r| r.point.height = 2,
        |r| r.point.state_version = 0,
        |r| r.point.decision_hash = [90; 32],
        |r| r.outbox_revision = 0,
        |r| r.context.parent_block_hash = [90; 32],
    ];
    for f in mutate {
        let mut bad = original.clone();
        f(&mut bad);
        assert!(bad.encode().is_err());
        assert!(ChainRecord::decode(&encode_local(RECORD_MAGIC, &bad).unwrap()).is_err());
    }
}
