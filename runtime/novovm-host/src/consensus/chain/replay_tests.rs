//! Pure read-side rejection tests. Synthetic records/signatures are NOT
//! executed blocks, AOEM integration, or proof that replay grants authority.
use super::*;
use crate::consensus::wire::{self, Validator, Vote};
use ed25519_dalek::SigningKey;

fn fixture() -> (ChainRecord, Arc<ValidatorSet>, SigningKey) {
    let key = SigningKey::from_bytes(&[11; 32]);
    let member = Validator::new(key.verifying_key().to_bytes(), 1).unwrap();
    let validator = member.id();
    let set = Arc::new(ValidatorSet::new(9, 3, 1, vec![member]).unwrap());
    let context = ConsensusContext {
        chain_id: 9,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        epoch: 3,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let record = ChainRecord {
        parent: ParentPoint {
            height: 0,
            block_hash: [0; 32],
            state_root: [3; 32],
            receipt_batch_commitment: [4; 32],
            state_version: 0,
            decision_hash: [0; 32],
        },
        context,
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
    };
    record.validate_shape().unwrap();
    (record, set, key)
}

fn signed_evidence(
    record: &ChainRecord,
    set: &ValidatorSet,
    key: &SigningKey,
    value: Hash,
    phase: Phase,
) -> (Proposal, Quorum) {
    let proposal = Proposal::sign(record.context, 0, value, None, set, key).unwrap();
    let vote = Vote::sign(record.context, 0, phase, Some(value), set, key).unwrap();
    let quorum = Quorum::from_votes(set, vec![vote]).unwrap();
    (proposal, quorum)
}

// Test-only fixture for the existing immutable journal envelope. It deliberately
// cannot produce a signer snapshot or a current-session DurableCandidate.
fn outbox(revision: u64, proposal: &Proposal, quorum: &Quorum) -> Vec<u8> {
    let mut bytes = b"NVOUT001".to_vec();
    bytes.extend_from_slice(&revision.to_be_bytes());
    bytes.extend_from_slice(&[0x91; 32]);
    bytes.push(3);
    for body in [
        wire::encode_proposal(proposal).unwrap(),
        wire::encode_quorum(quorum).unwrap(),
    ] {
        bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&body);
    }
    let mut digest = Sha256::new();
    digest.update(b"novovm/replacement/signing-journal/v1\0");
    digest.update(&bytes);
    bytes.extend_from_slice(&digest.finalize());
    bytes
}

#[test]
fn request_rejects_zero_future_height_missing_head_and_wrong_set() {
    let (record, set, _) = fixture();
    assert!(ArchiveRead::new(0, record.point, record.context, set.clone()).is_err());
    assert!(ArchiveRead::new(2, record.point, record.context, set.clone()).is_err());
    assert!(ArchiveRead::new(1, record.parent, record.context, set.clone()).is_err());
    for mutate in [
        |p: &mut ParentPoint| p.block_hash = [0; 32],
        |p: &mut ParentPoint| p.state_root = [0; 32],
        |p: &mut ParentPoint| p.receipt_batch_commitment = [0; 32],
        |p: &mut ParentPoint| p.decision_hash = [0; 32],
        |p: &mut ParentPoint| p.state_version = 0,
    ] {
        let mut ceiling = record.point;
        mutate(&mut ceiling);
        assert!(ArchiveRead::new(1, ceiling, record.context, set.clone()).is_err());
    }
    let mut context = record.context;
    context.epoch += 1;
    assert!(ArchiveRead::new(1, record.point, context, set).is_err());
}

#[test]
fn head_check_rejects_rollback_and_replacement_but_allows_forward_progress() {
    let (record, set, _) = fixture();
    let read = ArchiveRead::new(1, record.point, record.context, set).unwrap();
    let head = Head::decode(&record.head_bytes().unwrap()).unwrap();
    read.check_head(&head).unwrap();
    let mut bad = head.clone();
    bad.height = 0;
    assert!(read.check_head(&bad).is_err());
    bad = head.clone();
    bad.block_hash = [99; 32];
    assert!(read.check_head(&bad).is_err());
    // Normal advancement cannot invalidate an immutable already-verified prefix.
    bad.height = 2;
    read.check_head(&bad).unwrap();
}

