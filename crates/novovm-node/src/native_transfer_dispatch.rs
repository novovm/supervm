#![forbid(unsafe_code)]

//! Fresh-candidate NOV transfers: AOEM computes small immutable account views;
//! the final AOEM callback settles complete business effects in original order.
//! Receipt finalization is supplied by the candidate's state codec. The default
//! entry point retains the existing full candidate store/root codec.

use super::*;
use crate::native_transfer_delta::{
    Account, TransferExecutionFailureV1, TransferExecutionOutcomeV1, TransferIntent,
    TransferSnapshot,
};
use crate::native_transfer_executor::{
    execute_transfer_components_with_reducer_v1, TransferWorkV1,
};
use novovm_exec::AoemComputeSessionV1;

#[path = "native_transfer_business.rs"]
mod business;

pub(super) const FEE_PROJECTION_V1: &str =
    "nov-transfer-fee/v1:native_asset.transfer:json(asset,to_hex,amount_decimal):gas21000";

/// Local scheduler facts only. Never serialized into a receipt/state root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ComponentObservationV1 {
    pub transactions: usize,
    pub components: usize,
    pub recomputed_transactions: usize,
    pub graphs: usize,
    pub peak_inflight: usize,
}

#[cfg(test)]
std::thread_local! {
    static COMPONENT_OBSERVATION: std::cell::Cell<Option<ComponentObservationV1>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
pub(super) fn take_component_observation_for_test_v1() -> Option<ComponentObservationV1> {
    COMPONENT_OBSERVATION.with(std::cell::Cell::take)
}

fn record_component_observation_v1(observation: ComponentObservationV1) {
    #[cfg(test)]
    COMPONENT_OBSERVATION.with(|slot| slot.set(Some(observation)));
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED
        .get_or_init(|| std::env::var("NOVOVM_NATIVE_FRESH_TIMING").is_ok_and(|value| value == "1"))
    {
        use std::io::Write;
        let _ = writeln!(std::io::stdout().lock(),
            "native_transfer_components: transactions={} components={} recomputed_transactions={} sessions=1 graphs={} peak_inflight={}",
            observation.transactions, observation.components, observation.recomputed_transactions,
            observation.graphs, observation.peak_inflight);
    }
}

fn snapshot_v1(store: &NovNativeExecutionStoreV1, intent: &TransferIntent) -> TransferSnapshot {
    TransferSnapshot {
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
    }
}

fn outcome_binding_v1(outcome: &TransferExecutionOutcomeV1, intent: &TransferIntent) -> Result<()> {
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
    {
        bail!("AOEM transfer outcome does not match its authenticated input");
    }
    Ok(())
}

fn outcome_matches_snapshot_v1(
    outcome: &TransferExecutionOutcomeV1,
    snapshot: TransferSnapshot,
) -> bool {
    let delta = outcome.delta();
    delta.payer.before == snapshot.payer_balance
        && delta.recipient.before == snapshot.recipient_balance
        && delta.nonce_before == snapshot.next_nonce
}

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

/// A candidate-owned receipt boundary, not an authority publication hook.
///
/// `begin` observes the ordered pre-fee state. `finish` must commit the valid
/// nonce and receipt even for a fee/business rejection, before the next item
/// can read state. An error aborts execution; the caller must discard/recover
/// its isolated candidate, not assume that mutations have been rolled back.
pub(super) trait TransferReceiptFinalizerV1 {
    fn begin(&mut self, store: &NovNativeExecutionStoreV1) -> Result<()>;

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        store: &mut NovNativeExecutionStoreV1,
        transaction: &NovNativeTxWireV1,
        request: &NovExecutionRequestV1,
        settled_fee: &NovSettledFeeV1,
        subject: &NovExecutionSubjectMetaV1,
        reservation: &NovNativeDurableAuthReservationV1,
        ingress: NovAoemSemanticIngressMetaV1,
        now_ms: u128,
        receipt: NovNativeExecutionReceiptV1,
    ) -> Result<()>;
}

struct LegacyFinalizer<'a> {
    before: Option<NovNativeExecutionModuleStateV1>,
    mirrors: &'a mut Vec<NovAoemSemanticLedgerMirrorRecordV1>,
}

