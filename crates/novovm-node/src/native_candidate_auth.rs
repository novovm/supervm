#![forbid(unsafe_code)]

//! Authenticate one complete candidate against its verified, immutable parent.
//! This boundary never admits pending transactions, reserves process-wide
//! nonces, loads live authority state, or treats a committed intent as a replay.

use super::super::native_store_records::NativeRecordAccessV1;
use super::super::*;
use crate::native_block_ledger::{
    NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1, NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1,
};
use crate::native_candidate_plan::NovNativeCandidateExecutionPlanV1;
use serde::de::DeserializeOwned;
use std::collections::{HashMap, HashSet};

/// A peer-supplied candidate is invalid. Storage reads, corrupt parent records,
/// local configuration and AOEM failures must not acquire this classification:
/// the lifecycle may reject this input, but must surface those other failures.
#[derive(Debug)]
pub(crate) struct CandidateInputRejected(anyhow::Error);

impl CandidateInputRejected {
    pub(crate) fn from_error(error: anyhow::Error) -> anyhow::Error {
        anyhow::Error::new(Self(error))
    }
}

impl std::fmt::Display for CandidateInputRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, formatter)
    }
}

impl std::error::Error for CandidateInputRejected {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Keep the original context text exactly once, with its underlying
        // source chain. Classification never depends on that display text.
        let original: &(dyn std::error::Error + Send + Sync) = self.0.as_ref();
        original.source()
    }
}

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
    authenticate_parent_plan(plan, parent, params)
}

/// Authenticate from immutable point reads, without loading historical maps.
/// The caller must bind the physical/state/receipt roots and authority namespace
/// to the verified parent and plan, and cross-check each consensus-bearing read
/// against its consensus tree. A record reader is not an authority capability.
pub(super) fn authenticate_record_plan(
    plan: &NovNativeCandidateExecutionPlanV1,
    parent: &dyn NativeRecordAccessV1,
    params: &serde_json::Value,
) -> Result<Vec<AuthenticatedItem>> {
    authenticate_parent_plan(plan, &RecordParentAuth(parent), params)
}

trait ParentAuthAccess {
    fn verify_domain(&self, chain: u64, protocol: [u8; 32]) -> Result<()>;
    fn contains_reservation(&self, ledger_key: &str) -> Result<bool>;
    fn contains_receipt(&self, tx_hash: &str) -> Result<bool>;
    fn next_nonce(&self, identity_key: &str) -> Result<u64>;
}

impl ParentAuthAccess for NovNativeExecutionStoreV1 {
    fn verify_domain(&self, chain: u64, protocol: [u8; 32]) -> Result<()> {
        verify_native_nonce_identity_scheme_v2(self)?;
        verify_parent_domain(
            chain,
            protocol,
            self.authority_chain_id,
            &self.module_state.protocol_config_commitment,
        )
    }

    fn contains_reservation(&self, ledger_key: &str) -> Result<bool> {
        Ok(self
            .module_state
            .native_auth_nonce_reservations
            .contains_key(ledger_key))
    }

    fn contains_receipt(&self, tx_hash: &str) -> Result<bool> {
        Ok(self.receipts.contains_key(tx_hash))
    }

    fn next_nonce(&self, identity_key: &str) -> Result<u64> {
        Ok(self
            .module_state
            .native_auth_next_nonces
            .get(identity_key)
            .copied()
            .unwrap_or(0))
    }
}

fn verify_parent_domain(
    chain: u64,
    protocol: [u8; 32],
    authority_chain_id: Option<u64>,
    protocol_config_commitment: &str,
) -> Result<()> {
    if authority_chain_id != Some(chain) {
        bail!("candidate authentication parent authority chain mismatch");
    }
    if protocol_config_commitment != to_hex(&protocol) {
        bail!("candidate authentication parent protocol configuration mismatch");
    }
    Ok(())
}

struct RecordParentAuth<'a>(&'a dyn NativeRecordAccessV1);

