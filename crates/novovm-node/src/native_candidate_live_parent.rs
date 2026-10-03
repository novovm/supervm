//! Point-in-time finalized parent views. Root references are accepted only
//! after existing ledger finality and the single live AOEM publication agree.
//! This is not a signing/publication capability and never creates authority.

use super::*;
use crate::native_root_codecs::NativeRootCodecProfileV1;
use crate::native_state_records::RecordOverlayV1;
use crate::native_state_storage::AoemStateReaderV1;
use native_store_records::NativeRecordAccessV1;
use native_transfer_record_execution::{ExecutionReader, RootedAccess};
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;

enum DirectPredecessor {
    Uncaptured,
    Genesis,
    Successor {
        workspace_id: [u8; 32],
        block_hash: [u8; 32],
    },
}

enum ParentView {
    Cold(Box<FinalizedGenesisParentV1>),
    Rooted {
        snapshot: Box<rooted_parent::RootedParentSnapshot>,
        reference: Box<state_records::StoreRef>,
        // Populated only by live capture after verifying the retained direct
        // predecessor archive/source under the same authority lock.
        direct_predecessor: DirectPredecessor,
    },
}

/// No public constructor or Deserialize: caller-supplied roots are not parents.
pub(crate) struct FinalizedParentViewV1(ParentView);

/// Restricted reads from one already captured finalized parent. This is not a
/// live authority or an arbitrary state-tree interface. Callers provide the
/// same nonce identity key used by native authentication, not an account alias.
pub(crate) trait FinalizedRecordReaderV1 {
    fn next_nonce(&self, identity: &str) -> Result<u64>;
    fn nov_balance(&self, account: &str) -> Result<Option<u128>>;
    fn receipt(&self, hash: &[u8; 32]) -> Result<Option<NovNativeExecutionReceiptV1>>;
    fn contains_receipt(&self, hash: &[u8; 32]) -> Result<bool> {
        Ok(self.receipt(hash)?.is_some())
    }
}

struct ColdFinalizedRecords<'a>(&'a NovNativeExecutionStoreV1);

impl FinalizedRecordReaderV1 for ColdFinalizedRecords<'_> {
    fn nov_balance(&self, account: &str) -> Result<Option<u128>> {
        Ok(self
            .0
            .module_state
            .account_asset_balances
            .get(account)
            .and_then(|assets| assets.get("NOV"))
            .copied())
    }

    fn next_nonce(&self, identity: &str) -> Result<u64> {
        Ok(self
            .0
            .module_state
            .native_auth_next_nonces
            .get(identity)
            .copied()
            .unwrap_or(0))
    }

    fn receipt(&self, hash: &[u8; 32]) -> Result<Option<NovNativeExecutionReceiptV1>> {
        let key = to_hex(hash);
        let receipt = self.0.receipts.get(&key);
        if receipt.is_some_and(|receipt| receipt.tx_hash != key) {
            bail!("finalized receipt map key differs from its transaction hash");
        }
        Ok(receipt.cloned())
    }
}

struct RootedFinalizedRecords<'a>(&'a dyn NativeRecordAccessV1);

impl<'a> RootedFinalizedRecords<'a> {
    fn new(access: &'a dyn NativeRecordAccessV1) -> Result<Self> {
        // A missing map is corruption, never evidence that every key is absent.
        // RootedAccess also checks each consensus-bearing marker against its
        // independently committed tree before the callback can query anything.
        for path in [
            &[][..],
            &["module_state"][..],
            &["module_state", "native_auth_next_nonces"][..],
            &["module_state", "account_asset_balances"][..],
            &["receipts"][..],
        ] {
            if access.read_path(path)?.as_deref() != Some(b"{}") {
                bail!("finalized query required object marker missing or invalid: {path:?}");
            }
        }
        Ok(Self(access))
    }
}

