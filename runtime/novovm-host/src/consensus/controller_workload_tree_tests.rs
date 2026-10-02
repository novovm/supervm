//! Exercise the real NOV execution output, not a Put-only tree fixture: fee
//! pagination emits Deletes even when the account/nonce tail is all unique Put.
//! This is a compute regression, not persistence, finality or a TPS benchmark.

use super::{Workload, CHAIN_ID};
use crate::business::nov_transfer_batch::NovTransferPlan;
use crate::consensus::tests::{batch_context, library, policy, Memory};
use crate::execution::plan::PlanBudget;
use crate::ingress::batch::{authenticate_batch, AuthenticationBudget};
use crate::state::frontier::CaptureBudget;
use crate::state::tree::{empty_root, stage_state_update, StateChange};
use anyhow::Result;
use novovm_aoem::ComputeSession;
use std::time::Duration;

#[test]
#[ignore = "requires explicit real AOEM library; signed NOV mixed fee-page/account effects"]
fn real_nov_fee_page_deletes_do_not_disable_batched_account_updates() -> Result<()> {
    let timeout = Duration::from_secs(30);
    let mut session = ComputeSession::open(&library()?, 4)?;
    for size in [32, 1024] {
        let policy = policy();
        let workload = Workload::new(size, 2, policy.clone())?;
        let initial = stage_state_update(
            &Memory::default(),
            empty_root(),
            &workload.initial_changes()?,
        )?;
        let mut memory = Memory(initial.nodes().clone());
        let mut parent = initial.root();
        for height in 1..=2 {
            let mut context = batch_context(parent);
            context.height = height;
            context.parent_height = height - 1;
            context.parent_state_version = height - 1;
            // Explicit synthetic parent identity for this compute-only test;
            // it is not a persisted block or an authorization to publish.
            context.parent_block_hash = if height == 1 { [0; 32] } else { [0x73; 32] };
            context.timestamp_unix_ms += height;
            let now = context.timestamp_unix_ms;
            let authenticated = authenticate_batch(
                &mut session,
                CHAIN_ID,
                workload.raw_height(height)?,
                AuthenticationBudget {
                    transactions: size,
                    transaction_bytes: 1024,
                    body_bytes: size * 1024,
                },
                timeout,
            )?;
            let reads_before = crate::state::tree::read_batch_stats_for_test();
            let plan = NovTransferPlan::compile(
                authenticated,
                context,
                policy.clone(),
                PlanBudget {
                    transactions: size,
                    transaction_bytes: 1024,
                    body_bytes: size * 1024,
                    access_keys: size * 2 + 128,
                },
            )?;
            let declared_keys = plan.plan().declared_access().len();
            let input = plan.capture(
                &memory,
                CaptureBudget {
                    keys: size * 2 + 128,
                    nodes: 65_536,
                    bytes: 16 * 1024 * 1024,
                },
            )?;
            let reads_after = crate::state::tree::read_batch_stats_for_test();
            assert_eq!(reads_after.0 - reads_before.0, 1);
            assert_eq!(
                reads_after.1 - reads_before.1,
                declared_keys,
                "batch read must include every declared policy/fee page, including absent tails"
            );
            assert!(
                reads_after.1 - reads_before.1 > size * 2,
                "real NOV finalize_capture must batch all accounts/nonces and complete fee pages"
            );
            let executed = input.execute(&mut session, timeout)?;
            let update = executed.effects().update();
            let expected = workload.expected_through(height, |h| u128::from(now - height + h))?;
            let expected_records = expected.record_changes(&policy)?;
            assert!(expected_records
                .iter()
                .any(|change| matches!(change, StateChange::Delete { .. })));
            let expected_tree =
                stage_state_update(&Memory::default(), empty_root(), &expected_records)?;
            assert_eq!(update.parent_root(), parent);
            assert_eq!(update.root(), expected_tree.root());
            assert_eq!(executed.receipts().len(), size);
            assert!(executed
                .receipts()
                .iter()
                .all(|receipt| receipt.failure.is_none() && receipt.fee_failure.is_none()));
            assert_eq!(executed.fees(), &expected.fees);
            assert_eq!(expected.nonces.len(), size);
            assert!(expected.nonces.values().all(|nonce| *nonce == height));
            assert!(
                update.max_batched_puts() > size * 2,
                "fee-tail Deletes must not return the real account/nonce batch to per-key updates"
            );
            eprintln!(
                "real NOV batch={size} height={height} batched_reads={} max_batched_puts={} staged_calls={} reachable_nodes={}",
                reads_after.1 - reads_before.1,
                update.max_batched_puts(),
                update.staged_calls(),
                update.nodes().len()
            );
            parent = update.root();
            memory.0.extend(update.nodes().clone());
        }
    }
    Ok(())
}