impl RecordParentAuth<'_> {
    fn required<T: DeserializeOwned>(&self, path: &[&str]) -> Result<T> {
        let raw = self.0.read_path(path)?.with_context(|| {
            format!("candidate authentication required parent record missing: {path:?}")
        })?;
        serde_json::from_slice(&raw).with_context(|| {
            format!("candidate authentication parent record has invalid type: {path:?}")
        })
    }

    fn require_object(&self, path: &[&str]) -> Result<()> {
        if self.0.read_path(path)?.as_deref() != Some(b"{}") {
            bail!("candidate authentication required parent object marker missing or invalid: {path:?}");
        }
        Ok(())
    }
}

impl ParentAuthAccess for RecordParentAuth<'_> {
    fn verify_domain(&self, chain: u64, protocol: [u8; 32]) -> Result<()> {
        // A missing map is corruption, not proof that all of its keys are absent.
        for path in [
            &[][..],
            &["module_state"][..],
            &["module_state", "native_auth_next_nonces"][..],
            &["module_state", "native_auth_nonce_reservations"][..],
            &["receipts"][..],
        ] {
            self.require_object(path)?;
        }
        let scheme: String =
            self.required(&["module_state", "native_auth_nonce_identity_scheme"])?;
        if scheme != NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2 {
            bail!(
                "native nonce identity scheme is legacy or unsupported: {:?}; offline migration preflight and explicit protocol activation are required",
                scheme
            );
        }
        let parent_chain = self.required(&["authority_chain_id"])?;
        let parent_protocol: String =
            self.required(&["module_state", "protocol_config_commitment"])?;
        verify_parent_domain(chain, protocol, parent_chain, &parent_protocol)
    }

    fn contains_reservation(&self, ledger_key: &str) -> Result<bool> {
        Ok(self
            .0
            .read_path(&["module_state", "native_auth_nonce_reservations", ledger_key])?
            .is_some())
    }

    fn contains_receipt(&self, tx_hash: &str) -> Result<bool> {
        Ok(self.0.read_path(&["receipts", tx_hash])?.is_some())
    }

    fn next_nonce(&self, identity_key: &str) -> Result<u64> {
        match self
            .0
            .read_path(&["module_state", "native_auth_next_nonces", identity_key])?
        {
            None => Ok(0),
            Some(raw) => serde_json::from_slice(&raw)
                .context("candidate authentication parent next nonce must be a u64 JSON integer"),
        }
    }
}

fn authenticate_parent_plan(
    plan: &NovNativeCandidateExecutionPlanV1,
    parent: &dyn ParentAuthAccess,
    params: &serde_json::Value,
) -> Result<Vec<AuthenticatedItem>> {
    verify_auth_configuration(plan.context.chain_id, params)?;
    plan.validate()
        .map_err(CandidateInputRejected::from_error)?;
    parent.verify_domain(plan.context.chain_id, plan.protocol_config_commitment)?;

    // Only cache signers touched by this batch; the immutable parent remains
    // the source for each signer's first nonce, not a cloned historical map.
    let mut expected_nonces = BTreeMap::new();
    let mut seen_hashes = HashSet::with_capacity(plan.raw_txs.len());
    let mut seen_nonce_keys = HashSet::with_capacity(plan.raw_txs.len());
    let mut authenticated = Vec::with_capacity(plan.raw_txs.len());

    for (index, (raw, expected_hash)) in plan.raw_txs.iter().zip(&plan.tx_hashes).enumerate() {
        let item = authenticate_transaction(raw, plan.context.chain_id, params, index)
            .map_err(CandidateInputRejected::from_error)?;
        let tx_hash = item.tx_hash;
        if tx_hash != *expected_hash {
            return Err(CandidateInputRejected::from_error(anyhow::anyhow!(
                "candidate authentication transaction {index} canonical hash mismatch"
            )));
        }
        if !seen_hashes.insert(tx_hash) {
            return Err(CandidateInputRejected::from_error(anyhow::anyhow!(
                "candidate authentication duplicate signed intent at transaction {index}"
            )));
        }

        // Candidates and authority execution share the pinned V2 signer domain.
        let reservation = &item.durable_auth_reservation;
        if !seen_nonce_keys.insert(reservation.ledger_key.clone()) {
            return Err(CandidateInputRejected::from_error(anyhow::anyhow!(
                "candidate authentication duplicate nonce key at transaction {index}"
            )));
        }
        if parent.contains_reservation(&reservation.ledger_key)?
            || parent.contains_receipt(&reservation.tx_hash)?
        {
            return Err(CandidateInputRejected::from_error(anyhow::anyhow!(
                "candidate authentication transaction {index} was already committed in its parent"
            )));
        }
        let expected = match expected_nonces.entry(reservation.identity_key.clone()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(parent.next_nonce(&reservation.identity_key)?)
            }
        };
        *expected = match novovm_protocol::native_nonce::advance_nonce_v1(*expected, reservation.nonce) {
            Ok(next) => next,
            Err(novovm_protocol::native_nonce::NonceSequenceErrorV1::Mismatch) => return Err(CandidateInputRejected::from_error(anyhow::anyhow!(
                "candidate authentication nonce sequence mismatch transaction={index} expected={} got={}",
                *expected,
                reservation.nonce
            ))),
            Err(novovm_protocol::native_nonce::NonceSequenceErrorV1::Exhausted) => return Err(CandidateInputRejected::from_error(anyhow::anyhow!("candidate authentication nonce sequence overflow"))),
        };

        authenticated.push(item);
    }
    Ok(authenticated)
}

