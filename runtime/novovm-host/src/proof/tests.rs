//! Native evaluation of the shared guest relation and its wire boundary.
//! These tests do not generate or verify a cryptographic zkVM receipt.

use super::*;
use crate::business::direct_nov_fee::FeeState;
use crate::business::nov_transfer_batch::{
    balance_key, effect_contract, fee_record_changes, nonce_key, program_id, receipt_codec,
    SEMANTIC_VERSION,
};
use crate::business::quoted_transfer::Account;
use crate::ingress::authentication::authenticate_transfer_v3;
use crate::ingress::wire::{encode_transfer_v3, signing_message, FeePolicy, TransferV3};
use crate::state::tree::{empty_root, stage_state_update, StateChange, StateNodeReader};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const CHAIN: u64 = 91;
// Fixed V1 byte offsets are checked by decoding and re-encoding a real fixture.
const CONTEXT_END: usize = 316;
const CHAIN_OFFSET: usize = 8;
const PROGRAM_OFFSET: usize = 80;
const SEMANTIC_OFFSET: usize = 112;
const CONTRACT_OFFSET: usize = 116;
const PARENT_ROOT_OFFSET: usize = 188;
const RECEIPT_CODEC_OFFSET: usize = 260;

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
        policy_version: 3,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 3333,
        fee_share_bps: 3333,
        risk_buffer_share_bps: 3334,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn signed(seed: u8, nonce: u64) -> Vec<u8> {
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let public = signer.verifying_key().to_bytes();
    let mut tx = TransferV3 {
        chain_id: CHAIN,
        from: Sha256::digest(public)[12..].to_vec(),
        to: vec![90; 20],
        asset: "NOV".into(),
        amount: u128::from(seed),
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 0,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    let signature = signer.sign(&signing_message(&tx).unwrap());
    tx.signature = public.to_vec();
    tx.signature.extend_from_slice(&signature.to_bytes());
    encode_transfer_v3(&tx).unwrap()
}

struct Fixture {
    input: Vec<u8>,
    executed: ExecutedNovBatch,
}

fn fixture_with(payer_balance: u128, settlements: u64) -> Fixture {
    let policy = policy();
    let raw = vec![signed(21, 3), signed(22, 5)];
    let mut fees = FeeState::default();
    fees.accounting.settlements = settlements;
    fees.accounting.journal_next_seq = 11;
    fees.accounting.daily_window_day = 1;
    fees.accounting.daily_nov_used = 77;
    fees.diagnostics.quote_max_pay_exceeded = 4;
    let mut changes = fee_record_changes(&policy, &fees).unwrap();
    for bytes in &raw {
        let authenticated = authenticate_transfer_v3(bytes, CHAIN, MAX_TRANSACTION_BYTES).unwrap();
        let tx = authenticated.transfer();
        let payer = Account::try_from(tx.from.as_slice()).unwrap();
        changes.push(StateChange::Put {
            key: balance_key(&payer),
            value: payer_balance.to_le_bytes().to_vec(),
        });
        changes.push(StateChange::Put {
            key: nonce_key(&authenticated.nonce_identity()),
            value: tx.nonce.to_le_bytes().to_vec(),
        });
    }
    let parent = stage_state_update(&Memory::default(), empty_root(), &changes).unwrap();
    let memory = Memory(parent.nodes().clone());
    let context = BatchContext {
        chain_id: CHAIN,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&policy).unwrap(),
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: parent.root(),
        parent_receipt_root: empty_root(),
        parent_state_version: 17,
        receipt_codec: receipt_codec(),
        height: 1,
        slot: 0,
        timestamp_unix_ms: 2 * 86_400_000 + 123,
    };
    let authenticated = authenticate_batch_for_proof(CHAIN, raw, auth_budget()).unwrap();
    assert_eq!(authenticated.peak_callbacks(), 0);
    let prepared = NovTransferPlan::compile(authenticated, context, policy, plan_budget())
        .unwrap()
        .capture(&memory, capture_budget())
        .unwrap();
    let input = prepared.execution_proof_input().unwrap();
    drop(memory);
    Fixture {
        input,
        executed: prepared.execute_for_proof().unwrap(),
    }
}

fn fixture() -> Fixture {
    fixture_with(10_000, 7)
}

