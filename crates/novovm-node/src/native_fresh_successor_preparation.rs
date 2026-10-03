//! Main-owner continuation for an isolated successor. It carries the service
//! key but NEVER crosses the compute worker boundary. A completed computation
//! is only data; registration still reloads and checks the current parent.
use super::*;
use crate::native_block_seal::NovNativeSealSubjectV1;
use crate::tx_ingress::candidate_workspace as workspace;

#[derive(Debug)]
pub(crate) struct SuccessorOutputMismatch;
impl std::fmt::Display for SuccessorOutputMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("received successor output differs from local verified execution")
    }
}
impl std::error::Error for SuccessorOutputMismatch {}

pub(crate) struct FreshSuccessorPreparationV1 {
    config: NovNativeSealServiceConfigV1,
    candidate: [u8; 32],
    expected: Option<NovNativeSealSubjectV1>,
    execution: Option<workspace::ExecutionStartV1>,
}

impl FreshSuccessorPreparationV1 {
    pub(super) fn new(
        config: NovNativeSealServiceConfigV1,
        candidate: [u8; 32],
        expected: Option<NovNativeSealSubjectV1>,
        execution: Option<workspace::ExecutionStartV1>,
    ) -> Self {
        Self {
            config,
            candidate,
            expected,
            execution,
        }
    }

    pub(crate) fn chain_id(&self) -> u64 {
        self.config.chain_id
    }
    pub(crate) fn workspace_id(&self) -> [u8; 32] {
        self.candidate
    }

    pub(crate) fn matches_parent(&self, current: &NovNativeSealServiceConfigV1) -> bool {
        self.config.chain_id == current.chain_id
            && self.config.height == current.height
            && self.config.block_hash == current.block_hash
            && self.config.isolated_workspace_id == current.isolated_workspace_id
            && self.config.fresh_genesis_config_commitment
                == current.fresh_genesis_config_commitment
            && self.config.authority == current.authority
            && self.config.local_validator_id == current.local_validator_id
    }

    pub(crate) fn take_execution(&mut self) -> Result<workspace::ExecutionStartV1> {
        self.execution
            .take()
            .context("asynchronous successor input was not captured")
    }

    pub(crate) fn check_computed(
        &self,
        computed: &workspace::PreparedExecutionV1,
        params: &serde_json::Value,
    ) -> Result<()> {
        if computed.workspace_id() != self.candidate || computed.chain_id() != self.chain_id() {
            bail!("computed successor identity differs from its continuation");
        }
        let parent = workspace::load_finalized_parent_view_v1(
            self.chain_id(),
            self.config
                .isolated_workspace_id
                .context("successor parent missing")?,
            self.config
                .fresh_genesis_config_commitment
                .context("successor genesis missing")?,
            params,
        )?;
        if parent.block().header.height != self.config.height
            || parent.block().header.block_hash != self.config.block_hash
            || parent.finality_proof().authority != self.config.authority
        {
            bail!("computed successor parent changed before input staging");
        }
        // This is a data-only preview, not an artifact or signing capability.
        // Reject a peer's wrong roots/evidence before any new catalog slot or
        // output is stored; the original full readback runs again after commit.
        let actual = computed.preview_successor_subject_v1(
            &parent,
            self.expected.as_ref().map_or(0, |subject| subject.round),
        )?;
        if self
            .expected
            .as_ref()
            .is_some_and(|expected| expected != &actual)
        {
            return Err(SuccessorOutputMismatch.into());
        }
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        params: &serde_json::Value,
    ) -> Result<NovNativeSealServiceConfigV1> {
        let pin = self
            .config
            .fresh_genesis_config_commitment
            .context("successor genesis missing")?;
        let parent_id = self
            .config
            .isolated_workspace_id
            .context("successor parent missing")?;
        // Do not treat the earlier captured parent as live authority after the
        // worker yield. This exact capture also rejects pending publication.
        let parent =
            workspace::load_finalized_parent_view_v1(self.config.chain_id, parent_id, pin, params)?;
        if parent.block().header.height != self.config.height
            || parent.block().header.block_hash != self.config.block_hash
            || parent.finality_proof().authority != self.config.authority
        {
            bail!("prepared successor parent changed before registration");
        }
        let artifact = crate::native_fresh_timing::measure("successor.prepare.artifact", || {
            workspace::load_block_artifact_v1(self.config.chain_id, self.candidate, params)
        })?
        .context("prepared successor output missing")?;
        let actual = parent
            .successor_seal_subject(&artifact, self.expected.as_ref().map_or(0, |s| s.round))?;
        if self
            .expected
            .as_ref()
            .is_some_and(|expected| expected != &actual)
        {
            return Err(SuccessorOutputMismatch.into());
        }
        crate::native_fresh_timing::measure("successor.prepare.register", || {
            workspace::register_finalized_successor_v1(
                self.config.chain_id,
                parent_id,
                self.candidate,
                pin,
                params,
            )
        })?;
        self.config.height = self
            .config
            .height
            .checked_add(1)
            .context("successor height overflow")?;
        self.config.block_hash = artifact.block().header.block_hash;
        self.config.isolated_workspace_id = Some(self.candidate);
        self.config.finalized_parent_workspace_id = Some(parent_id);
        self.config.validate(self.config.chain_id)?;
        Ok(self.config)
    }
}
