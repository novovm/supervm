//! Deterministic public TEST keys and state. This never creates a production
//! genesis or node. Expected journal comes from actual AOEM-native execution.

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use novovm_aoem::ComputeSession;
use novovm_host::business::direct_nov_fee::{DirectNovFeePolicy, FeeState};
use novovm_host::business::nov_transfer_batch::{
    balance_key, effect_contract, fee_record_changes, program_id, receipt_codec, NovTransferPlan,
    SEMANTIC_VERSION,
};
use novovm_host::business::quoted_transfer::Account;
use novovm_host::execution::plan::{BatchContext, PlanBudget};
use novovm_host::ingress::batch::{authenticate_batch, AuthenticationBudget};
use novovm_host::ingress::wire::{encode_transfer_v3, signing_message, FeePolicy, TransferV3};
use novovm_host::proof::{execute_to_journal, ExecutionJournalV1};
use novovm_host::state::frontier::CaptureBudget;
use novovm_host::state::tree::{
    empty_root, stage_state_update, NodeHash, StateChange, StateNodeReader,
};
use std::collections::BTreeMap;
use std::time::Duration;

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
        policy_source: "config_path".into(),
        resolution_source: "test_fixture".into(),
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

pub(super) fn create(library: &Path, directory: &Path) -> Result<()> {
    ensure!(!directory.exists(), "fixture directory already exists");
    let signer = SigningKey::from_bytes(&[11; 32]);
    let public = signer.verifying_key().to_bytes();
    let sender: Account = public.into();
    let mut tx = TransferV3 {
        chain_id: 91,
        from: sender.as_bytes().to_vec(),
        to: vec![99; 20],
        asset: "NOV".into(),
        amount: 10,
        nonce: 0,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 0,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    let signature = signer.sign(&signing_message(&tx)?);
    tx.signature = public.to_vec();
    tx.signature.extend_from_slice(&signature.to_bytes());
    let raw = encode_transfer_v3(&tx)?;
    let policy = policy();
    let mut changes = fee_record_changes(&policy, &FeeState::default())?;
    changes.push(StateChange::Put {
        key: balance_key(&sender),
        value: 100_000u128.to_le_bytes().to_vec(),
    });
    let mut memory = Memory::default();
    let initial = stage_state_update(&memory, empty_root(), &changes)?;
    let root = initial.root();
    memory.0.extend(initial.nodes().clone());
    let mut session = ComputeSession::open(library, 4)?;
    let timeout = Duration::from_secs(30);
    let authenticated = authenticate_batch(
        &mut session,
        91,
        vec![raw],
        AuthenticationBudget {
            transactions: 16,
            transaction_bytes: 4096,
            body_bytes: 64 * 1024,
        },
        timeout,
    )?;
    let context = BatchContext {
        chain_id: 91,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&policy)?,
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: empty_root(),
        parent_state_version: 0,
        receipt_codec: receipt_codec(),
        height: 1,
        slot: 0,
        timestamp_unix_ms: 172_800_123,
    };
    let input = NovTransferPlan::compile(
        authenticated,
        context,
        policy,
        PlanBudget {
            transactions: 16,
            transaction_bytes: 4096,
            body_bytes: 64 * 1024,
            access_keys: 4096,
        },
    )?
    .capture(
        &memory,
        CaptureBudget {
            keys: 4096,
            nodes: 16_384,
            bytes: 5 * 1024 * 1024,
        },
    )?;
    let proof_input = input.execution_proof_input()?;
    drop(memory);
    let actual = input.execute(&mut session, timeout)?;
    ensure!(
        actual.receipts().len() == 1 && actual.receipts()[0].failure.is_none(),
        "fixture transfer failed"
    );
    ensure!(
        actual.fees().accounting.settled_nov_total > 0,
        "fixture did not settle a fee"
    );
    let expected = ExecutionJournalV1::from_executed(&actual)?.encode();
    ensure!(
        execute_to_journal(&proof_input)?.encode() == expected,
        "proof relation differs from native AOEM"
    );
    std::fs::create_dir(directory)?;
    write_new(&directory.join("input.bin"), &proof_input)?;
    write_new(&directory.join("expected-journal.bin"), &expected)?;
    println!("fixture_only=true production_genesis=false native_aoem_executed=true transactions=1 input_bytes={} input_sha256={} expected_journal_sha256={}",
        proof_input.len(), hash(&proof_input), hash(&expected));
    Ok(())
}