/// Local configuration is not peer input. Validate it outside the typed
/// transaction rejection boundary used below, preserving the verifier's text.
fn verify_auth_configuration(chain: u64, params: &serde_json::Value) -> Result<()> {
    if let Some(requested) = requested_native_chain_id_v1(params) {
        if requested != chain {
            bail!(
                "nov native authentication rejected: chain domain mismatch requested={} signed={}",
                requested,
                chain
            );
        }
    }
    if let Ok(raw) = std::env::var(NOV_NATIVE_CHAIN_ID_ENV) {
        let configured = raw.trim().parse::<u64>().map_err(|error| {
            anyhow::anyhow!(
                "nov native authentication rejected: invalid {}: {}",
                NOV_NATIVE_CHAIN_ID_ENV,
                error
            )
        })?;
        if configured != chain {
            bail!("nov native authentication rejected: configured chain domain mismatch configured={} signed={}", configured, chain);
        }
    }
    Ok(())
}

/// Shared pure per-item checks. No parent reads, pending admission or nonce
/// reservation: both final batch authentication and selection use these rules.
fn authenticate_transaction(
    raw: &[u8],
    chain: u64,
    params: &serde_json::Value,
    index: usize,
) -> Result<AuthenticatedItem> {
    let native_tx = decode_nov_native_tx_wire_v1(raw)
        .with_context(|| format!("decode candidate transaction {index}"))?;
    if native_tx.chain_id != chain {
        bail!("candidate authentication transaction {index} chain domain mismatch");
    }
    let ir = nov_native_tx_to_adapter_tx_ir_v1(&native_tx)
        .with_context(|| format!("derive candidate transaction {index} signed intent"))?;
    let tx_hash = tx_hash_array_from_ir_v1(&ir);
    // Ingress(false, false) would still record rejection observations. Keep
    // this shared verifier on the original isolated candidate-auth boundary.
    verify_nov_native_auth_v1(params, &native_tx, &ir, tx_hash)
        .with_context(|| format!("authenticate candidate transaction {index}"))?;
    native_transfer_dispatch::require_execution_capability_v1(&native_tx, true)?;
    let (execution_subject, requested_execution_behavior, execution_request) = match &native_tx.kind
    {
        NovTxKindV1::Execute(execute) => (
            subject_meta_from_execute_tx_v1(execute),
            requested_execution_behavior_v1(
                effective_execution_policy_for_fee_asset_v1(
                    execute.execution_policy,
                    &execute.fee_policy.pay_asset,
                ),
                execute.privacy_mode,
            ),
            nov_native_tx_to_execution_request_v1(&native_tx)?
                .context("candidate authentication requires an executable native request")?,
        ),
        NovTxKindV1::Transfer(_) => {
            let request = native_transfer_dispatch::fee_request_v1(&native_tx, tx_hash)?;
            (
                fallback_execution_subject_meta_v1(&request),
                default_execution_behavior_v1(),
                request,
            )
        }
        _ => bail!("candidate transaction capability mismatch"),
    };
    let reservation = nov_native_durable_auth_reservation_v1(&native_tx, &ir, tx_hash)?;
    Ok(AuthenticatedItem {
        native_tx,
        ir,
        tx_hash,
        durable_auth_reservation: reservation,
        execution_subject,
        requested_execution_behavior,
        execution_request,
    })
}

