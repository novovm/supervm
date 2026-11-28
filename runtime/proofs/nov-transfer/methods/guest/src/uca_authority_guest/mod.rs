//! Compile the native state relation and tree directly into the guest image.
#![allow(dead_code, unused_imports)]

mod business;
#[path = "../../../../../../../crates/novovm-node/src/native_pipeline/state/mod.rs"]
pub(crate) mod state;
#[path = "../../../../../../../crates/novovm-node/src/native_pipeline/proof/uca_authority.rs"]
pub(crate) mod uca_authority;
