//! Shared read-only decoding and authorization rules for native state and proofs.
use super::super::{record_pages, uca_identity::IdentityAccessV1};
use crate::native_pipeline::state::{
    frontier::{DeclaredAccess, OwnedStateInput},
    tree::NodeHash,
};
use anyhow::{ensure, Context, Result};
use novovm_protocol::{
    native_query::{
        ApplicationGrantStateV1, ApplicationGrantViewV1, IdentityStatusV1, UcaIdentityViewV1,
    },
    uca_application::ApplicationGrantSelectorV1,
};
use serde::{Deserialize, Serialize};

pub(super) const GRANT_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct GrantRecordV1 {
    pub(super) grant_id: NodeHash,
    pub(super) issuer_key_ref: NodeHash,
    pub(super) revision: u64,
    pub(super) revoked: bool,
    pub(super) not_before_height: u64,
    pub(super) expires_at_height: u64,
}

pub struct ApplicationGrantAccessV1 {
    pub(super) selector: ApplicationGrantSelectorV1,
    identity: IdentityAccessV1,
    pub(super) prefix: Vec<u8>,
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
        self.read_under_rules(input, parent_root, height, false)
    }

    pub(crate) fn read_under_rules(
        &self,
        input: &OwnedStateInput,
        parent_root: NodeHash,
        height: u64,
        native_registration: bool,
    ) -> Result<ApplicationGrantObservationV1> {
        let identity = self.identity.read(input, parent_root, height)?;
        if identity.is_some() {
            self.identity
                .require_original_binding(input, native_registration)?;
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
            height,
            identity,
            record,
        })
    }
}

pub struct ApplicationGrantObservationV1 {
    pub(super) selector: ApplicationGrantSelectorV1,
    parent_root: NodeHash,
    height: u64,
    identity: Option<UcaIdentityViewV1>,
    pub(super) record: Option<GrantRecordV1>,
}
impl ApplicationGrantObservationV1 {
    /// Retains revoked/expired/inactive metadata instead of disguising it as
    /// absence. This projection cannot certify device possession or finality.
    pub fn query_state(&self, height: u64) -> Result<ApplicationGrantStateV1> {
        ensure!(
            self.height == height,
            "application observation height mismatch"
        );
        let Some(identity) = &self.identity else {
            return Ok(ApplicationGrantStateV1::IdentityMissing {
                state_root: self.parent_root,
                height,
            });
        };
        ensure!(
            identity.height == height && identity.state_root == self.parent_root,
            "application observation height/root mismatch"
        );
        Ok(match &self.record {
            None => ApplicationGrantStateV1::Missing {
                state_root: self.parent_root,
                height,
            },
            Some(record) => ApplicationGrantStateV1::Present(ApplicationGrantViewV1 {
                state_root: self.parent_root,
                height,
                revision: record.revision,
                issuer_key_ref: record.issuer_key_ref,
                identity_key_ref: identity.primary_key_ref,
                identity_status: identity.status,
                revoked: record.revoked,
                not_before_height: record.not_before_height,
                expires_at_height: record.expires_at_height,
            }),
        })
    }
    pub(super) fn current_identity(&self, parent_root: NodeHash) -> Result<&UcaIdentityViewV1> {
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