/// Admission only, never part of historical authentication or output readback.
/// A stored Execute result remains readable with the comparison permission off;
/// a new worker job that would need Host computation must be refused up front.
fn require_new_execution_capability(native_tx: &NovNativeTxWireV1) -> Result<()> {
    native_transfer_dispatch::require_execution_capability_v1(native_tx, true)
        .map_err(CandidateInputRejected::from_error)?;
    if !matches!(native_tx.kind, NovTxKindV1::Transfer(_)) {
        require_legacy_host_execution_comparison_v1("fresh successor Host Execute")
            .map_err(CandidateInputRejected::from_error)?;
    }
    Ok(())
}

pub(crate) fn require_new_successor_execution(raw_txs: &[Vec<u8>]) -> Result<()> {
    for (index, raw) in raw_txs.iter().enumerate() {
        let tx = decode_nov_native_tx_wire_v1(raw)
            .with_context(|| format!("decode candidate transaction {index}"))
            .map_err(CandidateInputRejected::from_error)?;
        require_new_execution_capability(&tx)?;
    }
    Ok(())
}

pub(super) fn select_transactions(
    chain: u64,
    protocol: [u8; 32],
    parent: &NovNativeExecutionStoreV1,
    ordered: Vec<Vec<u8>>,
    params: &serde_json::Value,
    limit: usize,
) -> Result<Vec<Vec<u8>>> {
    select_parent_transactions(chain, protocol, parent, ordered, params, limit)
}

pub(super) fn select_record_transactions(
    chain: u64,
    protocol: [u8; 32],
    parent: &dyn NativeRecordAccessV1,
    ordered: Vec<Vec<u8>>,
    params: &serde_json::Value,
    limit: usize,
) -> Result<Vec<Vec<u8>>> {
    select_parent_transactions(
        chain,
        protocol,
        &RecordParentAuth(parent),
        ordered,
        params,
        limit,
    )
}

/// Scan the caller's existing pool order once, without altering its entries.
/// Authenticate raw bytes rather than trusting cached signer/nonce metadata.
/// An item rejection cannot consume candidate-local nonce/bytes; parent read
/// failures abort the whole selection instead of masquerading as absence.
fn select_parent_transactions(
    chain: u64,
    protocol: [u8; 32],
    parent: &dyn ParentAuthAccess,
    ordered: Vec<Vec<u8>>,
    params: &serde_json::Value,
    limit: usize,
) -> Result<Vec<Vec<u8>>> {
    verify_auth_configuration(chain, params)?;
    if !(1..=NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1).contains(&limit) {
        bail!("proposal_max_transactions is outside its bounds");
    }
    if chain == 0 || protocol == [0; 32] {
        bail!("transaction selection requires a verified parent chain/protocol domain");
    }
    parent.verify_domain(chain, protocol)?;
    // These maps are lookup-only: randomized bucket order never changes the
    // externally supplied scan order or the resulting transaction sequence.
    let mut expected_nonces = HashMap::new();
    let mut seen_hashes = HashSet::new();
    let mut seen_nonce_keys = HashSet::new();
    let mut selected = Vec::with_capacity(limit.min(ordered.len()));
    let mut body_bytes = 0usize;
    for (index, raw) in ordered.into_iter().enumerate() {
        if selected.len() == limit {
            break;
        }
        let Some(next_bytes) = body_bytes.checked_add(raw.len()) else {
            continue;
        };
        if raw.is_empty() || next_bytes > NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1 {
            continue;
        }
        // As before, a failed per-item authentication is not selected. Do not
        // label all such failures "bad signatures": auth has other policies.
        // Final preparation still independently authenticates the whole batch.
        let Ok(item) = authenticate_transaction(&raw, chain, params, index) else {
            continue;
        };
        if require_new_execution_capability(&item.native_tx).is_err() {
            continue;
        }
        let reservation = &item.durable_auth_reservation;
        if seen_hashes.contains(&item.tx_hash) || seen_nonce_keys.contains(&reservation.ledger_key)
        {
            continue;
        }
        if parent.contains_reservation(&reservation.ledger_key)?
            || parent.contains_receipt(&reservation.tx_hash)?
        {
            continue;
        }
        let expected = match expected_nonces.entry(reservation.identity_key.clone()) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(parent.next_nonce(&reservation.identity_key)?)
            }
        };
        let Ok(next_nonce) =
            novovm_protocol::native_nonce::advance_nonce_v1(*expected, reservation.nonce)
        else {
            continue;
        };
        *expected = next_nonce;
        seen_hashes.insert(item.tx_hash);
        seen_nonce_keys.insert(reservation.ledger_key.clone());
        body_bytes = next_bytes;
        selected.push(raw);
    }
    Ok(selected)
}

