//! Application grant transitions over the original immutable native state tree.
//! The native admission/journal owner must establish the parent and commit these
//! effects atomically. This component is not an ingress, login or asset grant.
use super::record_pages;
use crate::native_pipeline::state::{
    frontier::{DeclaredAccess, OwnedStateInput},
    tree::{NodeHash, StateChange},
};
use anyhow::{ensure, Context, Result};
use novovm_exec::uca_application::CheckedApplicationGrantV1;
#[cfg(test)]
use novovm_protocol::native_query::{ApplicationGrantStateV1, IdentityStatusV1};
use novovm_protocol::uca_application::{ApplicationGrantActionV1, ApplicationGrantIntentV1};

mod batch;
pub(crate) use batch::application_command_id;
pub use batch::{
    application_effect_contract_v1, application_program_id_v1, application_receipt_codec_v1,
    ApplicationGrantBodyV1, ApplicationGrantCaptureV1, ApplicationGrantInputV1,
    ApplicationGrantPlanV1, ExecutedApplicationGrantBatchV1, APPLICATION_SEMANTIC_VERSION_V1,
};

pub(crate) mod state;
pub use state::{ApplicationGrantAccessV1, ApplicationGrantObservationV1};
use state::{GrantRecordV1, GRANT_BYTES};

impl ApplicationGrantAccessV1 {
    /// Compiles effects only. Never publish without the original admission,
    /// immutable-parent and atomic-commit owners, or reuse after the parent changes.
    pub fn transition(
        &self,
        state: &ApplicationGrantObservationV1,
        checked: &CheckedApplicationGrantV1,
        parent_root: NodeHash,
        execution_height: u64,
    ) -> Result<Vec<StateChange>> {
        let record =
            self.transition_record(state, checked.intent(), parent_root, execution_height)?;
        record_pages::record_changes(&self.prefix, &postcard::to_allocvec(&record)?, GRANT_BYTES)
    }

    pub(crate) fn preflight(
        &self,
        state: &OwnedStateInput,
        checked: &CheckedApplicationGrantV1,
        parent_root: NodeHash,
        parent_height: u64,
        native_registration: bool,
    ) -> Result<()> {
        let height = parent_height
            .checked_add(1)
            .context("application height exhausted")?;
        let read = |access: &Self| {
            access.read_under_rules(state, parent_root, parent_height, native_registration)
        };
        let previous = read(self)?;
        if let Some([old, new]) = checked.intent().replacement_effects() {
            self.transition_record(&previous, &old, parent_root, height)?;
            let next = Self::new(new.selector.clone())?;
            let ApplicationGrantActionV1::Authorize {
                not_before_height, ..
            } = new.action
            else {
                unreachable!("replacement successor is an authorization")
            };
            ensure!(
                not_before_height <= height,
                "replacement device must be active at execution height"
            );
            next.transition_record(&read(&next)?, &new, parent_root, height)?;
        } else {
            self.transition_record(&previous, checked.intent(), parent_root, height)?;
        }
        Ok(())
    }

    fn transition_record(
        &self,
        state: &ApplicationGrantObservationV1,
        intent: &ApplicationGrantIntentV1,
        parent_root: NodeHash,
        execution_height: u64,
    ) -> Result<GrantRecordV1> {
        ensure!(
            intent.selector == self.selector && state.selector == self.selector,
            "application grant selector mismatch"
        );
        let identity = state.current_identity(parent_root)?;
        ensure!(
            identity.primary_key_ref == intent.issuer_key_ref,
            "application issuer is not current UCA owner"
        );
        ensure!(
            intent.issuer_key_ref != self.selector.device,
            "UCA root must not be an application device key"
        );
        ensure!(
            intent.valid_from_height <= execution_height
                && execution_height < intent.valid_until_height,
            "application command expired or not yet valid"
        );
        let revision = state.record.as_ref().map_or(0, |r| r.revision);
        ensure!(
            intent.expected_revision == revision,
            "application grant revision mismatch"
        );
        let (not_before_height, expires_at_height, revoked) = match intent.action {
            ApplicationGrantActionV1::Authorize {
                not_before_height,
                expires_at_height,
            } => {
                ensure!(
                    execution_height < expires_at_height,
                    "application grant already expired"
                );
                (not_before_height, expires_at_height, false)
            }
            ApplicationGrantActionV1::Revoke => {
                let previous = state
                    .record
                    .as_ref()
                    .context("cannot revoke missing application grant")?;
                ensure!(!previous.revoked, "application grant already revoked");
                (previous.not_before_height, previous.expires_at_height, true)
            }
            ApplicationGrantActionV1::ReplaceDevice { .. } => {
                anyhow::bail!("device replacement requires atomic application batch execution")
            }
        };
        Ok(GrantRecordV1 {
            grant_id: self.selector.grant_id()?,
            issuer_key_ref: intent.issuer_key_ref,
            revision: revision
                .checked_add(1)
                .context("application revision overflow")?,
            revoked,
            not_before_height,
            expires_at_height,
        })
    }
}

#[cfg(test)]
mod tests;
