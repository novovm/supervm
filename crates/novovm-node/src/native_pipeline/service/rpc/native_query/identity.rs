//! Low-frequency identity/application observations on the original tree I/O lane.
use super::super::{Hash, IoTicket};
use crate::native_pipeline::{
    business::{uca_application::ApplicationGrantAccessV1, uca_identity::IdentityAccessV1},
    persistence::io::NodeReadReply,
    pipeline::CandidatePipeline,
    state::frontier::{BulkCapture, CaptureStep, DeclaredAccess},
};
use anyhow::{ensure, Context, Result};
use novovm_protocol::native_query::{Answer, IdentityStateV1};

pub(super) enum AuthorityAccessV1 {
    Identity(IdentityAccessV1),
    Application(Box<ApplicationGrantAccessV1>),
}
impl AuthorityAccessV1 {
    fn declarations(&self) -> Vec<DeclaredAccess> {
        let declarations = match self {
            Self::Identity(access) => access.declarations(),
            Self::Application(access) => access.declarations(),
        };
        // A query never obtains the transition compiler's write declarations.
        declarations
            .iter()
            .map(|d| DeclaredAccess {
                key: d.key.clone(),
                may_put: false,
                may_delete: false,
            })
            .collect()
    }
}

pub(super) struct AuthorityReadV1 {
    access: AuthorityAccessV1,
    capture: Option<BulkCapture>,
    ticket: Option<IoTicket<NodeReadReply>>,
    root: Hash,
    height: u64,
}

impl AuthorityReadV1 {
    pub(super) fn new(
        access: AuthorityAccessV1,
        root: Hash,
        height: u64,
        pipeline_budget: crate::native_pipeline::state::frontier::CaptureBudget,
    ) -> Result<Self> {
        ensure!(root != [0; 32], "identity published parent missing");
        let capture = BulkCapture::new(root, &access.declarations(), pipeline_budget)?;
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
                let answer = match &self.access {
                    AuthorityAccessV1::Identity(access) => {
                        let state = match access.read(&input, self.root, self.height)? {
                            Some(view) => IdentityStateV1::Present(view),
                            None => IdentityStateV1::Missing {
                                state_root: self.root,
                                height: self.height,
                            },
                        };
                        Answer::UcaIdentity(state)
                    }
                    AuthorityAccessV1::Application(access) => Answer::UcaApplicationGrant(
                        access
                            .read_under_rules(
                                &input,
                                self.root,
                                self.height,
                                pipeline
                                    .business_schedule
                                    .as_ref()
                                    .is_some_and(|schedule| schedule.is_active_at(self.height)),
                            )?
                            .query_state(self.height)?,
                    ),
                };
                Ok(Some(answer))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_protocol::{
        native_query::UcaIdentityIdV1, uca_application::ApplicationGrantSelectorV1,
    };
    #[test]
    fn application_query_capture_drops_all_transition_write_permissions() {
        let access = ApplicationGrantAccessV1::new(ApplicationGrantSelectorV1 {
            chain_id: 7,
            genesis: [1; 32],
            identity: UcaIdentityIdV1::new("uca:alice").unwrap(),
            application: [2; 32],
            service: [3; 32],
            device: [4; 32],
        })
        .unwrap();
        assert!(access.declarations().iter().any(|d| d.may_put));
        let access = AuthorityAccessV1::Application(Box::new(access));
        let declarations = access.declarations();
        assert_eq!(declarations.len(), 35);
        assert!(declarations.iter().all(|d| !d.may_put && !d.may_delete));
    }
}