impl FinalizedRecordReaderV1 for RootedFinalizedRecords<'_> {
    fn nov_balance(&self, account: &str) -> Result<Option<u128>> {
        let parent = self
            .0
            .read_path(&["module_state", "account_asset_balances", account])?;
        let raw = self
            .0
            .read_path(&["module_state", "account_asset_balances", account, "NOV"])?;
        match parent.as_deref() {
            Some(b"{}") => (),
            None if raw.is_none() => return Ok(None),
            _ => bail!("finalized balance account object missing or invalid"),
        }
        raw.map(|bytes| {
            serde_json::from_slice::<u128>(&bytes)
                .context("finalized NOV balance must be a u128 JSON integer")
        })
        .transpose()
    }

    fn next_nonce(&self, identity: &str) -> Result<u64> {
        match self
            .0
            .read_path(&["module_state", "native_auth_next_nonces", identity])?
        {
            None => Ok(0),
            Some(raw) => serde_json::from_slice(&raw)
                .context("finalized next nonce must be a u64 JSON integer"),
        }
    }

    fn receipt(&self, hash: &[u8; 32]) -> Result<Option<NovNativeExecutionReceiptV1>> {
        let key = to_hex(hash);
        let Some(raw) = self.0.read_path(&["receipts", &key])? else {
            return Ok(None);
        };
        // RootedAccess has compared the exact typed receipt commitment with
        // the receipt tree. Parse directly into u128 fields, never via Value.
        let receipt: NovNativeExecutionReceiptV1 =
            serde_json::from_slice(&raw).context("invalid finalized typed receipt record")?;
        if receipt.tx_hash != key {
            bail!("finalized receipt record differs from requested transaction hash");
        }
        Ok(Some(receipt))
    }
}

/// Isolated tests may exercise the restricted reader with genuine tree
/// fixtures. This does not construct a verified parent or authorize a root.
#[cfg(test)]
pub(super) fn with_rooted_records_for_test_v1<T>(
    access: &dyn NativeRecordAccessV1,
    f: impl FnOnce(&dyn FinalizedRecordReaderV1) -> Result<T>,
) -> Result<T> {
    f(&RootedFinalizedRecords::new(access)?)
}

pub(crate) fn load_finalized_parent_view_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FinalizedParentViewV1> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    capture_finalized_parent_view_locked(&mut workspace, id, genesis, params)
}

