//! Application grant transitions over the original immutable native state tree.
//! The native admission/journal owner must establish the parent and commit these
//! effects atomically. This component is not an ingress, login or asset grant.
use super::{record_pages, uca_identity::IdentityAccessV1};
use crate::native_pipeline::state::{
    frontier::{DeclaredAccess, OwnedStateInput},
    tree::{NodeHash, StateChange},
};
use anyhow::{ensure, Context, Result};
use novovm_exec::uca_application::CheckedApplicationGrantV1;
use novovm_protocol::{
    native_query::{IdentityStatusV1, UcaIdentityViewV1},
    uca_application::{ApplicationGrantActionV1, ApplicationGrantSelectorV1},
};
use serde::{Deserialize, Serialize};

const GRANT_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct GrantRecordV1 {
    grant_id: NodeHash,
    issuer_key_ref: NodeHash,
    revision: u64,
    revoked: bool,
    not_before_height: u64,
    expires_at_height: u64,
}

pub struct ApplicationGrantAccessV1 {
    selector: ApplicationGrantSelectorV1,
    identity: IdentityAccessV1,
    prefix: Vec<u8>,
    declarations: Vec<DeclaredAccess>,
}
impl ApplicationGrantAccessV1 {
    pub fn new(selector: ApplicationGrantSelectorV1) -> Result<Self> {
        let grant_id = selector.grant_id()?;
        let prefix = [b"novovm/uca/application-grant/v1/".as_slice(), &grant_id].concat();
        let identity = IdentityAccessV1::new(
            selector.chain_id,
            &selector.genesis,
            selector.identity.as_str(),
        )?;
        let mut declarations = identity.declarations().to_vec();
        declarations.extend(record_pages::declarations(&prefix, GRANT_BYTES, true)?);
        Ok(Self {
            selector,
            identity,
            prefix,
            declarations,
        })
    }
    pub fn declarations(&self) -> &[DeclaredAccess] {
        &self.declarations
    }
    pub fn read(
        &self,
        input: &OwnedStateInput,
        parent_root: NodeHash,
        height: u64,
    ) -> Result<ApplicationGrantObservationV1> {
        let identity = self.identity.read(input, parent_root, height)?;
        if identity.is_some() {
            self.identity.require_initialization_binding(input)?;
        }
        let keys: Vec<_> = self.declarations[self.identity.declarations().len()..]
            .iter()
            .map(|d| d.key.clone())
            .collect();
        let values = input.read_many(&keys)?;
        let record = if values[0].is_none() {
            ensure!(
                values.iter().all(Option::is_none),
                "orphan application grant page"
            );
            None
        } else {
            let bytes = record_pages::read_with(|key| input.read(key), &self.prefix, GRANT_BYTES)?;
            let (record, tail): (GrantRecordV1, _) = postcard::take_from_bytes(&bytes)?;
            ensure!(
                tail.is_empty() && postcard::to_allocvec(&record)? == bytes,
                "noncanonical application grant record"
            );
            ensure!(
                record.grant_id == self.selector.grant_id()?
                    && record.revision > 0
                    && (record.revision < u64::MAX || record.revoked)
                    && record.issuer_key_ref != [0; 32]
                    && record.expires_at_height > record.not_before_height,
                "invalid application grant record"
            );
            Some(record)
        };
        Ok(ApplicationGrantObservationV1 {
            selector: self.selector.clone(),
            parent_root,
            identity,
            record,
        })
    }
    /// Compiles effects only. Never publish without the original admission,
    /// immutable-parent and atomic-commit owners, or reuse after the parent changes.
    pub fn transition(
        &self,
        state: &ApplicationGrantObservationV1,
        checked: &CheckedApplicationGrantV1,
        parent_root: NodeHash,
        execution_height: u64,
    ) -> Result<Vec<StateChange>> {
        let intent = checked.intent();
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
        };
        let record = GrantRecordV1 {
            grant_id: self.selector.grant_id()?,
            issuer_key_ref: intent.issuer_key_ref,
            revision: revision
                .checked_add(1)
                .context("application revision overflow")?,
            revoked,
            not_before_height,
            expires_at_height,
        };
        record_pages::record_changes(&self.prefix, &postcard::to_allocvec(&record)?, GRANT_BYTES)
    }
}

pub struct ApplicationGrantObservationV1 {
    selector: ApplicationGrantSelectorV1,
    parent_root: NodeHash,
    identity: Option<UcaIdentityViewV1>,
    record: Option<GrantRecordV1>,
}
impl ApplicationGrantObservationV1 {
    fn current_identity(&self, parent_root: NodeHash) -> Result<&UcaIdentityViewV1> {
        ensure!(
            parent_root != [0; 32] && parent_root == self.parent_root,
            "application parent mismatch"
        );
        let identity = self
            .identity
            .as_ref()
            .context("registered UCA identity missing")?;
        ensure!(
            identity.status == IdentityStatusV1::Active && identity.state_root == parent_root,
            "UCA identity unavailable or inactive"
        );
        Ok(identity)
    }
    pub fn revision(&self) -> u64 {
        self.record.as_ref().map_or(0, |r| r.revision)
    }
    /// State policy check, NOT proof of key possession or trusted finality.
    /// Callers still need a fresh session-bound device challenge and trusted head.
    pub fn check_at_root(&self, parent_root: NodeHash, height: u64, revision: u64) -> Result<()> {
        let identity = self.current_identity(parent_root)?;
        let grant = self.record.as_ref().context("application grant missing")?;
        ensure!(
            !grant.revoked
                && grant.revision == revision
                && grant.issuer_key_ref == identity.primary_key_ref
                && self.selector.device != identity.primary_key_ref
                && grant.not_before_height <= height
                && height < grant.expires_at_height,
            "application grant revoked, stale, expired or not yet valid"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
