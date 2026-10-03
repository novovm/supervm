#![forbid(unsafe_code)]

//! Transaction-local typed projections into the batch's declared record set.
//! This journal neither executes policy nor authorizes writes. Its batch-end
//! net changes must still equal the sparse view's independent validated diff.
//! No whole Store, module, or growing map is encoded by either phase.

use super::*;
use serde::Serialize;

struct RecordEffectV1 {
    before: Option<Vec<u8>>,
    current: Option<Vec<u8>>,
}

pub(super) struct TransferRecordEffectsV1 {
    records: BTreeMap<Vec<String>, RecordEffectV1>,
}

#[derive(Default)]
struct ProjectionV1(BTreeMap<Vec<String>, Option<Vec<u8>>>);

impl ProjectionV1 {
    fn optional<T: Serialize + ?Sized>(&mut self, path: &[&str], value: Option<&T>) -> Result<()> {
        self.0.insert(
            path.iter().map(|part| (*part).to_owned()).collect(),
            value.map(physical::typed_raw_v1).transpose()?,
        );
        Ok(())
    }

    fn put<T: Serialize + ?Sized>(&mut self, path: &[&str], value: &T) -> Result<()> {
        self.optional(path, Some(value))
    }

    fn object(&mut self, path: &[&str], present: bool) {
        self.0.insert(
            path.iter().map(|part| (*part).to_owned()).collect(),
            present.then(|| b"{}".to_vec()),
        );
    }
}

macro_rules! project_module_fields {
    ($projection:ident, $module:ident, $($field:ident),+ $(,)?) => {
        $(
            $projection.put(&["module_state", stringify!($field)], &$module.$field)?;
        )+
    };
}

impl TransferRecordEffectsV1 {
    /// The map includes authorized absent records, not just existing values.
    /// Keep original spelling, including `{}` markers and exact u128 tokens.
    pub(super) fn new(before_records: BTreeMap<Vec<String>, Option<Vec<u8>>>) -> Self {
        Self {
            records: before_records
                .into_iter()
                .map(|(path, before)| {
                    let current = before.clone();
                    (path, RecordEffectV1 { before, current })
                })
                .collect(),
        }
    }

    /// Observe existing NOV business/fee helpers after nonce consumption and
    /// before receipt finalization. Missing accounts must not become zero-valued
    /// records; self-transfers project the same account only once.
    pub(super) fn business(
        &mut self,
        store: &NovNativeExecutionStoreV1,
        wire: &NovNativeTxWireV1,
        reservation: &NovNativeDurableAuthReservationV1,
    ) -> Result<Vec<RawPathChangeV1>> {
        native_transfer_dispatch::require_execution_capability_v1(wire, true)?;
        let NovTxKindV1::Transfer(transfer) = &wire.kind else {
            bail!("typed transfer record effects require a Transfer wire");
        };
        let module = &store.module_state;
        let mut projection = ProjectionV1::default();
        project_module_fields!(
            projection,
            module,
            treasury_settled_nov_total,
            treasury_settlements,
            treasury_reserve_bucket_nov,
            treasury_fee_bucket_nov,
            treasury_risk_buffer_nov,
            treasury_settlement_journal,
            treasury_settlement_journal_next_seq,
            clearing_daily_window_day,
            clearing_daily_nov_used,
            last_clearing_failure_code,
            last_clearing_failure_reason,
            last_clearing_failure_unix_ms,
            last_clearing_candidates,
            last_fee_quote,
            last_fee_quote_failure,
        );
        let payer = to_hex_prefixed_v1(&transfer.from);
        let recipient = to_hex_prefixed_v1(&transfer.to);
        for (index, account) in [&payer, &recipient].into_iter().enumerate() {
            if index == 1 && payer == recipient {
                continue;
            }
            let assets = module.account_asset_balances.get(account);
            projection.object(
                &["module_state", "account_asset_balances", account],
                assets.is_some(),
            );
            projection.optional(
                &["module_state", "account_asset_balances", account, "NOV"],
                assets.and_then(|assets| assets.get("NOV")),
            )?;
        }
        projection.optional(
            &["module_state", "treasury_reserves", "NOV"],
            module.treasury_reserves.get("NOV"),
        )?;
        projection.optional(
            &["module_state", "treasury_settled_by_asset", "NOV"],
            module.treasury_settled_by_asset.get("NOV"),
        )?;
        for code in ["policy_fallback", "settlement_paused", "amount_overflow"] {
            projection.optional(
                &["module_state", "treasury_settlement_failure_counts", code],
                module.treasury_settlement_failure_counts.get(code),
            )?;
        }
        for code in [
            NovClearingFailureCodeV1::QuoteExpired,
            NovClearingFailureCodeV1::InsufficientUserBalance,
        ] {
            let key = format!("NOV:{}", code.short_reason());
            projection.optional(
                &["module_state", "clearing_failure_counts", &key],
                module.clearing_failure_counts.get(&key),
            )?;
        }
        projection.optional(
            &[
                "module_state",
                "fee_quote_failure_counts",
                "NOV:max_pay_exceeded",
            ],
            module.fee_quote_failure_counts.get("NOV:max_pay_exceeded"),
        )?;
        projection.optional(
            &[
                "module_state",
                "native_auth_nonce_reservations",
                &reservation.ledger_key,
            ],
            module
                .native_auth_nonce_reservations
                .get(&reservation.ledger_key),
        )?;
        projection.optional(
            &[
                "module_state",
                "native_auth_next_nonces",
                &reservation.identity_key,
            ],
            module
                .native_auth_next_nonces
                .get(&reservation.identity_key),
        )?;
        self.apply(projection)
    }

