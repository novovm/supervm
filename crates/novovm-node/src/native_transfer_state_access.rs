#![forbid(unsafe_code)]

//! A bounded typed view for authenticated NOV/NOV transfer segments. This is
//! not a finalizer or a consensus root codec. Unloaded state is never exported.
//! The current physical codec stores journal/order windows as bounded array
//! leaves; trace, receipt, nonce and asset maps remain independent records.

use super::native_store_records::{self as records, NativeRecordAccessV1, RawPathChangeV1};
use super::*;
use std::collections::BTreeSet;

type RawPaths = BTreeMap<Vec<String>, Vec<u8>>;

#[derive(Clone, Copy)]
struct Access {
    required: bool,
    put: bool,
    delete: bool,
    structure: bool,
}

/// Exact access declaration. The caller supplies already authenticated wires;
/// reservation binding is checked again, but this does not replace ingress auth.
pub(super) struct TransferAccessV1 {
    paths: BTreeMap<Vec<String>, Access>,
    tx_hashes: BTreeSet<String>,
    chain_id: u64,
    count: usize,
}

// Every scalar read by the existing NOV fee policy resolver/trace builder.
// Policy fields are read-only: a Transfer cannot change governance parameters.
const READ_FIELDS: &[&str] = &[
    "native_auth_nonce_identity_scheme",
    "protocol_config_commitment",
    "treasury_policy_version",
    "treasury_policy_source",
    "treasury_reserve_share_bps",
    "treasury_fee_share_bps",
    "treasury_risk_buffer_share_bps",
    "treasury_min_reserve_bucket_nov",
    "treasury_min_fee_bucket_nov",
    "treasury_min_risk_buffer_nov",
    "treasury_settlement_paused",
    "treasury_redeem_paused",
    "mapped_lock_bridge_paused",
    "mapped_lock_min_confirmations",
    "mapped_lock_contract_address",
    "mapped_asset_burn_paused",
    "mapped_asset_release_paused",
    "mapped_asset_auto_heal_enabled",
    "mapped_asset_auto_heal_rollback_enabled",
    "clearing_enabled",
    "clearing_require_healthy_risk_buffer",
    "clearing_constrained_max_slippage_bps",
    "clearing_constrained_daily_usage_bps",
    "clearing_constrained_strategy",
    "clearing_daily_nov_hard_limit",
    "last_clearing_route",
];
const WRITE_FIELDS: &[&str] = &[
    "treasury_settled_nov_total",
    "treasury_settlements",
    "treasury_reserve_bucket_nov",
    "treasury_fee_bucket_nov",
    "treasury_risk_buffer_nov",
    "treasury_settlement_journal",
    "treasury_settlement_journal_next_seq",
    "clearing_daily_window_day",
    "clearing_daily_nov_used",
    "last_clearing_failure_code",
    "last_clearing_failure_reason",
    "last_clearing_failure_unix_ms",
    "last_clearing_candidates",
    "last_fee_quote",
    "last_fee_quote_failure",
    "last_execution_trace",
    "execution_trace_order",
    "aoem_semantic_ledger_sequence",
    "aoem_semantic_ledger_head",
];
const MAPS: &[&str] = &[
    "account_asset_balances",
    "native_auth_nonce_reservations",
    "native_auth_next_nonces",
    "treasury_reserves",
    "treasury_settled_by_asset",
    "treasury_settlement_failure_counts",
    "clearing_failure_counts",
    "fee_quote_failure_counts",
    "execution_traces_by_tx",
    "aoem_semantic_ledger_records",
];