// Test-only framing can represent malicious bodies that a BatchPlan constructor
// properly refuses, e.g. duplicate raw input. It never constructs a capability.
fn reframe(base: &[u8], raw: &[Vec<u8>], policy: &[u8], witness: &[u8]) -> Vec<u8> {
    fn field(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_be_bytes());
        out.extend_from_slice(bytes);
    }
    let mut out = base[..CONTEXT_END].to_vec();
    out.extend_from_slice(&u32::try_from(raw.len()).unwrap().to_be_bytes());
    for bytes in raw {
        field(&mut out, bytes);
    }
    field(&mut out, policy);
    field(&mut out, witness);
    out
}

fn wire_rejects(input: &[u8], expected: &str) {
    let Err(error) = wire::decode(input) else {
        panic!("wire must reject");
    };
    assert!(error.to_string().contains(expected), "{error:#}");
}

fn relation_rejects(input: &[u8], expected: &str) {
    let error = execute_to_journal(input).unwrap_err();
    assert!(format!("{error:#}").contains(expected), "{error:#}");
}

#[test]
fn journal_has_fixed_versioned_size() {
    assert_eq!(JOURNAL_BYTES, 152);
    assert_eq!(JOURNAL_MAGIC, b"NVEXEC01");
}

#[test]
fn signed_parent_relation_matches_direct_result_and_fixed_journal_fields() {
    let fixture = fixture();
    let actual = execute_to_journal(&fixture.input).unwrap().encode();
    let batch = &fixture.executed;
    assert_eq!(
        actual,
        ExecutionJournalV1::from_executed(batch).unwrap().encode()
    );
    assert_eq!(&actual[..8], b"NVEXEC01");
    assert_eq!(&actual[8..40], &batch.effects().plan_commitment());
    assert_eq!(&actual[40..72], &batch.effects().update().root());
    assert_eq!(&actual[72..104], &batch.receipt_batch_commitment());
    assert_eq!(&actual[104..136], &batch.statement_commitment());
    assert_eq!(&actual[136..144], &2_u64.to_be_bytes());
    assert_eq!(&actual[144..152], &19_u64.to_be_bytes());
    assert_eq!(batch.observation().peak_callbacks, 0);
    assert_eq!(batch.fees().accounting.settlements, 9);
    assert_eq!(batch.fees().accounting.journal_next_seq, 13);
    assert_eq!(batch.fees().accounting.daily_window_day, 2);
    assert_eq!(batch.fees().accounting.daily_nov_used, 0);
    assert_eq!(batch.fees().diagnostics.quote_max_pay_exceeded, 4);
    assert_eq!(batch.receipts()[0].delta.nonce_before, 3);
    assert_eq!(batch.receipts()[0].delta.nonce_after, 4);
    assert_eq!(batch.receipts()[1].delta.nonce_before, 5);
    assert_eq!(batch.receipts()[1].delta.nonce_after, 6);
    assert!(batch.receipts().iter().all(|receipt| {
        receipt.failure.is_none()
            && receipt.delta.fee_funding_delta > 0
            && receipt.journal.is_some()
            && receipt.clear_clearing_candidates
    }));
}

#[test]
fn wire_roundtrip_preserves_context_raw_policy_and_original_parent_witness() {
    let fixture = fixture();
    let decoded = wire::decode(&fixture.input).unwrap();
    let plan = fixture.executed.effects().plan();
    assert_eq!(decoded.context, *plan.context());
    assert_eq!(decoded.raw, plan.raw_transactions());
    assert_eq!(decoded.policy, policy());
    assert_eq!(&decoded.witness[..8], b"NVFRNT01");
    assert_eq!(&decoded.witness[8..40], &plan.context().parent_state_root);
    assert_eq!(
        wire::encode(plan, &decoded.policy, decoded.witness).unwrap(),
        fixture.input
    );
    assert_eq!(
        reframe(
            &fixture.input,
            &decoded.raw,
            &postcard::to_allocvec(&decoded.policy).unwrap(),
            decoded.witness
        ),
        fixture.input
    );
}

#[test]
fn bad_signature_wrong_chain_nonce_and_duplicate_transactions_are_rejected() {
    let fixture = fixture();
    let decoded = wire::decode(&fixture.input).unwrap();
    let policy = postcard::to_allocvec(&decoded.policy).unwrap();
    let mut bad = decoded.raw.clone();
    *bad[0].last_mut().unwrap() ^= 1;
    relation_rejects(
        &reframe(&fixture.input, &bad, &policy, decoded.witness),
        "signature",
    );
    let mut nonce = decoded.raw.clone();
    nonce[0] = signed(21, 4); // Still a real signature, wrong authenticated parent nonce.
    relation_rejects(
        &reframe(&fixture.input, &nonce, &policy, decoded.witness),
        "nonce replay",
    );
    relation_rejects(
        &reframe(
            &fixture.input,
            &[decoded.raw[0].clone(), decoded.raw[0].clone()],
            &policy,
            decoded.witness,
        ),
        "duplicate canonical transaction",
    );
    let mut chain = fixture.input.clone();
    chain[CHAIN_OFFSET + 7] ^= 1;
    relation_rejects(&chain, "signed chain domain mismatch");
}

