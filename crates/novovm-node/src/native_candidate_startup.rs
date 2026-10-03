//! Historical output lookup only. Live signing/publication remains gated by
//! the service's existing authority and recovery checks after configuration resolves.
use super::*;

pub(crate) struct StartupArtifactV1 {
    pub(crate) artifact: IsolatedBlockArtifactV1,
    pub(crate) previous: Option<[u8; 32]>,
    pub(crate) authority: crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1,
    pub(crate) pending_promotion: bool,
}

fn artifact(
    workspace: &WorkspaceStore,
    block: NovNativeDurableBlockV1,
    binding: crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1,
    params: &serde_json::Value,
) -> Result<IsolatedBlockArtifactV1> {
    let image =
        block_artifact::load_block_artifact_inner_v1(workspace, binding.workspace_id, params)?
            .context("startup pinned execution output missing")?;
    if image.block() != &block
        || image.plan_commitment != binding.plan_commitment
        || image.output_digest != binding.output_digest
    {
        bail!("startup execution differs from immutable ledger binding");
    }
    Ok(image)
}

pub(crate) fn load_startup_artifact_v1(
    chain: u64,
    genesis: [u8; 32],
    height: u64,
    hash: [u8; 32],
    id: [u8; 32],
    previous: Option<[u8; 32]>,
    params: &serde_json::Value,
) -> Result<Option<StartupArtifactV1>> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let path = resolve_native_execution_store_path_from_params_v1(params)
        .context("startup native path missing")?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "startup namespace")?;
    let Some(tip) = NovNativeBlockLedgerV1::fresh_startup_tip_v1(
        &nov_native_block_ledger_rocksdb_path_v1(&path),
        genesis,
        namespace,
        height,
        hash,
        id,
        previous,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(StartupArtifactV1 {
        artifact: artifact(&workspace, tip.block, tip.execution, params)?,
        previous: tip.previous,
        authority: tip.authority,
        pending_promotion: tip.pending_promotion,
    }))
}

pub(crate) fn load_startup_successor_v1(
    chain: u64,
    genesis: [u8; 32],
    parent: &IsolatedBlockArtifactV1,
    hash: [u8; 32],
    params: &serde_json::Value,
) -> Result<IsolatedBlockArtifactV1> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let path = resolve_native_execution_store_path_from_params_v1(params)
        .context("startup native path missing")?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "startup namespace")?;
    let (block, binding) = NovNativeBlockLedgerV1::fresh_startup_successor_v1(
        &nov_native_block_ledger_rocksdb_path_v1(&path),
        genesis,
        namespace,
        parent.block().header.height,
        parent.block().header.block_hash,
        hash,
    )?;
    artifact(&workspace, block, binding, params)
}
