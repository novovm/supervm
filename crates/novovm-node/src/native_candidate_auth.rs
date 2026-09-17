#![forbid(unsafe_code)]

//! Authenticate one complete candidate against its verified, immutable parent.
//! This boundary never admits pending transactions, reserves process-wide
//! nonces, loads live authority state, or treats a committed intent as a replay.

use super::super::*;
use crate::native_candidate_plan::NovNativeCandidateExecutionPlanV1;
use std::collections::HashSet;

pub(super) struct AuthenticatedItem {
    pub(super) native_tx: NovNativeTxWireV1,
    pub(super) ir: TxIR,
    pub(super) tx_hash: [u8; 32],
    pub(super) durable_auth_reservation: NovNativeDurableAuthReservationV1,
    pub(super) execution_subject: NovExecutionSubjectMetaV1,
    pub(super) requested_execution_behavior: NovRequestedExecutionBehaviorV1,
    pub(super) execution_request: NovExecutionRequestV1,
}

/// The caller must first verify the stored parent envelope and its exact
/// binding to the plan. Only a fully authenticated ordered batch is returned;
/// any failure discards all intermediate, candidate-local nonce calculations.
pub(super) fn authenticate_plan(
    plan: &NovNativeCandidateExecutionPlanV1,
    parent: &NovNativeExecutionStoreV1,
    params: &serde_json::Value,
) -> Result<Vec<AuthenticatedItem>> {
    plan.validate()?;
    if parent.authority_chain_id != Some(plan.context.chain_id) {
        bail!("candidate authentication parent authority chain mismatch");
    }
    if parent.module_state.protocol_config_commitment != to_hex(&plan.protocol_config_commitment) {
        bail!("candidate authentication parent protocol configuration mismatch");
    }

    let mut expected_nonces = parent.module_state.native_auth_next_nonces.clone();
    let mut seen_hashes = HashSet::with_capacity(plan.raw_txs.len());
    let mut seen_nonce_keys = HashSet::with_capacity(plan.raw_txs.len());
    let mut authenticated = Vec::with_capacity(plan.raw_txs.len());

    for (index, (raw, expected_hash)) in plan.raw_txs.iter().zip(&plan.tx_hashes).enumerate() {
        let native_tx = decode_nov_native_tx_wire_v1(raw)
            .with_context(|| format!("decode candidate transaction {index}"))?;
        if native_tx.chain_id != plan.context.chain_id {
            bail!("candidate authentication transaction {index} chain domain mismatch");
        }
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&native_tx)
            .with_context(|| format!("derive candidate transaction {index} signed intent"))?;
        let tx_hash = tx_hash_array_from_ir_v1(&ir);
        if tx_hash != *expected_hash {
            bail!("candidate authentication transaction {index} canonical hash mismatch");
        }
        if !seen_hashes.insert(tx_hash) {
            bail!("candidate authentication duplicate signed intent at transaction {index}");
        }

        // Call the pure verifier directly. Even ingress(false, false) records
        // pending-rejection observations on failure and is not isolated.
        verify_nov_native_auth_v1(params, &native_tx, &ir, tx_hash)
            .with_context(|| format!("authenticate candidate transaction {index}"))?;
        let NovTxKindV1::Execute(execute) = &native_tx.kind else {
            bail!("candidate authentication supports only execute transactions");
        };
        let execution_subject = subject_meta_from_execute_tx_v1(execute);
        let requested_execution_behavior = requested_execution_behavior_v1(
            effective_execution_policy_for_fee_asset_v1(
                execute.execution_policy,
                execute.fee_policy.pay_asset.as_str(),
            ),
            execute.privacy_mode,
        );
        let execution_request = nov_native_tx_to_execution_request_v1(&native_tx)?
            .context("candidate authentication requires an executable native request")?;

        // Preserve the current protocol's identity derivation for parity with
        // authority execution. Its raw account spelling versus signer fallback
        // alias debt must be closed by an explicit protocol migration, not by
        // silently choosing different nonce identities for candidate branches.
        let reservation = nov_native_durable_auth_reservation_v1(&native_tx, &ir, tx_hash);
        if !seen_nonce_keys.insert(reservation.ledger_key.clone()) {
            bail!("candidate authentication duplicate nonce key at transaction {index}");
        }
        if parent
            .module_state
            .native_auth_nonce_reservations
            .contains_key(&reservation.ledger_key)
            || parent.receipts.contains_key(&reservation.tx_hash)
        {
            bail!(
                "candidate authentication transaction {index} was already committed in its parent"
            );
        }
        let expected = expected_nonces
            .entry(reservation.identity_key.clone())
            .or_insert(0);
        if reservation.nonce != *expected {
            bail!(
                "candidate authentication nonce sequence mismatch transaction={index} expected={} got={}",
                *expected,
                reservation.nonce
            );
        }
        *expected = expected
            .checked_add(1)
            .context("candidate authentication nonce sequence overflow")?;

        authenticated.push(AuthenticatedItem {
            native_tx,
            ir,
            tx_hash,
            durable_auth_reservation: reservation,
            execution_subject,
            requested_execution_behavior,
            execution_request,
        });
    }
    Ok(authenticated)
}
