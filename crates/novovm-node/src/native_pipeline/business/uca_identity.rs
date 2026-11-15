//! Read the original identity namespace at one captured published root.
//! This observes registered identity state, not app grants or spending rights.
use super::{record_pages, uca_spend_state::identity_prefix};
use crate::native_pipeline::state::{
    frontier::{DeclaredAccess, OwnedStateInput},
    tree::NodeHash,
};
use anyhow::{ensure, Context, Result};
use novovm_adapter_api::{
    uca_key_binding::derive_primary_key_ref_from_binding_v1,
    unified_account::{UcaAccount, UcaKeyAlgo, UcaStatus},
};
use novovm_protocol::native_query::{IdentityStatusV1, UcaIdentityViewV1};

pub(crate) struct IdentityAccessV1 {
    identity: String,
    prefix: Vec<u8>,
    declarations: Vec<DeclaredAccess>,
}

impl IdentityAccessV1 {
    pub(crate) fn new(chain: u64, genesis: &NodeHash, identity: &str) -> Result<Self> {
        let prefix = identity_prefix(chain, genesis, identity)?;
        let declarations = record_pages::declarations(&prefix, 8192, false)?;
        Ok(Self {
            identity: identity.into(),
            prefix,
            declarations,
        })
    }

    pub(crate) fn declarations(&self) -> &[DeclaredAccess] {
        &self.declarations
    }

    pub(crate) fn read(
        &self,
        input: &OwnedStateInput,
        root: NodeHash,
        height: u64,
    ) -> Result<Option<UcaIdentityViewV1>> {
        ensure!(
            root != [0; 32] && input.parent_root() == root,
            "identity parent mismatch"
        );
        let keys: Vec<_> = self.declarations.iter().map(|d| d.key.clone()).collect();
        let values = input.read_many(&keys)?;
        // Absence requires the entire declared range; an orphan page is corruption.
        if values[0].is_none() {
            ensure!(
                values.iter().all(Option::is_none),
                "orphan identity record page"
            );
            return Ok(None);
        }
        let bytes = record_pages::read_with(|key| input.read(key), &self.prefix, 8192)?;
        let (record, tail): (UcaAccount, _) = postcard::take_from_bytes(&bytes)?;
        ensure!(
            tail.is_empty() && postcard::to_allocvec(&record)? == bytes,
            "noncanonical identity record"
        );
        let binding = record
            .primary_key_binding
            .as_ref()
            .context("identity binding missing")?;
        ensure!(
            record.uca_id == self.identity
                && record.updated_at >= record.created_at
                && binding.key_algo == UcaKeyAlgo::Mldsa65
                && binding.public_key.len() == novovm_protocol::uca_spend::MLDSA65_PUBLIC_KEY_BYTES,
            "identity binding or algorithm mismatch"
        );
        let reference =
            derive_primary_key_ref_from_binding_v1(binding.key_algo, &binding.public_key);
        ensure!(
            record.primary_key_ref == reference,
            "identity key reference mismatch"
        );
        Ok(Some(UcaIdentityViewV1 {
            state_root: root,
            height,
            primary_key_ref: reference.as_slice().try_into()?,
            status: match record.status {
                UcaStatus::Active => IdentityStatusV1::Active,
                UcaStatus::Suspended => IdentityStatusV1::Suspended,
                UcaStatus::Recovering => IdentityStatusV1::Recovering,
                UcaStatus::Revoked => IdentityStatusV1::Revoked,
            },
            created_at_ms: record.created_at,
            updated_at_ms: record.updated_at,
        }))
    }
}

#[cfg(test)]
mod tests;
