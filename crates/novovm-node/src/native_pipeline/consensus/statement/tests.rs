use super::*;
use std::collections::BTreeSet;

// Private hash-codec fixtures only: these statements have NOT been constructed
// from an executed packet, persisted, signed, or authorized as a chain head.
// Mutations intentionally need not satisfy the public constructor's invariants;
// they isolate whether each independently encoded field affects the hash.
fn statement() -> BlockStatement {
    let execution = BatchContext {
        chain_id: 0x0102_0304_0506_0708,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: [3; 32],
        semantic_version: 0x1112_1314,
        effect_contract: [4; 32],
        parent_block_hash: [5; 32],
        parent_height: 6,
        parent_state_root: [7; 32],
        parent_receipt_root: [8; 32],
        parent_state_version: 9,
        receipt_codec: [10; 32],
        height: 7,
        slot: 12,
        timestamp_unix_ms: 13,
    };
    BlockStatement {
        consensus: ConsensusContext {
            chain_id: execution.chain_id,
            genesis_config_commitment: execution.genesis_config_commitment,
            protocol_commitment: execution.protocol_commitment,
            epoch: 14,
            validator_set_hash: [15; 32],
            height: execution.height,
            parent_block_hash: execution.parent_block_hash,
            parent_decision_hash: [16; 32],
        },
        execution,
        candidate_id: [17; 32],
        state_root: [18; 32],
        receipt_batch_commitment: [19; 32],
        execution_statement: [20; 32],
        document_digest: [21; 32],
        transaction_count: 2,
        state_version: 11,
        hash: [0; 32],
    }
}

type StatementMutation = (&'static str, fn(&mut BlockStatement));

fn assert_hash_sensitive(changes: &[StatementMutation]) {
    let original = statement();
    let baseline = original.compute_hash();
    let mut hashes = BTreeSet::from([baseline]);
    assert_eq!(original.compute_hash(), baseline);
    for (name, mutate) in changes {
        let mut changed = original.clone();
        mutate(&mut changed);
        let actual = changed.compute_hash();
        assert_ne!(actual, baseline, "omitted statement field: {name}");
        assert!(hashes.insert(actual), "ambiguous field encoding: {name}");
    }
}

#[test]
fn hash_binds_every_batch_context_field() {
    let changes: [StatementMutation; 15] = [
        ("chain_id", |s| s.execution.chain_id ^= 1),
        ("genesis_config_commitment", |s| {
            s.execution.genesis_config_commitment[0] ^= 1
        }),
        ("protocol_commitment", |s| {
            s.execution.protocol_commitment[0] ^= 1
        }),
        ("business_program", |s| s.execution.business_program[0] ^= 1),
        ("semantic_version", |s| s.execution.semantic_version ^= 1),
        ("effect_contract", |s| s.execution.effect_contract[0] ^= 1),
        ("parent_block_hash", |s| {
            s.execution.parent_block_hash[0] ^= 1
        }),
        ("parent_height", |s| s.execution.parent_height ^= 1),
        ("parent_state_root", |s| {
            s.execution.parent_state_root[0] ^= 1
        }),
        ("parent_receipt_root", |s| {
            s.execution.parent_receipt_root[0] ^= 1
        }),
        ("parent_state_version", |s| {
            s.execution.parent_state_version ^= 1
        }),
        ("receipt_codec", |s| s.execution.receipt_codec[0] ^= 1),
        ("height", |s| s.execution.height ^= 1),
        ("slot", |s| s.execution.slot ^= 1),
        ("timestamp_unix_ms", |s| s.execution.timestamp_unix_ms ^= 1),
    ];
    assert_hash_sensitive(&changes);
}

#[test]
fn hash_binds_extra_consensus_domain_and_every_execution_output_field() {
    // The other consensus fields mirror BatchContext and are checked for exact
    // equality by from_executed. These three are additional consensus inputs.
    let changes: [StatementMutation; 10] = [
        ("epoch", |s| s.consensus.epoch ^= 1),
        ("validator_set_hash", |s| {
            s.consensus.validator_set_hash[0] ^= 1
        }),
        ("parent_decision_hash", |s| {
            s.consensus.parent_decision_hash[0] ^= 1
        }),
        ("candidate_id", |s| s.candidate_id[0] ^= 1),
        ("state_root", |s| s.state_root[0] ^= 1),
        ("receipt_batch_commitment", |s| {
            s.receipt_batch_commitment[0] ^= 1
        }),
        ("execution_statement", |s| s.execution_statement[0] ^= 1),
        ("document_digest", |s| s.document_digest[0] ^= 1),
        ("transaction_count", |s| s.transaction_count ^= 1),
        ("state_version", |s| s.state_version ^= 1),
    ];
    assert_hash_sensitive(&changes);
}

#[test]
fn hash_does_not_recursively_commit_its_cached_output() {
    let mut statement = statement();
    let expected = statement.compute_hash();
    statement.hash = expected;
    assert_eq!(statement.hash(), expected);
    assert_eq!(statement.compute_hash(), expected);
    statement.hash = [0xff; 32];
    assert_eq!(statement.compute_hash(), expected);
}

type ParentMutation = (&'static str, fn(&mut ParentPoint));

#[test]
fn parent_validation_rejects_each_of_the_six_independent_mismatches() {
    let statement = statement();
    let parent = ParentPoint {
        height: statement.execution.parent_height,
        block_hash: statement.execution.parent_block_hash,
        state_root: statement.execution.parent_state_root,
        receipt_batch_commitment: statement.execution.parent_receipt_root,
        state_version: statement.execution.parent_state_version,
        decision_hash: statement.consensus.parent_decision_hash,
    };
    statement.validate_parent(&parent).unwrap();
    let changes: [ParentMutation; 6] = [
        ("height", |p| p.height ^= 1),
        ("block_hash", |p| p.block_hash[0] ^= 1),
        ("state_root", |p| p.state_root[0] ^= 1),
        ("receipt_batch_commitment", |p| {
            p.receipt_batch_commitment[0] ^= 1
        }),
        ("state_version", |p| p.state_version ^= 1),
        ("decision_hash", |p| p.decision_hash[0] ^= 1),
    ];
    for (name, mutate) in changes {
        let mut changed = parent;
        mutate(&mut changed);
        assert!(
            statement.validate_parent(&changed).is_err(),
            "unbound parent field: {name}"
        );
    }
}