#[test]
fn historical_read_is_pinned_to_requested_height_and_fixed_domain() {
    let (record, set, _) = fixture();
    let mut ceiling = record.point;
    ceiling.height = 2;
    ceiling.block_hash = [40; 32];
    let read = ArchiveRead::new(1, ceiling, record.context, set).unwrap();
    let head = Head {
        height: 2,
        block_hash: ceiling.block_hash,
        record_digest: [41; 32],
    };
    read.check_record(&record, &head).unwrap();
    for mutate in [
        |r: &mut ChainRecord| r.context.genesis_config_commitment = [42; 32],
        |r: &mut ChainRecord| r.context.protocol_commitment = [43; 32],
        |r: &mut ChainRecord| r.context.epoch += 1,
        |r: &mut ChainRecord| r.context.validator_set_hash = [44; 32],
        |r: &mut ChainRecord| r.context.chain_id += 1,
    ] {
        let mut bad = record.clone();
        mutate(&mut bad);
        bad.point.decision_hash = decision_id(&bad.context, bad.point.block_hash);
        assert!(read.check_record(&bad, &head).is_err());
    }
    let mut wrong_height = record.clone();
    wrong_height.point.height = 2;
    assert!(read.check_record(&wrong_height, &head).is_err());
}

#[test]
fn head_record_must_bind_full_ceiling_and_exact_archive_digest() {
    let (record, set, _) = fixture();
    let read = ArchiveRead::new(1, record.point, record.context, set.clone()).unwrap();
    let head = Head::decode(&record.head_bytes().unwrap()).unwrap();
    read.check_record(&record, &head).unwrap();
    let mut locator_changed = record.clone();
    locator_changed.outbox_revision += 1;
    assert!(read.check_record(&locator_changed, &head).is_err());
    let mut ceiling = record.point;
    ceiling.state_root = [50; 32];
    let read = ArchiveRead::new(1, ceiling, record.context, set).unwrap();
    assert!(read.check_record(&record, &head).is_err());
}

#[test]
fn replay_outbox_preserves_exact_verified_proposal_and_quorum() {
    let (mut record, set, key) = fixture();
    let (proposal, quorum) = signed_evidence(
        &record,
        &set,
        &key,
        record.point.block_hash,
        Phase::Precommit,
    );
    let bytes = outbox(record.outbox_revision, &proposal, &quorum);
    record.outbox_digest = archived_outbox_digest(&bytes);
    let (read_proposal, read_quorum) = record.checked_outbox(&bytes, &set).unwrap();
    assert_eq!(read_proposal, proposal);
    assert_eq!(read_quorum, quorum);
    let mut wrong_digest = record.clone();
    wrong_digest.outbox_digest = [60; 32];
    assert!(wrong_digest.checked_outbox(&bytes, &set).is_err());
    let mut wrong_locator = record.clone();
    wrong_locator.outbox_revision += 1;
    assert!(wrong_locator.checked_outbox(&bytes, &set).is_err());
    record.outbox_validator = [61; 32];
    assert!(record.checked_outbox(&bytes, &set).is_err());
}

#[test]
fn rehashed_outbox_cannot_authorize_bad_signature_wrong_value_or_prevote() {
    let (mut record, set, key) = fixture();
    for (value, phase, damage_signature) in [
        (record.point.block_hash, Phase::Precommit, true),
        ([70; 32], Phase::Precommit, false),
        (record.point.block_hash, Phase::Prevote, false),
    ] {
        let (mut proposal, quorum) = signed_evidence(&record, &set, &key, value, phase);
        if damage_signature {
            proposal.signature[0] ^= 1;
        }
        let bytes = outbox(record.outbox_revision, &proposal, &quorum);
        record.outbox_digest = archived_outbox_digest(&bytes);
        assert!(record.checked_outbox(&bytes, &set).is_err());
    }
}
