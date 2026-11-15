//! Low-frequency identity observation on the original immutable-tree I/O lane.
use super::super::{Hash, IoTicket};
use crate::native_pipeline::{
    business::uca_identity::IdentityAccessV1,
    persistence::io::NodeReadReply,
    pipeline::CandidatePipeline,
    state::frontier::{BulkCapture, CaptureStep},
};
use anyhow::{ensure, Context, Result};
use novovm_protocol::native_query::{Answer, IdentityStateV1};

pub(super) struct IdentityReadV1 {
    access: IdentityAccessV1,
    capture: Option<BulkCapture>,
    ticket: Option<IoTicket<NodeReadReply>>,
    root: Hash,
    height: u64,
}

impl IdentityReadV1 {
    pub(super) fn new(
        access: IdentityAccessV1,
        root: Hash,
        height: u64,
        pipeline_budget: crate::native_pipeline::state::frontier::CaptureBudget,
    ) -> Result<Self> {
        ensure!(root != [0; 32], "identity published parent missing");
        let capture = BulkCapture::new(root, access.declarations(), pipeline_budget)?;
        Ok(Self {
            access,
            capture: Some(capture),
            ticket: None,
            root,
            height,
        })
    }

    pub(super) fn poll(&mut self, pipeline: &CandidatePipeline) -> Result<Option<Answer>> {
        let capture = self.capture.as_mut().context("identity read consumed")?;
        if let Some(ticket) = self.ticket.as_mut() {
            let Some(nodes) = ticket.try_take()? else {
                return Ok(None);
            };
            capture.accept(nodes)?;
            self.ticket = None;
            return Ok(None);
        }
        match capture.advance(64)? {
            CaptureStep::More => Ok(None),
            CaptureStep::NeedRead => {
                let hashes = capture
                    .next_request()?
                    .context("identity read nodes missing")?;
                self.ticket = pipeline.try_read_state_nodes(hashes)?;
                Ok(None)
            }
            CaptureStep::Complete => {
                let input = self.capture.take().expect("capture present").finish()?;
                let state = match self.access.read(&input, self.root, self.height)? {
                    Some(view) => IdentityStateV1::Present(view),
                    None => IdentityStateV1::Missing {
                        state_root: self.root,
                        height: self.height,
                    },
                };
                Ok(Some(Answer::UcaIdentity(state)))
            }
        }
    }
}