#[test]
fn wrong_business_profile_and_parent_policy_are_rejected() {
    let fixture = fixture();
    for (offset, expected) in [
        (PROGRAM_OFFSET, "unsupported NOV business program"),
        (SEMANTIC_OFFSET + 3, "unsupported NOV semantic version"),
        (CONTRACT_OFFSET, "fee/effect policy commitment mismatch"),
        (RECEIPT_CODEC_OFFSET, "unsupported NOV receipt codec"),
    ] {
        let mut input = fixture.input.clone();
        input[offset] ^= 0x80;
        relation_rejects(&input, expected);
    }
    let decoded = wire::decode(&fixture.input).unwrap();
    let mut alternate = decoded.policy.clone();
    alternate.quote_ttl_ms += 1;
    let mut input = reframe(
        &fixture.input,
        &decoded.raw,
        &postcard::to_allocvec(&alternate).unwrap(),
        decoded.witness,
    );
    // A self-consistent claimed policy/contract still cannot replace parent policy.
    input[CONTRACT_OFFSET..CONTRACT_OFFSET + 32]
        .copy_from_slice(&effect_contract(&alternate).unwrap());
    relation_rejects(&input, "fee policy differs from exact parent state");
}

#[test]
fn wrong_parent_or_corrupted_missing_and_trailing_witness_are_rejected() {
    let fixture = fixture();
    let mut parent = fixture.input.clone();
    parent[PARENT_ROOT_OFFSET] ^= 1;
    relation_rejects(&parent, "frontier witness parent root mismatch");
    let decoded = wire::decode(&fixture.input).unwrap();
    let policy = postcard::to_allocvec(&decoded.policy).unwrap();
    let mut bad = decoded.witness.to_vec();
    *bad.last_mut().unwrap() ^= 1;
    assert!(execute_to_journal(&reframe(&fixture.input, &decoded.raw, &policy, &bad)).is_err());
    bad = decoded.witness[..decoded.witness.len() - 1].to_vec();
    assert!(execute_to_journal(&reframe(&fixture.input, &decoded.raw, &policy, &bad)).is_err());
    bad = decoded.witness.to_vec();
    bad.push(0);
    relation_rejects(
        &reframe(&fixture.input, &decoded.raw, &policy, &bad),
        "frontier witness trailing bytes",
    );
}

#[test]
fn valid_alternate_contexts_change_journal_instead_of_claiming_rejection() {
    let fixture = fixture();
    let original = execute_to_journal(&fixture.input).unwrap().encode();
    // Genesis, protocol, parent receipt, parent state version and slot are
    // external trust pins, not facts the local economic relation can select.
    for offset in [16, 48, 220, 259, 307] {
        let mut input = fixture.input.clone();
        input[offset] ^= 1;
        let changed = execute_to_journal(&input).unwrap().encode();
        assert_ne!(changed, original, "context offset {offset}");
        assert_ne!(&changed[8..40], &original[8..40]);
        assert_eq!(&changed[40..104], &original[40..104]);
    }
    let mut timestamp = fixture.input.clone();
    timestamp[315] ^= 1;
    let changed = execute_to_journal(&timestamp).unwrap().encode();
    assert_ne!(changed, original);
    assert_ne!(
        &changed[72..104],
        &original[72..104],
        "quote times enter full receipts"
    );
    let mut successor = fixture.input.clone();
    successor[148..180].fill(9);
    successor[180..188].copy_from_slice(&1_u64.to_be_bytes());
    successor[292..300].copy_from_slice(&2_u64.to_be_bytes());
    let changed = execute_to_journal(&successor).unwrap().encode();
    assert_ne!(&changed[8..40], &original[8..40]);
    assert_eq!(&changed[40..104], &original[40..104]);
}

#[test]
fn invalid_height_and_exhausted_state_version_are_rejected() {
    let fixture = fixture();
    let mut height = fixture.input.clone();
    height[292..300].copy_from_slice(&2_u64.to_be_bytes());
    relation_rejects(&height, "batch height must follow its claimed parent");
    let mut state_version = fixture.input.clone();
    state_version[252..260].copy_from_slice(&u64::MAX.to_be_bytes());
    relation_rejects(&state_version, "batch state version exhausted");
}

