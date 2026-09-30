#![forbid(unsafe_code)]

//! Fresh-candidate NOV transfers: AOEM computes small immutable account views;
//! the Host merges outcomes and the existing unified fee settlement in order.
//! This intentionally still uses the existing full candidate store/root codec.

use super::*;
use crate::native_transfer_delta::{
    conflict_segments, Account, TransferExecutionFailureV1, TransferIntent, TransferSnapshot,
};
use crate::native_transfer_executor::{execute_transfer_wave_v1, TransferWorkV1};

pub(super) const FEE_PROJECTION_V1: &str =
    "nov-transfer-fee/v1:native_asset.transfer:json(asset,to_hex,amount_decimal):gas21000";

/// Fresh candidates have a transfer executor; legacy durable ingress does not.
/// Keep this distinction explicit instead of opening unsupported legacy paths.
pub(super) fn require_execution_capability_v1(tx: &NovNativeTxWireV1, fresh: bool) -> Result<()> {
    match &tx.kind {
        NovTxKindV1::Execute(_) => Ok(()),
        NovTxKindV1::Transfer(transfer) if fresh => {
            if normalize_asset_symbol_v1(&transfer.asset) != "NOV"
                || normalize_asset_symbol_v1(&transfer.fee_policy.pay_asset) != "NOV"
            {
                bail!("fresh transfer execution currently supports NOV assets with NOV fees");
            }
            Account::try_from(transfer.from.as_slice()).map_err(anyhow::Error::msg)?;
            Account::try_from(transfer.to.as_slice()).map_err(anyhow::Error::msg)?;
            Ok(())
        }
        _ => bail!("native transaction kind is not supported by this execution path"),
    }
}

/// A fee/receipt view only: never replace the original signed wire or its TxIR.
pub(super) fn fee_request_v1(
    tx: &NovNativeTxWireV1,
    tx_hash: [u8; 32],
) -> Result<NovExecutionRequestV1> {
    require_execution_capability_v1(tx, true)?;
    let NovTxKindV1::Transfer(transfer) = &tx.kind else {
        bail!("transfer fee projection requires a Transfer wire");
    };
    #[derive(serde::Serialize)]
    struct Arguments {
        asset: &'static str,
        to: String,
        amount: String,
    }
    Ok(NovExecutionRequestV1 {
        tx_hash,
        chain_id: tx.chain_id,
        caller: transfer.from.clone(),
        target: NovExecutionRequestTargetV1::NativeModule("native_asset".into()),
        method: "transfer".into(),
        args: serde_json::to_vec(&Arguments {
            asset: "NOV",
            to: to_hex_prefixed_v1(&transfer.to),
            amount: transfer.amount.to_string(),
        })?,
        fee_pay_asset: transfer.fee_policy.pay_asset.clone(),
        fee_max_pay_amount: transfer.fee_policy.max_pay_amount,
        fee_slippage_bps: transfer.fee_policy.slippage_bps,
        gas_like_limit: Some(21_000),
        nonce: transfer.nonce,
    })
}

fn quote_v1(request: &NovExecutionRequestV1, now_ms: u128) -> Result<NovFeeQuoteV1> {
    if normalize_asset_symbol_v1(&request.fee_pay_asset) != "NOV" {
        bail!("parallel transfer quotation requires NOV fee payment");
    }
    build_execution_fee_quote_v1(
        request,
        NOV_FEE_RATE_PPM_NOV_V1,
        "direct_nov".into(),
        now_ms,
        now_ms,
    )
}

pub(super) struct Item<'a> {
    pub transaction: &'a NovNativeTxWireV1,
    pub request: &'a NovExecutionRequestV1,
    pub subject: &'a NovExecutionSubjectMetaV1,
    pub reservation: &'a NovNativeDurableAuthReservationV1,
    pub ingress: NovAoemSemanticIngressMetaV1,
}

