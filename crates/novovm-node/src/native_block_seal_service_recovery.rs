//! Restore the exact locally bound successor, never select an arbitrary branch.
use super::*;
use crate::native_block_seal::{
    round_driver::NovNativeSealRoundDriverV1, NovNativeBlockSealStoreV1,
};
use crate::tx_ingress::candidate_workspace as workspace;

impl NovNativeSealServiceConfigV1 {
    pub(super) fn check_startup_paths(&self, params: &serde_json::Value) -> Result<()> {
        let paths = crate::tx_ingress::native_persistence_write_paths_v1(params);
        let ledger = paths
            .iter()
            .find(|(label, _)| *label == "native block ledger")
            .context("startup ledger path missing")?
            .1
            .clone();
        let writes = paths
            .into_iter()
            .filter(|(label, _)| *label != "native block ledger")
            .map(|(_, path)| path)
            .collect::<Vec<_>>();
        crate::native_block_seal::service_paths::validate_service_paths_v1(
            self,
            &ledger,
            &writes,
            &[],
        )
    }

    /// Resolve only configuration. The lifecycle must then open the normal
    /// live-verified service or resume publication with the durable full QC.
    pub(crate) fn resolve_lifecycle_startup(mut self, params: &serde_json::Value) -> Result<Self> {
        self.validate(self.chain_id)?;
        if !self.receive_successors {
            return self.resolve_finalized_startup(params);
        }
        self.check_startup_paths(params)?;
        let pin = self
            .fresh_genesis_config_commitment
            .context("startup genesis pin missing")?;
        let Some(parent) = workspace::load_startup_artifact_v1(
            self.chain_id,
            pin,
            self.height,
            self.block_hash,
            self.isolated_workspace_id
                .context("startup anchor missing")?,
            self.finalized_parent_workspace_id,
            params,
        )?
        else {
            // The operator explicitly configured an as-yet-unfinalized candidate.
            return self.resolve_finalized_startup(params);
        };
        if parent.authority != self.authority {
            bail!("startup authority differs from operator pin");
        }
        self.height = parent.artifact.block().header.height;
        self.block_hash = parent.artifact.block().header.block_hash;
        self.isolated_workspace_id = Some(parent.artifact.workspace_id);
        self.finalized_parent_workspace_id = parent.previous;
        let height = self
            .height
            .checked_add(1)
            .context("startup next height overflow")?;
        let bound = match NovNativeBlockSealStoreV1::open_existing_read_only(&self.seal_store_path)?
        {
            Some(store) => NovNativeSealRoundDriverV1::startup_candidate(
                &store,
                &self.authority,
                height,
                self.local_validator_id,
            )?,
            None => None,
        };
        let Some(bound) = bound else {
            if parent.pending_promotion {
                bail!("pending promotion has no local successor owner binding");
            }
            // Idle startup still checks current AOEM authority, not just history.
            return self.resolve_finalized_startup(params);
        };
        let hash = bound.block_hash;
        let candidate = workspace::load_startup_successor_v1(
            self.chain_id,
            pin,
            &parent.artifact,
            hash,
            params,
        )?;
        self.height = height;
        self.block_hash = hash;
        self.isolated_workspace_id = Some(candidate.workspace_id);
        self.finalized_parent_workspace_id = Some(parent.artifact.workspace_id);
        self.justify_qc_hash = bound.justify_qc_hash;
        self.validate(self.chain_id)?;
        Ok(self)
    }
}