#[test]
fn legal_transaction_reordering_and_different_parent_economics_change_journal() {
    let fixture = fixture();
    let original = execute_to_journal(&fixture.input).unwrap().encode();
    let decoded = wire::decode(&fixture.input).unwrap();
    let mut reversed = decoded.raw.clone();
    reversed.reverse(); // Independent signer nonces; this is a legal different batch.
    let input = reframe(
        &fixture.input,
        &reversed,
        &postcard::to_allocvec(&decoded.policy).unwrap(),
        decoded.witness,
    );
    let reordered = execute_to_journal(&input).unwrap().encode();
    assert_ne!(&reordered[8..40], &original[8..40]);
    assert_ne!(&reordered[72..104], &original[72..104]);
    for alternate in [fixture_with(10_001, 7), fixture_with(10_000, 8)] {
        let changed = execute_to_journal(&alternate.input).unwrap().encode();
        assert_ne!(&changed[8..40], &original[8..40]);
        assert_ne!(&changed[40..72], &original[40..72]);
    }
}

#[test]
fn all_wire_truncations_bad_version_and_trailing_bytes_are_rejected() {
    let fixture = fixture();
    for length in 0..fixture.input.len() {
        assert!(
            wire::decode(&fixture.input[..length]).is_err(),
            "truncation {length}"
        );
    }
    let mut version = fixture.input.clone();
    version[7] ^= 1;
    wire_rejects(&version, "version mismatch");
    let mut trailing = fixture.input.clone();
    trailing.push(0);
    wire_rejects(&trailing, "trailing bytes");
}

#[test]
fn wire_count_field_body_and_total_limits_are_checked_before_admission() {
    let fixture = fixture();
    let decoded = wire::decode(&fixture.input).unwrap();
    let policy = postcard::to_allocvec(&decoded.policy).unwrap();
    for count in [0, u32::MAX] {
        let mut input = fixture.input.clone();
        input[CONTEXT_END..CONTEXT_END + 4].copy_from_slice(&count.to_be_bytes());
        wire_rejects(&input, "transaction count exceeds bound");
    }
    let empty = reframe(&fixture.input, &[Vec::new()], &policy, decoded.witness);
    wire_rejects(&empty, "empty proof transaction");
    let mut field = fixture.input.clone();
    field[CONTEXT_END + 4..CONTEXT_END + 8].copy_from_slice(&u32::MAX.to_be_bytes());
    wire_rejects(&field, "field exceeds bound");
    let maximum = vec![vec![0; MAX_TRANSACTION_BYTES]; MAX_BODY_BYTES / MAX_TRANSACTION_BYTES];
    let input = reframe(&fixture.input, &maximum, &policy, decoded.witness);
    assert!(wire::decode(&input).is_ok(), "wire-only exact body bound");
    let mut too_many_bytes = maximum;
    too_many_bytes.push(vec![0]);
    wire_rejects(
        &reframe(&fixture.input, &too_many_bytes, &policy, decoded.witness),
        "body exceeds bound",
    );
    let raw_limit = reframe(
        &fixture.input,
        &[vec![0; MAX_TRANSACTION_BYTES + 1]],
        &policy,
        decoded.witness,
    );
    wire_rejects(&raw_limit, "field exceeds bound");
    let oversized = vec![0; MAX_INPUT_BYTES + 1];
    wire_rejects(&oversized, "input exceeds bound");
}

#[test]
fn oversized_noncanonical_and_invalid_policy_encoding_is_rejected() {
    let fixture = fixture();
    let decoded = wire::decode(&fixture.input).unwrap();
    let too_large = vec![0; MAX_POLICY_BYTES + 1];
    wire_rejects(
        &reframe(&fixture.input, &decoded.raw, &too_large, decoded.witness),
        "field exceeds bound",
    );
    let mut trailing = postcard::to_allocvec(&decoded.policy).unwrap();
    trailing.push(0);
    wire_rejects(
        &reframe(&fixture.input, &decoded.raw, &trailing, decoded.witness),
        "noncanonical proof policy",
    );
    let mut invalid = decoded.policy;
    invalid.reserve_share_bps = 0;
    wire_rejects(
        &reframe(
            &fixture.input,
            &decoded.raw,
            &postcard::to_allocvec(&invalid).unwrap(),
            decoded.witness,
        ),
        "fee shares must all be positive",
    );
}