/// Caller owns the workspace OS lock; authority is acquired before any ledger
/// read. Ledger getters must not run inside a non-reentrant ledger callback.
pub(super) fn capture_finalized_parent_view_locked(
    workspace: &mut WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FinalizedParentViewV1> {
    let (view, authority) = capture_rooted_live_parent_locked(workspace, id, genesis, params)?;
    if let Some(view) = view {
        return Ok(view);
    }
    // The legacy reader reacquires authority and repeats its full verification.
    // No action is authorized during this compatibility handover.
    drop(authority);
    Ok(FinalizedParentViewV1::from_verified_cold(
        execution::capture_finalized_parent_locked(workspace, id, genesis, params)?,
    ))
}

/// Live callback scope, unlike the freely retainable historical view. The
/// caller already owns the workspace lock; this function retains authority
/// continuously from verification through completion (or failure) of action.
/// False means only an identified legacy format, never invalid evidence. The
/// caller must then run its original cold, authority-locked callback path.
pub(super) fn try_with_rooted_finalized_parent_view_locked(
    workspace: &WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    action: &mut dyn FnMut(FinalizedParentViewV1) -> Result<()>,
) -> Result<bool> {
    let (view, authority) = capture_rooted_live_parent_locked(workspace, id, genesis, params)?;
    let Some(view) = view else {
        return Ok(false);
    };
    // Ledger getters used during capture have released their non-reentrant
    // mutex. The callback may acquire that mutex, but not workspace/authority.
    let result = action(view);
    drop(authority);
    result.map(|()| true)
}

fn capture_rooted_live_parent_locked(
    workspace: &WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<(
    Option<FinalizedParentViewV1>,
    NovNativeExecutionStoreWriteLockV1,
)> {
    let path = resolve_native_execution_store_path_from_params_v1(params)
        .context("live parent capture requires explicit native path")?;
    let authority = acquire_nov_native_execution_store_write_lock_v1(&path)?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "live parent namespace")?;
    let ledger_path = nov_native_block_ledger_rocksdb_path_v1(&path);
    let (archive, previous) = NovNativeBlockLedgerV1::load_fresh_finalized_tip_archive_v1(
        &ledger_path,
        genesis,
        namespace,
        id,
    )?;
    if archive.block.header.chain_id != workspace.chain_id || archive.execution.workspace_id != id {
        bail!("live parent tip differs from requested authority domain or workspace");
    }
    let target = execution::publication_target_fields(
        if archive.block.header.height == 1 {
            b"NVP1"
        } else {
            b"NVP2"
        },
        namespace,
        genesis,
        archive.commitment,
        id,
        archive.execution.output_digest,
        &archive.block,
    );
    verify_live_head(workspace, id, &target)?;
    match (&previous, archive.block.header.height) {
        (None, 1) => {}
        (Some(previous), height)
            if previous.block.header.height.checked_add(1) == Some(height)
                && previous.block.header.chain_id == archive.block.header.chain_id
                && previous.block.header.block_hash == archive.block.header.parent_block_hash => {}
        _ => bail!("live parent archive has inconsistent immediate predecessor"),
    }
    let view = capture_rooted_archive(workspace, &archive, params)?;
    let previous_is_rooted = match &previous {
        Some(previous) => capture_rooted_archive(workspace, previous, params)?.is_some(),
        None => true,
    };
    if let Some(mut view) = view.filter(|_| previous_is_rooted) {
        if let ParentView::Rooted {
            direct_predecessor, ..
        } = &mut view.0
        {
            *direct_predecessor = match &previous {
                Some(previous) => DirectPredecessor::Successor {
                    workspace_id: previous.execution.workspace_id,
                    block_hash: previous.block.header.block_hash,
                },
                None => DirectPredecessor::Genesis,
            };
        }
        // Readback while authority is still locked; no repair on missing evidence.
        verify_live_head(workspace, id, &target)?;
        return Ok((Some(view), authority));
    }
    // Only identified compatibility formats reach the original cold reader.
    // It also verifies the retained direct predecessor. A corrupt rooted output
    // never enters this branch; capture_rooted_archive returns an error for it.
    Ok((None, authority))
}

/// Verify one retained finalized output, not live authority. The archive must
/// come from a verified ledger getter. Callers separately enforce their live
/// head/pending-intent rules while holding authority around any action.
/// Do not recurse into earlier workspaces: those may already be retired.
pub(super) fn capture_rooted_archive(
    workspace: &WorkspaceStore,
    archive: &crate::native_block_ledger::FinalizedRecordArchiveV1,
    params: &serde_json::Value,
) -> Result<Option<FinalizedParentViewV1>> {
    let id = archive.execution.workspace_id;
    let bytes = execution::read_completed_output_bytes(workspace, id)?;
    if execution::output_digest(&bytes) != archive.execution.output_digest {
        bail!("live parent source output differs from finalized execution digest");
    }
    #[derive(Deserialize)]
    struct Version {
        schema: String,
    }
    let version: Version = serde_json::from_slice(&bytes)?;
    let cold = match version.schema.as_str() {
        "novovm-candidate-record-document/v1" | execution::OUTPUT_SCHEMA => true,
        "novovm-candidate-record-document/v2" | "novovm-candidate-record-document/v3" => false,
        _ => bail!("unsupported live parent output document schema"),
    };
    if cold || archive.config.root_codec_profile()? == NativeRootCodecProfileV1::LegacyWireV1 {
        return Ok(None);
    }
    let source_output = RawValue::from_string(String::from_utf8(bytes.clone())?)?;
    if source_output.get().as_bytes() != bytes {
        bail!("live parent capture changed the published output bytes");
    }
    let snapshot = rooted_parent::RootedParentSnapshot {
        config: archive.config.clone(),
        block: archive.block.clone(),
        proof: archive.proof.clone(),
        binding: archive.execution.clone(),
        promotion_commitment: archive.commitment,
        source_output,
        store: (),
    };
    let reference = snapshot.verify_source_with_archive(workspace, params, archive)?;
    snapshot.validate_live_input(workspace)?;
    let view = FinalizedParentViewV1(ParentView::Rooted {
        snapshot: Box::new(snapshot),
        reference: Box::new(reference),
        direct_predecessor: DirectPredecessor::Uncaptured,
    });
    view.with_record_access(workspace, |access| {
        verify_record_domain(access, workspace, view.block().header.state_version)
    })?;
    Ok(Some(view))
}

fn verify_live_head(workspace: &WorkspaceStore, id: [u8; 32], target: &[u8]) -> Result<()> {
    if workspace
        .graph
        .get(&native_aoem_owned_state_head_key_v1(
            workspace.chain_id,
            &workspace.namespace,
        ))?
        .as_deref()
        != Some(target)
        || workspace.graph.get(&workspace.key(b'h', &id))?.as_deref() != Some(target)
    {
        bail!("live finalized parent authority head/publication evidence mismatch");
    }
    Ok(())
}

fn required<T: DeserializeOwned>(access: &dyn NativeRecordAccessV1, path: &[&str]) -> Result<T> {
    let raw = access
        .read_path(path)?
        .with_context(|| format!("live parent required record missing: {path:?}"))?;
    serde_json::from_slice(&raw)
        .with_context(|| format!("live parent record type mismatch: {path:?}"))
}

fn verify_record_domain(
    access: &dyn NativeRecordAccessV1,
    workspace: &WorkspaceStore,
    version: u64,
) -> Result<()> {
    if required::<Option<u64>>(access, &["authority_chain_id"])? != Some(workspace.chain_id)
        || required::<String>(access, &["authority_namespace_digest"])? != workspace.namespace
        || required::<String>(access, &["module_state", "protocol_config_commitment"])?
            != to_hex(&workspace.protocol)
        || verify_required_native_business_protocol_config_pin_v1()? != to_hex(&workspace.protocol)
        || required::<String>(
            access,
            &["module_state", "native_auth_nonce_identity_scheme"],
        )? != NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2
        || required::<u64>(access, &["module_state", "aoem_semantic_ledger_sequence"])? != version
    {
        bail!("live parent record authority/protocol/nonce/sequence binding mismatch");
    }
    Ok(())
}

impl FinalizedParentViewV1 {
    /// Only existing fully verified cold capture may construct this variant.
    /// Its caller remains responsible for retaining authority around actions.
    pub(super) fn from_verified_cold(parent: FinalizedGenesisParentV1) -> Self {
        Self(ParentView::Cold(Box::new(parent)))
    }

    /// Read nonce/receipt records from this immutable historical image. The
    /// rooted branch opens one workspace/provider and shares one bounded node
    /// and blob reader for the whole callback. It does not recheck the live head
    /// or grant permission to admit, sign or publish against this saved image.
    /// The callback must not re-enter workspace APIs while this lock is held.
    pub(crate) fn with_records<T>(
        &self,
        params: &serde_json::Value,
        f: impl FnOnce(&dyn FinalizedRecordReaderV1) -> Result<T>,
    ) -> Result<T> {
        let chain = self.block().header.chain_id;
        if let ParentView::Cold(parent) = &self.0 {
            let store = parent.state();
            verify_native_nonce_identity_scheme_v2(store)?;
            verify_production_native_execution_store_authority_domain_v2(
                store,
                chain,
                &native_aoem_owned_state_namespace_digest_v1(params, chain),
            )?;
            verify_native_business_protocol_config_v1(store)?;
            if store.module_state.aoem_semantic_ledger_sequence != self.block().header.state_version
                || store.module_state.protocol_config_commitment
                    != to_hex(&self.genesis_config().protocol_config_commitment)
            {
                bail!("finalized query cold state version/protocol mismatch");
            }
            return f(&ColdFinalizedRecords(store));
        }
        let workspace = WorkspaceStore::open(chain, params)?;
        if workspace.protocol != self.genesis_config().protocol_config_commitment {
            bail!("finalized query workspace protocol differs from captured parent");
        }
        self.with_record_access(&workspace, |access| {
            verify_record_domain(access, &workspace, self.block().header.state_version)?;
            f(&RootedFinalizedRecords::new(access)?)
        })
    }

    pub(crate) fn block(&self) -> &NovNativeDurableBlockV1 {
        match &self.0 {
            ParentView::Cold(p) => p.block(),
            ParentView::Rooted { snapshot, .. } => &snapshot.block,
        }
    }
    pub(crate) fn finality_proof(
        &self,
    ) -> &crate::native_block_ledger::NovNativeFreshFinalityProofV1 {
        match &self.0 {
            ParentView::Cold(p) => p.finality_proof(),
            ParentView::Rooted { snapshot, .. } => &snapshot.proof,
        }
    }
    pub(crate) fn genesis_config(&self) -> &fresh_genesis::FreshGenesisConfigV1 {
        match &self.0 {
            ParentView::Cold(p) => p.genesis_config(),
            ParentView::Rooted { snapshot, .. } => &snapshot.config,
        }
    }
    pub(crate) fn workspace_id(&self) -> [u8; 32] {
        match &self.0 {
            ParentView::Cold(p) => p.workspace_id(),
            ParentView::Rooted { snapshot, .. } => snapshot.binding.workspace_id,
        }
    }
    pub(crate) fn output_digest(&self) -> [u8; 32] {
        match &self.0 {
            ParentView::Cold(p) => p.output_digest(),
            ParentView::Rooted { snapshot, .. } => snapshot.binding.output_digest,
        }
    }
    /// Compare historical lineage captured with this live parent, not a live
    /// permission. Only Cold returns false for the original compatibility check.
    /// Missing rooted lineage or a configured predecessor of genesis is an error.
    pub(crate) fn verify_direct_predecessor(&self, predecessor: [u8; 32]) -> Result<bool> {
        match &self.0 {
            ParentView::Cold(_) => Ok(false),
            ParentView::Rooted {
                direct_predecessor, ..
            } => match direct_predecessor {
                DirectPredecessor::Uncaptured => {
                    bail!("finalized predecessor lineage was not captured")
                }
                DirectPredecessor::Genesis => {
                    bail!("configured finalized predecessor is invalid for genesis")
                }
                DirectPredecessor::Successor {
                    workspace_id,
                    block_hash,
                } => {
                    if predecessor != *workspace_id
                        || *block_hash != self.block().header.parent_block_hash
                    {
                        bail!("successor service predecessor differs from finalized ancestry");
                    }
                    Ok(true)
                }
            },
        }
    }
    pub(super) fn record_state(&self) -> Option<&state_records::StoreRef> {
        match &self.0 {
            ParentView::Cold(p) => p.record_state.as_ref(),
            ParentView::Rooted { reference, .. } => Some(reference),
        }
    }
    pub(super) fn light_payload(
        &self,
        plan: &NovNativeCandidateExecutionPlanV1,
    ) -> Result<Option<LightPayload>> {
        plan.validate()?;
        Ok(match &self.0 {
            ParentView::Cold(_) => None,
            ParentView::Rooted {
                snapshot,
                reference,
                ..
            } => Some(LightPayload {
                schema: LIGHT_SCHEMA.into(),
                plan: plan.clone(),
                finalized_parent: (**snapshot).clone(),
                record_state: Some((**reference).clone()),
            }),
        })
    }
    pub(super) fn cold_snapshot(
        &self,
        workspace: &WorkspaceStore,
        params: &serde_json::Value,
    ) -> Result<FinalizedParentSnapshot> {
        match &self.0 {
            ParentView::Cold(p) => Ok(FinalizedParentSnapshot::capture(p)),
            ParentView::Rooted { snapshot, .. } => snapshot.materialize_cold(workspace, params),
        }
    }
    fn with_record_access<T>(
        &self,
        workspace: &WorkspaceStore,
        f: impl FnOnce(&dyn NativeRecordAccessV1) -> Result<T>,
    ) -> Result<T> {
        let (physical, state, receipts, _, _) = self
            .record_state()
            .context("live parent record reference missing")?
            .rooted_parts()?
            .context("live parent three-root bundle missing")?;
        let base = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
        let reader = ExecutionReader::new(&base);
        f(&RootedAccess {
            physical: &RecordOverlayV1::new(&reader, physical),
            state: &RecordOverlayV1::new(&reader, state),
            receipts: &RecordOverlayV1::new(&reader, receipts),
        })
    }
    /// Select from the pool's established order in one immutable-parent scope.
    /// Raw entries are authenticated individually; this neither mutates the
    /// pool nor replaces the final batch/live checks performed by preparation.
    pub(crate) fn select_ordered_transactions(
        &self,
        ordered: Vec<Vec<u8>>,
        limit: usize,
        params: &serde_json::Value,
    ) -> Result<Vec<Vec<u8>>> {
        let h = &self.block().header;
        let workspace = WorkspaceStore::open(h.chain_id, params)?;
        let protocol = self.genesis_config().protocol_config_commitment;
        if workspace.protocol != protocol
            || workspace.namespace
                != native_aoem_owned_state_namespace_digest_v1(params, h.chain_id)
        {
            bail!("historical parent selection workspace/protocol/namespace mismatch");
        }
        if let ParentView::Cold(parent) = &self.0 {
            return auth::select_transactions(
                h.chain_id,
                protocol,
                parent.state(),
                ordered,
                params,
                limit,
            );
        }
        self.with_record_access(&workspace, |access| {
            verify_record_domain(access, &workspace, h.state_version)?;
            auth::select_record_transactions(h.chain_id, protocol, access, ordered, params, limit)
        })
    }
    /// Authenticate an immutable historical parent. This neither reads live
    /// authority nor grants permission to register/sign/publish a successor.
    pub(crate) fn successor_plan(
        &self,
        context: novovm_protocol::NovBlockExecutionContextV1,
        raw_txs: Vec<Vec<u8>>,
        params: &serde_json::Value,
    ) -> Result<NovNativeCandidateExecutionPlanV1> {
        let workspace = WorkspaceStore::open(self.block().header.chain_id, params)?;
        self.successor_plan_locked(&workspace, context, raw_txs, params)
    }
    pub(super) fn successor_plan_locked(
        &self,
        workspace: &WorkspaceStore,
        context: novovm_protocol::NovBlockExecutionContextV1,
        raw_txs: Vec<Vec<u8>>,
        params: &serde_json::Value,
    ) -> Result<NovNativeCandidateExecutionPlanV1> {
        let h = &self.block().header;
        if workspace.chain_id != h.chain_id
            || workspace.protocol != self.genesis_config().protocol_config_commitment
            || workspace.namespace
                != native_aoem_owned_state_namespace_digest_v1(params, h.chain_id)
        {
            bail!("historical parent plan workspace/protocol/namespace mismatch");
        }
        if let ParentView::Cold(parent) = &self.0 {
            return parent.successor_plan(context, raw_txs, params);
        }
        if raw_txs.is_empty()
            || raw_txs.len() > crate::native_block_ledger::NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1
            || raw_txs
                .iter()
                .try_fold(0usize, |total, raw| total.checked_add(raw.len()))
                .is_none_or(|total| {
                    total > crate::native_block_ledger::NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1
                })
        {
            bail!("successor transaction batch exceeds block bounds");
        }
        if context.chain_id != workspace.chain_id
            || h.height.checked_add(1) != Some(context.block_height)
            || context.parent_block_hash != h.block_hash
            || context.slot <= h.slot
            || context.timestamp_unix_ms < h.timestamp_unix_ms
        {
            bail!("successor context does not extend the verified finalized parent");
        }
        let hashes = raw_txs
            .iter()
            .map(|raw| canonical_nov_native_tx_hash_from_payload_v1(raw))
            .collect::<Result<Vec<_>>>()?;
        let plan = NovNativeCandidateExecutionPlanV1::new(
            context,
            self.genesis_config().protocol_config_commitment,
            h.post_state_root,
            Some(NovNativePreparedAoemParentV1 {
                batch_id: h.aoem_batch_id.clone(),
                batch_result_id: h.aoem_batch_result_id.clone(),
                state_root: h.post_state_root,
                state_root_codec: h.post_state_root_codec.clone(),
                cumulative_receipt_root: h.cumulative_receipt_root,
                receipt_root_codec: h.cumulative_receipt_root_codec.clone(),
                state_version: h.state_version,
            }),
            hashes,
            raw_txs,
        )?;
        self.with_record_access(workspace, |access| {
            verify_record_domain(access, workspace, h.state_version)?;
            auth::authenticate_record_plan(&plan, access, params)?;
            Ok(())
        })?;
        Ok(plan)
    }
    /// Historical subject only; signing/publication retain their live lock scopes.
    pub(crate) fn successor_seal_subject(
        &self,
        candidate: &IsolatedBlockArtifactV1,
        round: u64,
    ) -> Result<crate::native_block_seal::NovNativeSealSubjectV1> {
        if let ParentView::Cold(parent) = &self.0 {
            return parent.successor_seal_subject(candidate, round);
        }
        let block = candidate.block();
        let h = &block.header;
        let p = &self.block().header;
        let compiled = self.genesis_config().compile()?;
        let expected = NovNativePreparedAoemParentV1 {
            batch_id: p.aoem_batch_id.clone(),
            batch_result_id: p.aoem_batch_result_id.clone(),
            state_root: p.post_state_root,
            state_root_codec: p.post_state_root_codec.clone(),
            cumulative_receipt_root: p.cumulative_receipt_root,
            receipt_root_codec: p.cumulative_receipt_root_codec.clone(),
            state_version: p.state_version,
        };
        if candidate.fresh_genesis_identity() != Some(&compiled.identity())
            || h.chain_id != p.chain_id
            || p.height.checked_add(1) != Some(h.height)
            || h.parent_block_hash != p.block_hash
            || h.pre_state_root != p.post_state_root
            || h.aoem_parent.as_ref() != Some(&expected)
            || p.state_version.checked_add(u64::from(h.tx_count)) != Some(h.state_version)
            || h.slot <= p.slot
            || h.timestamp_unix_ms < p.timestamp_unix_ms
        {
            bail!("successor seal subject does not extend verified finalized execution");
        }
        let target = self
            .finality_proof()
            .validated_decision_target(self.genesis_config(), self.block())?;
        crate::native_block_seal::subject_from_block_profile_v1(
            block,
            compiled.validator_set(),
            round,
            target,
            compiled.identity().anchor(),
            self.genesis_config().protocol_config_commitment,
            crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_FRESH_SUCCESSOR_PROOF_V1,
        )
    }
}