#[cfg(test)]
mod record_tests {
    use super::*;
    use std::cell::RefCell;

    const CHAIN: u64 = 98_919_749;
    const PROTOCOL: [u8; 32] = [0x45; 32];

    fn path(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    struct Reader {
        values: BTreeMap<Vec<String>, Vec<u8>>,
        reads: RefCell<Vec<Vec<String>>>,
        failed_path: Option<Vec<String>>,
    }

    impl Reader {
        fn new(store: &NovNativeExecutionStoreV1) -> Self {
            let values = native_store_records::encode(store)
                .unwrap()
                .into_iter()
                .map(|(key, value)| {
                    let (path, raw) = native_store_records::unpack(&key, &value).unwrap();
                    let raw = if raw == native_store_records::OBJECT {
                        b"{}".to_vec()
                    } else {
                        raw.to_vec()
                    };
                    (path, raw)
                })
                .collect();
            Self {
                values,
                reads: RefCell::new(Vec::new()),
                failed_path: None,
            }
        }
    }

    impl NativeRecordAccessV1 for Reader {
        fn read_path(&self, parts: &[&str]) -> Result<Option<Vec<u8>>> {
            let path = path(parts);
            self.reads.borrow_mut().push(path.clone());
            if self.failed_path.as_ref() == Some(&path) {
                bail!("injected missing or corrupt parent blob");
            }
            Ok(self.values.get(&path).cloned())
        }
    }

    fn store() -> NovNativeExecutionStoreV1 {
        let mut store = NovNativeExecutionStoreV1 {
            authority_chain_id: Some(CHAIN),
            ..Default::default()
        };
        store.module_state.protocol_config_commitment = to_hex(&PROTOCOL);
        store
    }

    fn transaction(nonce: u64, seed: [u8; 32], amount: u128) -> NovNativeTxWireV1 {
        let mut tx = NovNativeTxWireV1 {
            chain_id: CHAIN,
            kind: NovTxKindV1::Transfer(novovm_protocol::NovTransferTxV1 {
                from: Vec::new(),
                to: novovm_adapter_novovm::address_from_seed_v1([0x63; 32]),
                asset: "NOV".into(),
                amount,
                nonce,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "NOV".into(),
                    max_pay_amount: 1000,
                    slippage_bps: 0,
                },
            }),
            signature: Vec::new(),
        };
        sign_nov_native_tx_with_seed_v1(&mut tx, seed).unwrap();
        tx
    }

    fn reservation(tx: &NovNativeTxWireV1) -> NovNativeDurableAuthReservationV1 {
        let ir = nov_native_tx_to_adapter_tx_ir_v1(tx).unwrap();
        nov_native_durable_auth_reservation_v1(tx, &ir, tx_hash_array_from_ir_v1(&ir)).unwrap()
    }

