//! Reuse a complete ledger validation only for the identical live DB revision.
//!
//! The cache belongs to the physical RocksDB owner, not a path string, block
//! height, remote proof, or caller-provided token. RocksDB's sequence advances
//! for every put/delete/write batch, including writes outside ledger helpers.
//! Signing/publication locks and current candidate/AOEM bindings remain the
//! caller's responsibility; this cache contains no such authority.

use crate::tx_ingress::fresh_genesis::FreshGenesisConfigV1;
use anyhow::{Context, Result};

struct VerifiedRevision {
    sequence: u64,
    genesis: [u8; 32],
    namespace: [u8; 32],
    config: FreshGenesisConfigV1,
}

#[derive(Default)]
pub(super) struct FreshLedgerVerifiedRevisionV1 {
    leases: usize,
    verified: Option<VerifiedRevision>,
}

impl FreshLedgerVerifiedRevisionV1 {
    pub(super) fn invalidate(&mut self) {
        self.verified = None;
    }

    pub(super) fn retain(&mut self) -> Result<()> {
        self.leases = self
            .leases
            .checked_add(1)
            .context("fresh ledger revision lease overflow")?;
        Ok(())
    }

    pub(super) fn release(&mut self) {
        self.leases = self.leases.saturating_sub(1);
        if self.leases == 0 {
            self.verified = None;
        }
    }

    pub(super) fn get(
        &self,
        sequence: u64,
        genesis: [u8; 32],
        namespace: [u8; 32],
    ) -> Option<FreshGenesisConfigV1> {
        if self.leases == 0 {
            return None;
        }
        self.verified.as_ref().and_then(|verified| {
            (verified.sequence == sequence
                && verified.genesis == genesis
                && verified.namespace == namespace)
                .then(|| verified.config.clone())
        })
    }

    pub(super) fn store(
        &mut self,
        sequence: u64,
        genesis: [u8; 32],
        namespace: [u8; 32],
        config: &FreshGenesisConfigV1,
    ) {
        if self.leases != 0 {
            self.verified = Some(VerifiedRevision {
                sequence,
                genesis,
                namespace,
                config: config.clone(),
            });
        }
    }
}
