//! Offline isolated-successor preparation. A completed computation is only
//! data; registration reloads and checks the current parent and signed output.
use super::*;
use crate::native_block_seal::NovNativeSealSubjectV1;
use crate::tx_ingress::candidate_workspace as workspace;

#[derive(Debug)]
struct SuccessorOutputMismatch;
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
}

impl FreshSuccessorPreparationV1 {
    pub(super) fn new(
        config: NovNativeSealServiceConfigV1,
        candidate: [u8; 32],
        expected: Option<NovNativeSealSubjectV1>,
    ) -> Self {
        Self {
            config,
            candidate,
            expected,
        }
    }

    pub(crate) fn chain_id(&self) -> u64 {
        self.config.chain_id
    }
    pub(crate) fn workspace_id(&self) -> [u8; 32] {
        self.candidate
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
        // Recheck the parent between synchronous preparation and completion;
        // the earlier capture is not live authority. Also reject pending publication.
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