    /// Observe only this receipt and the bounded windows changed by the existing
    /// finalizer. `evicted_trace` is the actual pre-persist eviction candidate;
    /// read its post-persist presence instead of assuming it was deleted.
    pub(super) fn finalized(
        &mut self,
        store: &NovNativeExecutionStoreV1,
        tx_hash: &str,
        sequence: u64,
        evicted_trace: Option<&str>,
    ) -> Result<Vec<RawPathChangeV1>> {
        let module = &store.module_state;
        let mut projection = ProjectionV1::default();
        project_module_fields!(
            projection,
            module,
            last_execution_trace,
            execution_trace_order,
            aoem_semantic_ledger_sequence,
            aoem_semantic_ledger_head,
        );
        projection.put(&["last_updated_unix_ms"], &store.last_updated_unix_ms)?;
        projection.optional(&["receipts", tx_hash], store.receipts.get(tx_hash))?;
        let sequence_key = sequence.to_string();
        projection.optional(
            &[
                "module_state",
                "aoem_semantic_ledger_records",
                &sequence_key,
            ],
            module.aoem_semantic_ledger_records.get(&sequence),
        )?;
        for key in std::iter::once(tx_hash).chain(evicted_trace) {
            projection.optional(
                &["module_state", "execution_traces_by_tx", key],
                module.execution_traces_by_tx.get(key),
            )?;
        }
        self.apply(projection)
    }

    /// Return sorted unique net effects, omitting changes reverted in this batch.
    /// This is not a substitute for the independent sparse access/diff check.
    pub(super) fn net_changes(&self) -> Vec<RawPathChangeV1> {
        self.records
            .iter()
            .filter_map(|(path, effect)| raw_change(path, &effect.before, &effect.current))
            .collect()
    }

    fn apply(&mut self, projection: ProjectionV1) -> Result<Vec<RawPathChangeV1>> {
        // Check every projected path before changing the journal, including
        // absent/no-op values. An unknown path cannot be silently ignored.
        for path in projection.0.keys() {
            if !self.records.contains_key(path) {
                bail!("typed transfer record effect is outside captured access: {path:?}");
            }
        }
        let mut changes = Vec::new();
        for (path, value) in projection.0 {
            let effect = self.records.get_mut(&path).expect("prechecked path");
            if let Some(change) = raw_change(&path, &effect.current, &value) {
                changes.push(change);
            }
            effect.current = value;
        }
        Ok(changes)
    }
}

fn raw_change(
    path: &[String],
    before: &Option<Vec<u8>>,
    after: &Option<Vec<u8>>,
) -> Option<RawPathChangeV1> {
    if before == after {
        return None;
    }
    Some(match after {
        Some(value) => RawPathChangeV1::Put {
            path: path.to_vec(),
            value: value.clone(),
        },
        None => RawPathChangeV1::Delete {
            path: path.to_vec(),
        },
    })
}

#[cfg(test)]
#[path = "native_transfer_record_effects_tests.rs"]
mod tests;