    fn plan(txs: &[NovNativeTxWireV1]) -> NovNativeCandidateExecutionPlanV1 {
        let raw: Vec<_> = txs
            .iter()
            .map(|tx| novovm_protocol::encode_nov_native_tx_wire_v1(tx).unwrap())
            .collect();
        let hashes = raw
            .iter()
            .map(|raw| canonical_nov_native_tx_hash_from_payload_v1(raw).unwrap())
            .collect();
        NovNativeCandidateExecutionPlanV1::new(
            novovm_protocol::NovBlockExecutionContextV1 {
                chain_id: CHAIN,
                block_height: 1,
                parent_block_hash: [0; 32],
                slot: 1,
                timestamp_unix_ms: 1_900_000_000_000,
            },
            PROTOCOL,
            [0x46; 32],
            None,
            hashes,
            raw,
        )
        .unwrap()
    }

    #[test]
    fn record_auth_matches_typed_batch_without_reading_unrelated_history() {
        let first = transaction(7, [0x61; 32], u128::MAX);
        let first_reservation = reservation(&first);
        let txs = [
            first,
            transaction(0, [0x62; 32], 1),
            transaction(8, [0x61; 32], 2),
        ];
        let plan = plan(&txs);
        let mut parent = store();
        parent
            .module_state
            .native_auth_next_nonces
            .insert(first_reservation.identity_key.clone(), 7);
        for id in 0..1024 {
            parent
                .module_state
                .native_auth_next_nonces
                .insert(format!("unrelated-{id}"), id);
            parent
                .module_state
                .native_auth_nonce_reservations
                .insert(format!("old-{id}"), "old-reservation".into());
        }
        let reader = Reader::new(&parent);
        let params = serde_json::json!({"chain_id": CHAIN});
        let typed = authenticate_plan(&plan, &parent, &params).unwrap();
        let records = authenticate_record_plan(&plan, &reader, &params).unwrap();
        assert_eq!(typed.len(), records.len());
        for (typed, record) in typed.iter().zip(&records) {
            assert_eq!(typed.tx_hash, record.tx_hash);
            assert_eq!(
                typed.durable_auth_reservation,
                record.durable_auth_reservation
            );
            assert_eq!(
                novovm_protocol::encode_nov_native_tx_wire_v1(&typed.native_tx).unwrap(),
                novovm_protocol::encode_nov_native_tx_wire_v1(&record.native_tx).unwrap()
            );
        }
        let reads = reader.reads.borrow();
        assert!(!reads
            .iter()
            .flatten()
            .any(|part| part.starts_with("unrelated-") || part.starts_with("old-")));
        let signer_nonce_path = path(&[
            "module_state",
            "native_auth_next_nonces",
            &first_reservation.identity_key,
        ]);
        assert_eq!(
            reads
                .iter()
                .filter(|item| *item == &signer_nonce_path)
                .count(),
            1
        );
        // Eight fixed domain/structure reads, one nonce per signer, and two
        // existence reads per transaction. No full map fetch or scan occurs.
        assert_eq!(reads.len(), 8 + 2 + 2 * txs.len());
        assert_eq!(
            parent.module_state.native_auth_next_nonces[&first_reservation.identity_key],
            7
        );
    }