fn path(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

impl TransferAccessV1 {
    pub(super) fn for_batch(
        batch: &[(&NovNativeTxWireV1, &NovNativeDurableAuthReservationV1)],
    ) -> Result<Self> {
        if batch.is_empty() || batch.len() > 1024 {
            bail!("sparse transfer access requires 1..=1024 authenticated transfers");
        }
        let mut access = Self {
            paths: BTreeMap::new(),
            tx_hashes: BTreeSet::new(),
            chain_id: batch[0].0.chain_id,
            count: batch.len(),
        };
        for parts in [&[][..], &["module_state"][..], &["receipts"][..]] {
            access.add(parts, true, false, false, true);
        }
        for field in ["schema", "authority_chain_id", "authority_namespace_digest"] {
            access.add(&[field], true, false, false, false);
        }
        access.add(&["last_updated_unix_ms"], true, true, false, false);
        for field in READ_FIELDS {
            access.add(&["module_state", field], true, false, false, false);
        }
        for field in WRITE_FIELDS {
            access.add(&["module_state", field], true, true, false, false);
        }
        for field in MAPS {
            access.add(&["module_state", field], true, false, false, true);
        }
        for field in ["treasury_reserves", "treasury_settled_by_asset"] {
            access.add(&["module_state", field, "NOV"], false, true, false, false);
        }
        for code in ["policy_fallback", "settlement_paused", "amount_overflow"] {
            access.add(
                &["module_state", "treasury_settlement_failure_counts", code],
                false,
                true,
                false,
                false,
            );
        }
        for code in [
            NovClearingFailureCodeV1::QuoteExpired,
            NovClearingFailureCodeV1::InsufficientUserBalance,
        ] {
            access.add(
                &[
                    "module_state",
                    "clearing_failure_counts",
                    &format!("NOV:{}", code.short_reason()),
                ],
                false,
                true,
                false,
                false,
            );
        }
        access.add(
            &[
                "module_state",
                "fee_quote_failure_counts",
                "NOV:max_pay_exceeded",
            ],
            false,
            true,
            false,
            false,
        );
        for &(wire, reservation) in batch {
            native_transfer_dispatch::require_execution_capability_v1(wire, true)?;
            let NovTxKindV1::Transfer(transfer) = &wire.kind else {
                bail!("sparse transfer access does not support Execute barriers");
            };
            let ir = nov_native_tx_to_adapter_tx_ir_v1(wire)?;
            let expected =
                nov_native_durable_auth_reservation_v1(wire, &ir, tx_hash_array_from_ir_v1(&ir))?;
            if wire.chain_id != access.chain_id || expected != *reservation {
                bail!("sparse transfer reservation/chain binding mismatch");
            }
            if !access.tx_hashes.insert(reservation.tx_hash.clone()) {
                bail!("sparse transfer batch contains a duplicate transaction");
            }
            for account in [&transfer.from, &transfer.to] {
                let account = to_hex_prefixed_v1(account);
                access.add(
                    &["module_state", "account_asset_balances", &account],
                    false,
                    true,
                    false,
                    true,
                );
                access.add(
                    &["module_state", "account_asset_balances", &account, "NOV"],
                    false,
                    true,
                    false,
                    false,
                );
            }
            access.add(
                &[
                    "module_state",
                    "native_auth_nonce_reservations",
                    &reservation.ledger_key,
                ],
                false,
                true,
                false,
                false,
            );
            access.add(
                &[
                    "module_state",
                    "native_auth_next_nonces",
                    &reservation.identity_key,
                ],
                false,
                true,
                false,
                false,
            );
            access.add(
                &["receipts", &reservation.tx_hash],
                false,
                true,
                false,
                false,
            );
            access.add(
                &[
                    "module_state",
                    "execution_traces_by_tx",
                    &reservation.tx_hash,
                ],
                false,
                true,
                true,
                false,
            );
        }
        Ok(access)
    }

    fn add(&mut self, parts: &[&str], required: bool, put: bool, delete: bool, structure: bool) {
        self.paths
            .entry(path(parts))
            .and_modify(|old| {
                old.required |= required;
                old.put |= put;
                old.delete |= delete;
            })
            .or_insert(Access {
                required,
                put,
                delete,
                structure,
            });
    }

    pub(super) fn load(&self, reader: &dyn NativeRecordAccessV1) -> Result<SparseTransferStateV1> {
        let mut paths = self.paths.clone();
        let mut before_records = BTreeMap::new();
        let mut sparse = sparse_records(&NovNativeExecutionStoreV1::default())?;
        for (parts, permission) in &paths {
            read_one(reader, parts, *permission, &mut before_records, &mut sparse)?;
        }
        let order: Vec<String> =
            serde_json::from_slice(&sparse[&path(&["module_state", "execution_trace_order"])])?;
        if order.len() > NOV_EXECUTION_TRACE_MAX_ENTRIES_V1
            || order.iter().collect::<BTreeSet<_>>().len() != order.len()
        {
            bail!("sparse transfer trace window exceeds its bound or contains duplicates");
        }
        for hash in &order {
            if normalize_tx_hash_hex_v1(hash) != *hash {
                bail!("sparse transfer trace window has a noncanonical key");
            }
            let parts = path(&["module_state", "execution_traces_by_tx", hash]);
            let permission = Access {
                required: true,
                put: self.tx_hashes.contains(hash),
                delete: true,
                structure: false,
            };
            paths.insert(parts.clone(), permission);
            read_one(reader, &parts, permission, &mut before_records, &mut sparse)?;
        }
        let sequence: u64 = serde_json::from_slice(
            &sparse[&path(&["module_state", "aoem_semantic_ledger_sequence"])],
        )?;
        for offset in 1..=self.count {
            let sequence = sequence
                .checked_add(u64::try_from(offset)?)
                .context("sparse transfer semantic sequence exhausted")?;
            let parts = path(&[
                "module_state",
                "aoem_semantic_ledger_records",
                &sequence.to_string(),
            ]);
            let permission = Access {
                required: false,
                put: true,
                delete: false,
                structure: false,
            };
            paths.insert(parts.clone(), permission);
            read_one(reader, &parts, permission, &mut before_records, &mut sparse)?;
            if before_records[&parts].is_some() {
                bail!("sparse transfer future semantic sequence is already occupied");
            }
        }
        // Serialization covers only defaults and the explicitly loaded small
        // footprint, never any unbounded source map. RawValue keeps u128 exact.
        let store = typed_sparse_store(&sparse, &paths)?;
        if store.schema != NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1
            || store
                .authority_chain_id
                .is_some_and(|id| id != self.chain_id)
        {
            bail!("sparse transfer store schema/chain mismatch");
        }
        verify_native_nonce_identity_scheme_v2(&store)?;
        validate_windows(&store)?;
        let canonical = sparse_records(&store)?;
        for (parts, value) in &before_records {
            if value.as_ref() != canonical.get(parts) {
                bail!("sparse transfer record is noncanonical or orphaned at {parts:?}");
            }
        }
        Ok(SparseTransferStateV1 {
            store,
            before_records,
            initial_sparse: canonical,
            paths,
        })
    }
}

fn read_one(
    reader: &dyn NativeRecordAccessV1,
    parts: &[String],
    access: Access,
    before: &mut BTreeMap<Vec<String>, Option<Vec<u8>>>,
    sparse: &mut RawPaths,
) -> Result<()> {
    if let Some(value) = before.get(parts) {
        if access.required && value.is_none() {
            bail!("required sparse transfer record missing at {parts:?}");
        }
        return Ok(());
    }
    let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    let value = reader
        .read_path(&refs)
        .with_context(|| format!("read sparse transfer record {parts:?}"))?;
    if access.required && value.is_none() {
        bail!("required sparse transfer record missing at {parts:?}");
    }
    if let Some(raw) = &value {
        if access.structure && raw.as_slice() != b"{}" {
            bail!("invalid sparse transfer object marker at {parts:?}");
        }
        sparse.insert(parts.to_vec(), raw.clone());
    } else {
        sparse.remove(parts);
    }
    before.insert(parts.to_vec(), value);
    Ok(())
}

fn sparse_records(store: &NovNativeExecutionStoreV1) -> Result<RawPaths> {
    records::encode(store)?
        .into_iter()
        .map(|(key, value)| {
            let (parts, raw) = records::unpack(&key, &value)?;
            Ok((
                parts,
                if raw == records::OBJECT {
                    b"{}".to_vec()
                } else {
                    raw.to_vec()
                },
            ))
        })
        .collect()
}

fn typed_sparse_store(
    raw: &RawPaths,
    access: &BTreeMap<Vec<String>, Access>,
) -> Result<NovNativeExecutionStoreV1> {
    let defaults = records::encode(&NovNativeExecutionStoreV1::default())?;
    let mut structures = BTreeSet::new();
    for (key, value) in defaults {
        let (parts, token) = records::unpack(&key, &value)?;
        if token == records::OBJECT {
            structures.insert(parts);
        }
    }
    structures.extend(
        access
            .iter()
            .filter(|(_, permission)| permission.structure)
            .map(|(parts, _)| parts.clone()),
    );
    let physical = raw
        .iter()
        .map(|(parts, token)| {
            let token = if structures.contains(parts) {
                records::OBJECT
            } else {
                token.as_slice()
            };
            Ok((records::key(parts)?, records::value(parts, token)?))
        })
        .collect::<Result<_>>()?;
    records::decode(physical)
}

fn validate_windows(store: &NovNativeExecutionStoreV1) -> Result<()> {
    if store.module_state.treasury_settlement_journal.len()
        > NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1
        || store.module_state.execution_trace_order.len() > NOV_EXECUTION_TRACE_MAX_ENTRIES_V1
    {
        bail!("sparse transfer history window exceeds its fixed bound");
    }
    let mut seen = BTreeSet::new();
    for hash in &store.module_state.execution_trace_order {
        if normalize_tx_hash_hex_v1(hash) != *hash || !seen.insert(hash) {
            bail!("sparse transfer trace order is not canonical/unique");
        }
        let trace = store
            .module_state
            .execution_traces_by_tx
            .get(hash)
            .context("sparse transfer trace window references a missing trace")?;
        if normalize_tx_hash_hex_v1(&trace.tx_id) != *hash {
            bail!("sparse transfer trace does not match its key");
        }
    }
    Ok(())
}

/// Private fields prevent callers from widening the authorized write set.
/// `working_store_mut` is for existing typed fee/receipt helpers only; exporting
/// rejects any mutation outside the declared footprint, including other assets.
pub(super) struct SparseTransferStateV1 {
    store: NovNativeExecutionStoreV1,
    before_records: BTreeMap<Vec<String>, Option<Vec<u8>>>,
    initial_sparse: RawPaths,
    paths: BTreeMap<Vec<String>, Access>,
}

impl SparseTransferStateV1 {
    #[cfg(test)]
    pub(super) fn working_store(&self) -> &NovNativeExecutionStoreV1 {
        &self.store
    }
    pub(super) fn working_store_mut(&mut self) -> &mut NovNativeExecutionStoreV1 {
        &mut self.store
    }
    #[cfg(test)]
    pub(super) fn read_paths(&self) -> impl Iterator<Item = &[String]> {
        self.before_records.keys().map(Vec::as_slice)
    }

    /// Validate a final, sorted patch against the captured bounded parent view.
    /// This proves the write footprint and exact typed encoding, not that its
    /// amounts/nonce/receipts are the expected transaction result. The caller
    /// must still bind those values and replay the patch from the verified roots.
    pub(super) fn validate_changes_v1(&self, changes: &[RawPathChangeV1]) -> Result<()> {
        let mut previous: Option<&[String]> = None;
        let mut after = self.initial_sparse.clone();
        for change in changes {
            let (parts, raw) = match change {
                RawPathChangeV1::Put { path, value } => (path, Some(value)),
                RawPathChangeV1::Delete { path } => (path, None),
            };
            if previous.is_some_and(|previous| previous >= parts.as_slice()) {
                bail!("sparse transfer patch paths must be strictly sorted and unique");
            }
            previous = Some(parts);
            let permission = self.paths.get(parts).with_context(|| {
                format!("sparse transfer wrote an unloaded record at {parts:?}")
            })?;
            let before = self
                .before_records
                .get(parts)
                .context("sparse transfer declared path has no captured parent value")?;
            if (raw.is_some() && !permission.put) || (raw.is_none() && !permission.delete) {
                bail!("sparse transfer changed a read-only record at {parts:?}");
            }
            if permission.structure {
                if before.is_some() || raw.is_none() {
                    bail!("sparse transfer cannot replace/delete an account object");
                }
                if raw.map(Vec::as_slice) != Some(b"{}".as_slice()) {
                    bail!("invalid sparse transfer object marker at {parts:?}");
                }
            }
            if raw == before.as_ref() {
                bail!("sparse transfer patch contains a non-changing record at {parts:?}");
            }
            if let Some(value) = raw {
                after.insert(parts.clone(), value.clone());
            } else {
                after.remove(parts);
            }
        }
        // Only this already-loaded footprint plus schema defaults is rebuilt.
        // It is never exposed as a complete ledger, and no historical map is
        // read. Round-trip equality rejects malformed/unknown typed fields,
        // noncanonical number tokens, and children without their object marker.
        let typed = typed_sparse_store(&after, &self.paths)?;
        validate_windows(&typed)?;
        if sparse_records(&typed)? != after {
            bail!("sparse transfer patch does not reproduce its exact typed records");
        }
        Ok(())
    }

    pub(super) fn changed_records(&self) -> Result<Vec<RawPathChangeV1>> {
        validate_windows(&self.store)?;
        let after = sparse_records(&self.store)?;
        let all: BTreeSet<_> = self.initial_sparse.keys().chain(after.keys()).collect();
        let mut changes = Vec::new();
        for parts in all {
            if self.initial_sparse.get(parts) == after.get(parts) {
                continue;
            }
            let raw = after.get(parts);
            if let Some(value) = raw {
                changes.push(RawPathChangeV1::Put {
                    path: parts.clone(),
                    value: value.clone(),
                });
            } else {
                changes.push(RawPathChangeV1::Delete {
                    path: parts.clone(),
                });
            }
        }
        self.validate_changes_v1(&changes)?;
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Reader {
        records: RawPaths,
        reads: RefCell<Vec<Vec<String>>>,
    }
    impl Reader {
        fn new(store: &NovNativeExecutionStoreV1) -> Self {
            Self {
                records: sparse_records(store).unwrap(),
                reads: RefCell::new(Vec::new()),
            }
        }
    }
    impl NativeRecordAccessV1 for Reader {
        fn read_path(&self, parts: &[&str]) -> Result<Option<Vec<u8>>> {
            let parts = path(parts);
            self.reads.borrow_mut().push(parts.clone());
            Ok(self.records.get(&parts).cloned())
        }
    }
    struct Fixture {
        wire: NovNativeTxWireV1,
        reservation: NovNativeDurableAuthReservationV1,
        request: NovExecutionRequestV1,
        subject: NovExecutionSubjectMetaV1,
    }
    impl Fixture {
        fn new(max_fee: u128) -> Self {
            let mut wire = NovNativeTxWireV1 {
                chain_id: 81742,
                kind: NovTxKindV1::Transfer(novovm_protocol::NovTransferTxV1 {
                    from: vec![1; 20],
                    to: vec![2; 32],
                    asset: "NOV".into(),
                    amount: 17,
                    nonce: 0,
                    fee_policy: NovFeePolicyV1 {
                        pay_asset: "NOV".into(),
                        max_pay_amount: max_fee,
                        slippage_bps: 0,
                    },
                }),
                signature: Vec::new(),
            };
            sign_nov_native_tx_with_seed_v1(&mut wire, [0x51; 32]).unwrap();
            let ir = nov_native_tx_to_adapter_tx_ir_v1(&wire).unwrap();
            let hash = tx_hash_array_from_ir_v1(&ir);
            let reservation = nov_native_durable_auth_reservation_v1(&wire, &ir, hash).unwrap();
            let request = native_transfer_dispatch::fee_request_v1(&wire, hash).unwrap();
            let subject = fallback_execution_subject_meta_v1(&request);
            Self {
                wire,
                reservation,
                request,
                subject,
            }
        }
        fn access(&self) -> TransferAccessV1 {
            TransferAccessV1::for_batch(&[(&self.wire, &self.reservation)]).unwrap()
        }
        fn store(&self) -> NovNativeExecutionStoreV1 {
            let mut store = NovNativeExecutionStoreV1 {
                authority_chain_id: Some(self.wire.chain_id),
                ..Default::default()
            };
            let state = &mut store.module_state;
            state.account_asset_balances.insert(
                self.subject.account_id.clone(),
                BTreeMap::from([("NOV".into(), 10000), ("USDT".into(), u128::MAX)]),
            );
            state.account_asset_balances.insert(
                to_hex_prefixed_v1(&[2; 32]),
                BTreeMap::from([("USDT".into(), 91)]),
            );
            state.account_asset_balances.insert(
                "unrelated-account".into(),
                BTreeMap::from([("NOV".into(), u128::MAX)]),
            );
            state
                .native_auth_next_nonces
                .insert("unrelated-identity".into(), 91);
            state
                .native_auth_nonce_reservations
                .insert("unrelated-reservation".into(), "retained".into());
            state.treasury_reserves.insert("USDT".into(), u128::MAX);
            state
                .treasury_settled_by_asset
                .insert("USDT".into(), u128::MAX);
            state
                .treasury_settlement_failure_counts
                .insert("unrelated".into(), 81);
            state
                .fee_quote_failure_counts
                .insert("USDT:max_pay_exceeded".into(), 91);
            state.treasury_reserve_share_bps = 4500;
            state.treasury_fee_share_bps = 3500;
            state.treasury_risk_buffer_share_bps = 2000;
            state.treasury_min_reserve_bucket_nov = 17;
            state.treasury_min_fee_bucket_nov = 18;
            state.treasury_min_risk_buffer_nov = 19;
            state.treasury_policy_version = 3;
            state.treasury_policy_source = "runtime_path".into();
            state.treasury_redeem_paused = true;
            state.mapped_asset_auto_heal_rollback_enabled = true;
            state.clearing_daily_window_day = 5;
            state.clearing_daily_nov_used = 71;
            state.clearing_daily_nov_hard_limit = 900;
            state.clearing_require_healthy_risk_buffer = true;
            state.clearing_constrained_daily_usage_bps = 7200;
            state.clearing_constrained_max_slippage_bps = 11;
            state.clearing_constrained_strategy = "blocked".into();
            store
        }
        fn fee(
            &self,
            store: &mut NovNativeExecutionStoreV1,
            expire: bool,
        ) -> std::result::Result<Vec<u8>, String> {
            let result = (|| -> Result<_> {
                let mut quote =
                    quote_fee_policy_from_execution_request_v1(&self.request, store, 123)?;
                if expire {
                    quote.expires_at_unix_ms = 122;
                }
                settle_fee_quote_into_treasury_v1(
                    store,
                    &quote,
                    &self.reservation.tx_hash,
                    &self.subject,
                    123,
                )
            })();
            result
                .map(|fee| serde_json::to_vec(&fee).unwrap())
                .map_err(|error| error.to_string())
        }
    }

    fn apply(
        store: &NovNativeExecutionStoreV1,
        changes: &[RawPathChangeV1],
    ) -> NovNativeExecutionStoreV1 {
        let mut physical = records::encode(store).unwrap();
        for change in changes {
            match change {
                RawPathChangeV1::Put { path, value } => {
                    physical.insert(
                        records::key(path).unwrap(),
                        records::value(
                            path,
                            &records::physical_path_value_v1(path, value).unwrap(),
                        )
                        .unwrap(),
                    );
                }
                RawPathChangeV1::Delete { path } => {
                    physical.remove(&records::key(path).unwrap());
                }
            }
        }
        records::decode(physical).unwrap()
    }

    #[test]
    fn sparse_transfer_fee_six_branches_match_full_store_byte_for_byte() {
        for branch in [
            "success",
            "cap",
            "insufficient",
            "paused",
            "overflow",
            "expired",
        ] {
            let fixture = Fixture::new(if branch == "cap" { 1 } else { 0 });
            let mut original = fixture.store();
            match branch {
                "insufficient" => {
                    original
                        .module_state
                        .account_asset_balances
                        .get_mut(&fixture.subject.account_id)
                        .unwrap()
                        .insert("NOV".into(), 0);
                }
                "paused" => original.module_state.treasury_settlement_paused = true,
                "overflow" => original.module_state.treasury_settled_nov_total = u128::MAX,
                _ => {}
            }
            let reader = Reader::new(&original);
            let mut sparse = fixture.access().load(&reader).unwrap();
            let mut full = original.clone();
            let expected = fixture.fee(&mut full, branch == "expired");
            assert_eq!(
                fixture.fee(sparse.working_store_mut(), branch == "expired"),
                expected,
                "{branch}"
            );
            assert_eq!(expected.is_ok(), branch == "success", "{branch}");
            let changes = sparse.changed_records().unwrap();
            fixture
                .access()
                .load(&Reader::new(&original))
                .unwrap()
                .validate_changes_v1(&changes)
                .unwrap();
            let merged = apply(&original, &changes);
            assert_eq!(
                serde_json::to_vec(&merged).unwrap(),
                serde_json::to_vec(&full).unwrap(),
                "{branch}"
            );
            assert!(!reader.reads.borrow().iter().any(|parts| parts
                .last()
                .is_some_and(|part| part == "USDT" || part.starts_with("unrelated"))));
            assert_eq!(reader.reads.borrow().len(), sparse.read_paths().count());
        }
    }

    #[test]
    fn sparse_transfer_account_creation_nonce_and_current_receipt_preserve_other_assets() {
        let fixture = Fixture::new(0);
        for recipient_exists in [true, false] {
            let mut original = fixture.store();
            let recipient = to_hex_prefixed_v1(&[2; 32]);
            if !recipient_exists {
                original
                    .module_state
                    .account_asset_balances
                    .remove(&recipient);
            }
            let mut sparse = fixture.access().load(&Reader::new(&original)).unwrap();
            let mut full = original.clone();
            for store in [&mut full, sparse.working_store_mut()] {
                store
                    .module_state
                    .account_asset_balances
                    .entry(recipient.clone())
                    .or_default()
                    .insert("NOV".into(), u128::MAX - 3);
                commit_nov_native_durable_auth_reservation_v1(store, &fixture.reservation).unwrap();
                let receipt = build_failed_native_receipt_v1(
                    &fixture.request,
                    &unresolved_settled_fee_v1(&fixture.request),
                    &fixture.subject,
                    "fee".into(),
                    "quote".into(),
                    "test rejection".into(),
                );
                store
                    .receipts
                    .insert(fixture.reservation.tx_hash.clone(), receipt);
            }
            let changes = sparse.changed_records().unwrap();
            fixture
                .access()
                .load(&Reader::new(&original))
                .unwrap()
                .validate_changes_v1(&changes)
                .unwrap();
            assert_eq!(apply(&original, &changes), full);
            assert_eq!(changes.iter().filter(|change| matches!(change, RawPathChangeV1::Put { path, .. } if path == &super::path(&["module_state", "account_asset_balances", &recipient]))).count(), usize::from(!recipient_exists));
            assert!(
                !sparse.working_store().module_state.account_asset_balances[&recipient]
                    .contains_key("USDT")
            );
        }
    }

    #[test]
    fn sparse_transfer_rejects_unloaded_writes_deletion_and_missing_required_records() {
        let fixture = Fixture::new(0);
        let original = fixture.store();
        for attack in 0..5 {
            let mut sparse = fixture.access().load(&Reader::new(&original)).unwrap();
            match attack {
                0 => {
                    sparse
                        .store
                        .module_state
                        .account_asset_balances
                        .get_mut(&fixture.subject.account_id)
                        .unwrap()
                        .insert("USDT".into(), 9);
                }
                1 => {
                    sparse
                        .store
                        .module_state
                        .native_auth_next_nonces
                        .insert("unloaded".into(), 9);
                }
                2 => sparse.store.module_state.treasury_fee_share_bps = 9,
                3 => {
                    sparse
                        .store
                        .module_state
                        .account_asset_balances
                        .remove(&fixture.subject.account_id);
                }
                _ => sparse.store.module_state.fee_oracle_source = "unloaded".into(),
            }
            assert!(sparse.changed_records().is_err(), "attack {attack}");
        }
        for field in [
            "treasury_settled_nov_total",
            "native_auth_nonce_identity_scheme",
            "execution_trace_order",
        ] {
            let mut reader = Reader::new(&original);
            reader.records.remove(&path(&["module_state", field]));
            assert!(fixture.access().load(&reader).is_err(), "missing {field}");
        }
        let mut reader = Reader::new(&original);
        reader.records.insert(
            path(&["module_state", "treasury_settled_nov_total"]),
            b"0.0".to_vec(),
        );
        assert!(fixture.access().load(&reader).is_err());
    }

    #[test]
    fn sparse_transfer_trace_window_eviction_is_entry_level_and_bounded() {
        let fixture = Fixture::new(0);
        let mut original = fixture.store();
        let fee = unresolved_settled_fee_v1(&fixture.request);
        let mut receipt = build_failed_native_receipt_v1(
            &fixture.request,
            &fee,
            &fixture.subject,
            "fee".into(),
            "quote".into(),
            "test".into(),
        );
        for index in 0..NOV_EXECUTION_TRACE_MAX_ENTRIES_V1 {
            receipt.tx_hash = format!("{index:064x}");
            let trace = build_execution_trace_v1(
                &fixture.request,
                &fee,
                &receipt,
                &fixture.subject,
                &original,
                index as u128,
            );
            persist_execution_trace_v1(&mut original, trace);
        }
        let oldest = original.module_state.execution_trace_order[0].clone();
        let reader = Reader::new(&original);
        let mut sparse = fixture.access().load(&reader).unwrap();
        assert_eq!(
            sparse
                .working_store()
                .module_state
                .execution_traces_by_tx
                .len(),
            512
        );
        receipt.tx_hash.clone_from(&fixture.reservation.tx_hash);
        let trace = build_execution_trace_v1(
            &fixture.request,
            &fee,
            &receipt,
            &fixture.subject,
            &original,
            900,
        );
        let mut full = original.clone();
        persist_execution_trace_v1(&mut full, trace.clone());
        persist_execution_trace_v1(sparse.working_store_mut(), trace);
        let changes = sparse.changed_records().unwrap();
        fixture
            .access()
            .load(&Reader::new(&original))
            .unwrap()
            .validate_changes_v1(&changes)
            .unwrap();
        assert_eq!(apply(&original, &changes), full);
        assert_eq!(changes.iter().filter(|change| matches!(change, RawPathChangeV1::Delete { path: parts } if parts == &path(&["module_state", "execution_traces_by_tx", &oldest]))).count(), 1);
        assert!(changes.iter().all(|change| !matches!(change, RawPathChangeV1::Delete { path: parts } if parts.first().is_some_and(|part| part == "receipts"))));
        let mut missing = Reader::new(&original);
        missing
            .records
            .remove(&path(&["module_state", "execution_traces_by_tx", &oldest]));
        assert!(fixture.access().load(&missing).is_err());
    }

    #[test]
    fn sparse_transfer_patch_validator_rejects_undeclared_and_readonly_writes() {
        let fixture = Fixture::new(0);
        let original = fixture.store();
        let sparse = fixture.access().load(&Reader::new(&original)).unwrap();
        let changes = [
            (
                "unrelated balance",
                RawPathChangeV1::Put {
                    path: path(&[
                        "module_state",
                        "account_asset_balances",
                        "unrelated-account",
                        "NOV",
                    ]),
                    value: b"9".to_vec(),
                },
            ),
            (
                "unloaded asset of touched account",
                RawPathChangeV1::Put {
                    path: path(&[
                        "module_state",
                        "account_asset_balances",
                        &fixture.subject.account_id,
                        "USDT",
                    ]),
                    value: b"9".to_vec(),
                },
            ),
            (
                "read-only fee policy",
                RawPathChangeV1::Put {
                    path: path(&["module_state", "treasury_fee_share_bps"]),
                    value: b"9".to_vec(),
                },
            ),
            (
                "protocol commitment",
                RawPathChangeV1::Put {
                    path: path(&["module_state", "protocol_config_commitment"]),
                    value: br#""different""#.to_vec(),
                },
            ),
            (
                "existing account marker",
                RawPathChangeV1::Put {
                    path: path(&[
                        "module_state",
                        "account_asset_balances",
                        &fixture.subject.account_id,
                    ]),
                    value: b"{}".to_vec(),
                },
            ),
            (
                "undeclared nonce",
                RawPathChangeV1::Put {
                    path: path(&[
                        "module_state",
                        "native_auth_next_nonces",
                        "unrelated-identity",
                    ]),
                    value: b"92".to_vec(),
                },
            ),
            (
                "undeclared reservation",
                RawPathChangeV1::Put {
                    path: path(&[
                        "module_state",
                        "native_auth_nonce_reservations",
                        "unrelated-reservation",
                    ]),
                    value: br#""replaced""#.to_vec(),
                },
            ),
            (
                "undeclared receipt",
                RawPathChangeV1::Put {
                    path: path(&["receipts", &"99".repeat(32)]),
                    value: b"{}".to_vec(),
                },
            ),
            (
                "receipt deletion",
                RawPathChangeV1::Delete {
                    path: path(&["receipts", &fixture.reservation.tx_hash]),
                },
            ),
        ];
        for (label, change) in changes {
            assert!(sparse.validate_changes_v1(&[change]).is_err(), "{label}");
        }
    }

    #[test]
    fn sparse_transfer_patch_validator_requires_unique_ordered_net_typed_changes() {
        let fixture = Fixture::new(0);
        let original = fixture.store();
        let sparse = fixture.access().load(&Reader::new(&original)).unwrap();
        let balance = path(&[
            "module_state",
            "account_asset_balances",
            &fixture.subject.account_id,
            "NOV",
        ]);
        let valid = vec![
            RawPathChangeV1::Put {
                path: path(&["last_updated_unix_ms"]),
                value: b"123".to_vec(),
            },
            RawPathChangeV1::Put {
                path: balance.clone(),
                value: b"9000".to_vec(),
            },
        ];
        sparse.validate_changes_v1(&valid).unwrap();
        sparse.validate_changes_v1(&[]).unwrap();
        let mut reversed = valid.clone();
        reversed.reverse();
        assert!(sparse.validate_changes_v1(&reversed).is_err());
        assert!(sparse
            .validate_changes_v1(&[valid[0].clone(), valid[0].clone()])
            .is_err());
        for value in [
            b"10000".as_slice(), // An exact existing value is not a delta.
            b"\"9\"",            // Wrong typed value.
            b"-9",               // A balance is unsigned.
            b"9.0",              // No floating-point normalization.
            b" 9",               // Only the canonical typed token is accepted.
            b"340282366920938463463374607431768211456", // u128 overflow.
        ] {
            assert!(
                sparse
                    .validate_changes_v1(&[RawPathChangeV1::Put {
                        path: balance.clone(),
                        value: value.to_vec(),
                    }])
                    .is_err(),
                "{value:?}"
            );
        }
        assert!(sparse
            .validate_changes_v1(&[RawPathChangeV1::Delete {
                path: path(&[
                    "module_state",
                    "execution_traces_by_tx",
                    &fixture.reservation.tx_hash,
                ]),
            }])
            .is_err()); // Absent-to-absent is not a net deletion.
        assert_eq!(sparse.working_store().last_updated_unix_ms, 0);
        assert_eq!(
            sparse.working_store().module_state.account_asset_balances[&fixture.subject.account_id]
                ["NOV"],
            10000
        ); // Validation never applies an untrusted patch to the captured view.
    }

    #[test]
    fn sparse_transfer_patch_validator_requires_canonical_new_account_structure() {
        let fixture = Fixture::new(0);
        let mut original = fixture.store();
        let recipient = to_hex_prefixed_v1(&[2; 32]);
        original
            .module_state
            .account_asset_balances
            .remove(&recipient);
        let sparse = fixture.access().load(&Reader::new(&original)).unwrap();
        let marker = path(&["module_state", "account_asset_balances", &recipient]);
        let leaf = RawPathChangeV1::Put {
            path: path(&["module_state", "account_asset_balances", &recipient, "NOV"]),
            value: u128::MAX.to_string().into_bytes(),
        };
        assert!(sparse
            .validate_changes_v1(std::slice::from_ref(&leaf))
            .is_err());
        for raw in [b"null".as_slice(), b"[]", b"{ }", b"{\"NOV\":1}"] {
            assert!(sparse
                .validate_changes_v1(&[
                    RawPathChangeV1::Put {
                        path: marker.clone(),
                        value: raw.to_vec(),
                    },
                    leaf.clone(),
                ])
                .is_err());
        }
        sparse
            .validate_changes_v1(&[
                RawPathChangeV1::Put {
                    path: marker,
                    value: b"{}".to_vec(),
                },
                leaf,
            ])
            .unwrap();
    }

    #[test]
    fn sparse_transfer_read_count_does_not_grow_with_unrelated_history() {
        let fixture = Fixture::new(0);
        let mut original = fixture.store();
        let before = Reader::new(&original);
        fixture.access().load(&before).unwrap();
        for index in 0..2000 {
            original
                .module_state
                .native_auth_nonce_reservations
                .insert(format!("old-{index}"), "reserved".into());
            original
                .module_state
                .native_auth_next_nonces
                .insert(format!("old-{index}"), 71);
            original
                .module_state
                .account_asset_balances
                .insert(format!("old-{index}"), BTreeMap::from([("NOV".into(), 7)]));
        }
        let after = Reader::new(&original);
        let sparse = fixture.access().load(&after).unwrap();
        assert_eq!(*before.reads.borrow(), *after.reads.borrow());
        assert!(sparse.changed_records().unwrap().is_empty());
        assert_eq!(sparse.store.module_state.account_asset_balances.len(), 2);
    }

    #[test]
    fn sparse_transfer_journal_uses_only_the_existing_bounded_array_leaf() {
        let fixture = Fixture::new(0);
        let mut original = fixture.store();
        fixture.fee(&mut original, false).unwrap();
        let entry = original.module_state.treasury_settlement_journal[0].clone();
        original.module_state.treasury_settlement_journal = (1..=512)
            .map(|sequence| {
                let mut entry = entry.clone();
                entry.seq = sequence;
                entry.tx_hash = format!("{sequence:064x}");
                entry
            })
            .collect();
        original.module_state.treasury_settlement_journal_next_seq = 512;
        let mut sparse = fixture.access().load(&Reader::new(&original)).unwrap();
        let mut full = original.clone();
        assert_eq!(
            fixture.fee(sparse.working_store_mut(), false),
            fixture.fee(&mut full, false)
        );
        assert_eq!(full.module_state.treasury_settlement_journal.len(), 512);
        assert_eq!(full.module_state.treasury_settlement_journal[0].seq, 2);
        assert_eq!(apply(&original, &sparse.changed_records().unwrap()), full);
        original
            .module_state
            .treasury_settlement_journal
            .push(entry);
        assert!(fixture.access().load(&Reader::new(&original)).is_err());
    }

    #[test]
    fn sparse_transfer_footprint_rejects_mismatched_auth_and_duplicate_inputs() {
        let fixture = Fixture::new(0);
        let mut reservation = fixture.reservation.clone();
        reservation.identity_key.push('0');
        assert!(TransferAccessV1::for_batch(&[(&fixture.wire, &reservation)]).is_err());
        assert!(TransferAccessV1::for_batch(&[
            (&fixture.wire, &fixture.reservation),
            (&fixture.wire, &fixture.reservation)
        ])
        .is_err());
        let mut store = fixture.store();
        store.authority_chain_id = Some(fixture.wire.chain_id + 1);
        assert!(fixture.access().load(&Reader::new(&store)).is_err());
    }
}