/// Returns observed callback overlap for local diagnostics only. Never put it
/// in a receipt, state root, or byte-identical recoverable output descriptor.
pub(super) fn execute_v1(
    store: &mut NovNativeExecutionStoreV1,
    items: &[Item<'_>],
    now_ms: u128,
    mirrors: &mut Vec<NovAoemSemanticLedgerMirrorRecordV1>,
) -> Result<usize> {
    if items.is_empty() || items.len() > 1024 {
        bail!("transfer dispatch requires 1..=1024 authenticated transactions");
    }
    let mut intents = Vec::with_capacity(items.len());
    let mut quotes = Vec::with_capacity(items.len());
    for item in items {
        let expected_request = fee_request_v1(item.transaction, item.request.tx_hash)?;
        if expected_request != *item.request
            || item.reservation.tx_hash != to_hex(&item.request.tx_hash)
            || item.reservation.nonce != item.request.nonce
        {
            bail!("transfer dispatch input binding mismatch");
        }
        let NovTxKindV1::Transfer(transfer) = &item.transaction.kind else {
            unreachable!("fee projection checked the transaction kind");
        };
        let sender = to_hex_prefixed_v1(&transfer.from);
        if item.subject.account_id != sender
            || item.subject.fee_owner_account_id != sender
            || item.subject.nonce_owner_account_id != sender
        {
            bail!("transfer dispatch payer/subject binding mismatch");
        }
        let quote = quote_v1(item.request, now_ms).map_err(|error| error.to_string());
        intents.push(TransferIntent {
            tx_hash: item.request.tx_hash,
            from: Account::try_from(transfer.from.as_slice()).map_err(anyhow::Error::msg)?,
            to: Account::try_from(transfer.to.as_slice()).map_err(anyhow::Error::msg)?,
            nonce_identity: item.reservation.identity_key.clone(),
            nonce: transfer.nonce,
            amount: transfer.amount,
            approved_fee: estimate_execution_fee_nov_v1(item.request),
            fee_cap: quote
                .as_ref()
                .map(|quote| quote.max_pay_amount)
                .unwrap_or(0),
        });
        quotes.push(quote);
    }
    let runtime = native_aoem_owned_runtime_config_v1()?;
    let mut peak = 0;
    for indices in conflict_segments(&intents) {
        // Capture only after the preceding segment's global reduction. No
        // transaction arithmetic is preexecuted on the submitting Host.
        let mut work = Vec::with_capacity(indices.len());
        for &index in &indices {
            let item = &items[index];
            if check_nov_native_durable_auth_reservation_v1(store, item.reservation)?.is_some() {
                bail!("candidate transfer unexpectedly replays a committed transaction");
            }
            let intent = &intents[index];
            work.push(TransferWorkV1 {
                intent: intent.clone(),
                snapshot: TransferSnapshot {
                    payer_balance: native_account_asset_balance_v1(
                        store,
                        &intent.from.to_hex_prefixed(),
                        "NOV",
                    ),
                    recipient_balance: native_account_asset_balance_v1(
                        store,
                        &intent.to.to_hex_prefixed(),
                        "NOV",
                    ),
                    next_nonce: store
                        .module_state
                        .native_auth_next_nonces
                        .get(&intent.nonce_identity)
                        .copied()
                        .unwrap_or(0),
                },
                fee_rejection: quotes[index].as_ref().err().cloned(),
            });
        }
        let computed = execute_transfer_wave_v1(&runtime, work, Duration::from_secs(30))?;
        peak = peak.max(computed.peak_inflight);
        for (index, result) in indices.into_iter().zip(computed.outcomes) {
            // Runtime/nonce/invariant failures abort the isolated candidate;
            // they are never turned into business failure receipts.
            let mut outcome = result.context("AOEM transfer input invariant failed")?;
            let item = &items[index];
            let intent = &intents[index];
            let payer = intent.from.to_hex_prefixed();
            let recipient = intent.to.to_hex_prefixed();
            let delta = outcome.delta();
            if delta.tx_hash != intent.tx_hash
                || delta.payer.account != intent.from
                || delta.recipient.account != intent.to
                || delta.nonce_identity != intent.nonce_identity
                || delta.nonce_before != intent.nonce
                || delta.nonce_after
                    != intent
                        .nonce
                        .checked_add(1)
                        .context("transfer nonce exhausted")?
                || native_account_asset_balance_v1(store, &payer, "NOV") != delta.payer.before
                || native_account_asset_balance_v1(store, &recipient, "NOV")
                    != delta.recipient.before
            {
                bail!("AOEM transfer outcome does not match its ordered pre-fee state");
            }
            let computed_digest = to_hex(&sha256_bytes_v1(&[
                b"novovm-native-transfer-compute-output-v1\0",
                &serde_json::to_vec(&outcome)?,
            ]));
            let before = store.module_state.clone();
            let subject = enforce_requested_execution_behavior_with_observability_v1(
                item.subject,
                None,
                Some(UcaKeyAlgo::Ed25519),
                None,
                false,
            )
            .map_err(|_| anyhow::anyhow!("standard transfer policy unexpectedly rejected"))?;
            // Reuse the existing wrapper for exact quote observability. Its
            // pure result must equal the quote used by the AOEM computation.
            let quote = quote_fee_policy_from_execution_request_v1(item.request, store, now_ms);
            if quote.as_ref().map_err(|error| error.to_string())
                != quotes[index].as_ref().map_err(Clone::clone)
            {
                bail!("transfer pure quote differs from ordered unified quote");
            }
            let settled = match quote {
                Ok(quote) => settle_fee_quote_into_treasury_v1(
                    store,
                    &quote,
                    &item.reservation.tx_hash,
                    &subject,
                    now_ms,
                ),
                Err(error) => Err(error),
            };
            let (settled_fee, mut receipt) = match settled {
                Err(error) => {
                    let reason = error.to_string();
                    outcome = outcome.reject_fee(reason.clone());
                    if native_account_asset_balance_v1(store, &payer, "NOV")
                        != outcome.delta().payer.before
                        || native_account_asset_balance_v1(store, &recipient, "NOV")
                            != outcome.delta().recipient.before
                    {
                        bail!("rejected direct NOV fee changed a monetary balance");
                    }
                    let fee = unresolved_settled_fee_v1(item.request);
                    let method = if is_fee_quote_reason_v1(&reason) {
                        "quote"
                    } else {
                        "settlement"
                    };
                    let receipt = build_failed_native_receipt_v1(
                        item.request,
                        &fee,
                        &subject,
                        "fee".into(),
                        method.into(),
                        reason,
                    );
                    (fee, receipt)
                }
                Ok(fee) => {
                    if matches!(outcome.failure(), Some(TransferExecutionFailureV1::Fee(_)))
                        || outcome.delta().fee_funding_delta != fee.nov_amount
                        || fee.source_amount != fee.nov_amount
                        || fee.source_asset != "NOV"
                        || native_account_asset_balance_v1(store, &payer, "NOV")
                            != outcome
                                .delta()
                                .payer
                                .before
                                .checked_sub(fee.nov_amount)
                                .context("settled fee exceeds input balance")?
                    {
                        bail!("AOEM transfer fee outcome differs from unified settlement");
                    }
                    // Settlement already debited the fee. These are absolute
                    // AOEM-computed after values, NOT another debit operation.
                    store
                        .module_state
                        .account_asset_balances
                        .entry(payer.clone())
                        .or_default()
                        .insert("NOV".into(), outcome.delta().payer.after);
                    if payer != recipient && outcome.is_success() {
                        store
                            .module_state
                            .account_asset_balances
                            .entry(recipient.clone())
                            .or_default()
                            .insert("NOV".into(), outcome.delta().recipient.after);
                    }
                    let receipt = match outcome.failure() {
                        Some(TransferExecutionFailureV1::Business(error)) => {
                            build_failed_native_receipt_v1(
                                item.request,
                                &fee,
                                &subject,
                                "native_asset".into(),
                                "transfer".into(),
                                format!("native.transfer.{error}"),
                            )
                        }
                        None => build_success_native_receipt_v1(
                            item.request,
                            &fee,
                            &subject,
                            "native_asset",
                            "transfer",
                            vec![NovNativeExecutionLogV1 {
                                module: "native_asset".into(),
                                method: "transfer".into(),
                                event: "native_asset.transferred".into(),
                                data: serde_json::json!({"asset":"NOV", "from":payer, "to":recipient, "amount":intent.amount.to_string()}),
                            }],
                        ),
                        Some(TransferExecutionFailureV1::Fee(_)) => {
                            unreachable!("checked fee outcome")
                        }
                    };
                    (fee, receipt)
                }
            };
            receipt.logs.push(NovNativeExecutionLogV1 {
                module: "aoem".into(), method: "native_transfer_compute".into(), event: "aoem.native_transfer.computed".into(),
                data: serde_json::json!({"scheduler":"aoem_generic_compute_v2", "tx_hash":item.reservation.tx_hash, "output_digest":computed_digest,
                    "phase":"pre_global_fee_reduction", "authorizes_state_publication":false}),
            });
            finalize_native_execution_receipt_v1(
                store,
                item.request,
                &settled_fee,
                &subject,
                Some(item.reservation),
                Some(item.ingress.clone()),
                &before,
                Path::new(""),
                Some(mirrors),
                now_ms,
                receipt,
            )?;
        }
    }
    Ok(peak)
}

#[cfg(test)]
#[path = "native_transfer_dispatch_tests.rs"]
mod tests;