impl TransferReceiptFinalizerV1 for LegacyFinalizer<'_> {
    fn begin(&mut self, store: &NovNativeExecutionStoreV1) -> Result<()> {
        if self.before.is_some() {
            bail!("transfer legacy finalizer already has an unfinished item");
        }
        self.before = Some(store.module_state.clone());
        Ok(())
    }

    fn finish(
        &mut self,
        store: &mut NovNativeExecutionStoreV1,
        _transaction: &NovNativeTxWireV1,
        request: &NovExecutionRequestV1,
        settled_fee: &NovSettledFeeV1,
        subject: &NovExecutionSubjectMetaV1,
        reservation: &NovNativeDurableAuthReservationV1,
        ingress: NovAoemSemanticIngressMetaV1,
        now_ms: u128,
        receipt: NovNativeExecutionReceiptV1,
    ) -> Result<()> {
        let before = self
            .before
            .take()
            .context("transfer legacy finalizer has no pre-fee state")?;
        finalize_native_execution_receipt_v1(
            store,
            request,
            settled_fee,
            subject,
            Some(reservation),
            Some(ingress),
            &before,
            Path::new(""),
            Some(self.mirrors),
            now_ms,
            receipt,
        )?;
        Ok(())
    }
}

/// Preserve the existing finalization format until a candidate explicitly
/// selects another versioned state codec.
pub(super) fn execute_v1(
    store: &mut NovNativeExecutionStoreV1,
    items: &[Item<'_>],
    now_ms: u128,
    mirrors: &mut Vec<NovAoemSemanticLedgerMirrorRecordV1>,
) -> Result<usize> {
    execute_with_finalizer_v1(
        store,
        items,
        now_ms,
        &mut LegacyFinalizer {
            before: None,
            mirrors,
        },
    )
}

/// Execute AOEM computation and ordered unified fees without a full-state
/// clone or a prescribed receipt/root codec. Returns callback overlap for local
/// diagnostics only, never for a receipt or deterministic output commitment.
pub(super) fn execute_with_finalizer_v1(
    store: &mut NovNativeExecutionStoreV1,
    items: &[Item<'_>],
    now_ms: u128,
    finalizer: &mut impl TransferReceiptFinalizerV1,
) -> Result<usize> {
    #[cfg(test)]
    COMPONENT_OBSERVATION.with(std::cell::Cell::take);
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
    let mut work = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        // Nonce sequence has already been authenticated for the entire plan.
        // Check prior committed conflicts now; recheck the live prefix before
        // each ordered merge (later same-signer nonces are not parent nonces).
        if find_nov_native_durable_auth_receipt_v1(store, item.reservation)?.is_some() {
            bail!("candidate transfer unexpectedly replays a committed transaction");
        }
        work.push(TransferWorkV1 {
            intent: intents[index].clone(),
            snapshot: snapshot_v1(store, &intents[index]),
            fee_rejection: quotes[index].as_ref().err().cloned(),
        });
    }
    let runtime = native_aoem_owned_runtime_config_v1()?;
    // The actual fresh node owns a same-thread lifecycle scope. Preserve cold
    // sessions for callers without one, while reusing its resident computation
    // owner across batches. This call still waits for each graph to drain.
    let mut session = AoemComputeSessionV1::open_scoped(&runtime)?;
    let owned_items = items
        .iter()
        .map(business::OwnedItem::capture)
        .collect::<Result<Vec<_>>>()?;
    let captured = business::capture_store(store, &owned_items)?;
    let policy = resolve_treasury_settlement_policy_v1(&captured);
    let reduce_intents = intents.clone();
    let (computed, peak_inflight) = execute_transfer_components_with_reducer_v1(
        &mut session,
        work,
        Duration::from_secs(30),
        move |outcomes, components, count| {
            business::reduce(
                captured,
                owned_items,
                reduce_intents,
                quotes,
                policy,
                outcomes,
                components,
                count,
                now_ms,
            )
        },
    )?;
    let observation = ComponentObservationV1 {
        transactions: items.len(),
        components: computed.components,
        recomputed_transactions: computed.recomputed,
        graphs: 1,
        peak_inflight,
    };
    for (index, step) in computed.steps.into_iter().enumerate() {
        let item = &items[index];
        if check_nov_native_durable_auth_reservation_v1(store, item.reservation)?.is_some() {
            bail!("candidate transfer unexpectedly replays a committed transaction");
        }
        finalizer.begin(store)?;
        step.apply(store, &intents[index])?;
        finalizer.finish(
            store,
            item.transaction,
            item.request,
            &step.fee,
            &step.subject,
            item.reservation,
            item.ingress.clone(),
            now_ms,
            step.receipt,
        )?;
    }
    record_component_observation_v1(observation);
    Ok(observation.peak_inflight)
}

#[cfg(test)]
#[path = "native_transfer_dispatch_tests.rs"]
mod tests;