    #[test]
    fn record_auth_missing_invalid_or_failed_required_reads_never_default() {
        let tx = transaction(0, [0x61; 32], 1);
        let reservation = reservation(&tx);
        let plan = plan(&[tx]);
        let parent = store();
        let params = serde_json::json!({"chain_id": CHAIN});
        assert!(authenticate_record_plan(&plan, &Reader::new(&parent), &params).is_ok());
        for parts in [
            &[][..],
            &["module_state"][..],
            &["module_state", "native_auth_next_nonces"][..],
            &["module_state", "native_auth_nonce_reservations"][..],
            &["receipts"][..],
            &["authority_chain_id"][..],
            &["module_state", "native_auth_nonce_identity_scheme"][..],
            &["module_state", "protocol_config_commitment"][..],
        ] {
            let mut reader = Reader::new(&parent);
            reader.values.remove(&path(parts));
            let error = authenticate_record_plan(&plan, &reader, &params)
                .err()
                .unwrap();
            assert!(
                !error.is::<CandidateInputRejected>(),
                "parent failure classified as peer input: {parts:?}: {error:#}"
            );
        }
        let nonce_path = path(&[
            "module_state",
            "native_auth_next_nonces",
            &reservation.identity_key,
        ]);
        for invalid in ["null", "\"0\"", "-1", "0.5", "18446744073709551616", "{}"] {
            let mut reader = Reader::new(&parent);
            reader
                .values
                .insert(nonce_path.clone(), invalid.as_bytes().to_vec());
            let error = authenticate_record_plan(&plan, &reader, &params)
                .err()
                .unwrap();
            assert!(
                !error.is::<CandidateInputRejected>(),
                "parent failure classified as peer input: {invalid}: {error:#}"
            );
        }
        for (parts, value) in [
            (path(&["authority_chain_id"]), b"null".to_vec()),
            (path(&["authority_chain_id"]), b"1".to_vec()),
            (
                path(&["module_state", "native_auth_nonce_identity_scheme"]),
                b"\"legacy\"".to_vec(),
            ),
            (
                path(&["module_state", "protocol_config_commitment"]),
                b"\"wrong\"".to_vec(),
            ),
            (path(&["receipts"]), b"null".to_vec()),
        ] {
            let mut reader = Reader::new(&parent);
            reader.values.insert(parts, value);
            let error = authenticate_record_plan(&plan, &reader, &params)
                .err()
                .unwrap();
            assert!(
                !error.is::<CandidateInputRejected>(),
                "parent corruption classified as peer input: {error:#}"
            );
        }
        let mut reader = Reader::new(&parent);
        reader.failed_path = Some(nonce_path);
        let error = authenticate_record_plan(&plan, &reader, &params)
            .err()
            .unwrap();
        assert!(error
            .to_string()
            .contains("injected missing or corrupt parent blob"));
        assert!(!error.is::<CandidateInputRejected>());
        let error = authenticate_record_plan(
            &plan,
            &Reader::new(&parent),
            &serde_json::json!({"chain_id": CHAIN + 1}),
        )
        .err()
        .unwrap();
        assert!(
            !error.is::<CandidateInputRejected>(),
            "local chain configuration classified as peer input: {error:#}"
        );
    }

    #[test]
    fn record_auth_rejects_parent_replays_and_whole_batch_signature_nonce_failures() {
        let first = transaction(0, [0x61; 32], 1);
        let reservation = reservation(&first);
        let parent = store();
        let params = serde_json::json!({"chain_id": CHAIN});
        for parts in [
            path(&[
                "module_state",
                "native_auth_nonce_reservations",
                &reservation.ledger_key,
            ]),
            path(&["receipts", &reservation.tx_hash]),
        ] {
            let mut reader = Reader::new(&parent);
            reader.values.insert(parts, b"null".to_vec());
            let error =
                authenticate_record_plan(&plan(std::slice::from_ref(&first)), &reader, &params)
                    .err()
                    .unwrap();
            assert!(error.is::<CandidateInputRejected>(), "{error:#}");
        }
        let mut bad_signature = transaction(0, [0x62; 32], 2);
        bad_signature.signature[40] ^= 1;
        let mut wrong_chain = transaction(0, [0x62; 32], 2);
        wrong_chain.chain_id += 1;
        sign_nov_native_tx_with_seed_v1(&mut wrong_chain, [0x62; 32]).unwrap();
        for invalid in [
            bad_signature,
            wrong_chain,
            transaction(0, [0x61; 32], 2),
            transaction(2, [0x61; 32], 2),
            transaction(u64::MAX, [0x62; 32], 2),
        ] {
            let plan = plan(&[first.clone(), invalid]);
            let reader = Reader::new(&parent);
            let before = reader.values.clone();
            let error = authenticate_plan(&plan, &parent, &params).err().unwrap();
            assert!(error.is::<CandidateInputRejected>(), "{error:#}");
            let error = authenticate_record_plan(&plan, &reader, &params)
                .err()
                .unwrap();
            assert!(error.is::<CandidateInputRejected>(), "{error:#}");
            assert_eq!(reader.values, before);
        }
        // A failed late item cannot reserve an earlier valid nonce in the parent.
        assert!(authenticate_record_plan(&plan(&[first]), &Reader::new(&parent), &params).is_ok());
    }

    include!("native_candidate_selection_tests.rs");
}
