//! Conditional consensus metadata I/O on the resident candidate storage owner.
//! Pure keys, limits and transitions are defined once in novovm-consensus.
//! This adapter retains the a7db795 whole-transition CAS, WAL and exact readback;
//! neither a submitted ticket nor local bytes confer signing/finality permission.

use super::CandidateStore;
use anyhow::{ensure, Result};
#[cfg(test)]
use novovm_consensus::round_bft::journal::metadata::{MetaChange, MetaGuard, MAX_TRANSITION_BYTES};
pub(crate) use novovm_consensus::round_bft::journal::metadata::{
    MetaKey, MetaOutcome, MetaTransition, MetadataSnapshot, MAX_KEYS, MAX_VALUE_BYTES,
};
use novovm_exec::resident::StorageWrite;
#[cfg(test)]
use std::collections::BTreeSet;

impl CandidateStore {
    /// Same owner/DB/domain as candidate content. Duplicate read keys preserve
    /// request order, but mutation keys are unique. None differs from empty bytes.
    pub(crate) fn read_metadata(&self, keys: &[MetaKey]) -> Result<MetadataSnapshot> {
        ensure!(
            (1..=MAX_KEYS).contains(&keys.len()),
            "metadata read key count invalid"
        );
        let relative: Vec<_> = keys.iter().map(MetaKey::relative_key).collect();
        let values = self.read_relative(&relative)?;
        if values.len() != keys.len()
            || values
                .iter()
                .flatten()
                .any(|value| value.len() > MAX_VALUE_BYTES)
        {
            self.write_frozen.set(true);
            anyhow::bail!("stored metadata response exceeds its count/value bound");
        }
        Ok(MetadataSnapshot { values })
    }

    /// One non-yielding owner operation: read/compare, one atomic WAL-synchronous
    /// AOEM batch, then full readback. This is local CAS under the unique owner,
    /// not a cross-process distributed transaction or an execution certificate.
    pub(crate) fn apply_metadata(&self, transition: &MetaTransition) -> Result<MetaOutcome> {
        self.writable()?;
        let keys: Vec<_> = transition
            .guards()
            .iter()
            .map(|guard| guard.key.clone())
            .chain(transition.changes().iter().map(|change| change.key.clone()))
            .collect();
        let actual = self.read_metadata(&keys)?.values;
        let (actual_guards, actual_changes) = actual.split_at(transition.guards().len());
        // A stale parent/head must reject even an exact replay. Guard keys are
        // never written, and all conditions share this non-yielding owner turn.
        if !actual_guards
            .iter()
            .zip(transition.guards())
            .all(|(value, guard)| value == &guard.expected)
        {
            return Ok(MetaOutcome::Conflict);
        }
        if actual_changes
            .iter()
            .zip(transition.changes())
            .all(|(value, change)| value.as_ref() == Some(&change.value))
        {
            return Ok(MetaOutcome::AlreadyPresent);
        }
        if !actual_changes
            .iter()
            .zip(transition.changes())
            .all(|(value, change)| value == &change.expected)
        {
            return Ok(MetaOutcome::Conflict);
        }
        let writes: Vec<_> = transition
            .changes()
            .iter()
            .map(|change| StorageWrite::Put {
                key: self.scoped_key(&change.key.relative_key()),
                value: change.value.clone(),
            })
            .collect();
        // The adapter permanently poisons any uncertain native write result;
        // it will never reopen/retry the session to bypass this boundary.
        self.storage.borrow_mut().atomic_write_batch(&writes)?;
        let readback = self.read_metadata(&keys);
        match readback {
            Ok(snapshot)
                if snapshot
                    .values
                    .iter()
                    .take(transition.guards().len())
                    .zip(transition.guards())
                    .all(|(value, guard)| value == &guard.expected)
                    && snapshot
                        .values
                        .iter()
                        .skip(transition.guards().len())
                        .zip(transition.changes())
                        .all(|(value, change)| value.as_ref() == Some(&change.value)) =>
            {
                Ok(MetaOutcome::Applied)
            }
            Ok(_) => {
                self.write_frozen.set(true);
                anyhow::bail!("metadata atomic batch readback mismatch; recovery required")
            }
            Err(error) => {
                self.write_frozen.set(true);
                Err(error.context("metadata atomic batch readback failed; recovery required"))
            }
        }
    }
}

#[cfg(test)]
mod tests;
