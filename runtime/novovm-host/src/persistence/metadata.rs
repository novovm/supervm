//! Private, bounded conditional metadata updates on the existing storage owner.
//! This is NOT signature permission, a verified consensus transition or finality.
//! Callers must enforce protocol rules; bytes here are deliberately opaque.
//! Candidate node/document/completion keys cannot be addressed by this API.

use super::CandidateStore;
use anyhow::{ensure, Context, Result};
use novovm_aoem::StorageWrite;
use std::collections::BTreeSet;

const MAX_KEYS: usize = 8;
const MAX_GUARDS: usize = 2;
// Accommodates the bounded 1024-validator certificate/outbox profile while
// retaining the independent 1 MiB total expected-plus-new transition budget.
const MAX_VALUE_BYTES: usize = 512 * 1024;
const MAX_TRANSITION_BYTES: usize = 1024 * 1024;
const PREFIX: &[u8] = b"m/consensus/v1/";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MetaKey {
    ConsensusState([u8; 32]),
    ConsensusOutbox { validator: [u8; 32], sequence: u64 },
    ChainHead,
    ChainBlock { height: u64 },
}

impl MetaKey {
    fn relative_key(&self) -> Vec<u8> {
        match self {
            Self::ConsensusState(validator) => [PREFIX, b"state/", validator.as_slice()].concat(),
            Self::ConsensusOutbox {
                validator,
                sequence,
            } => [
                PREFIX,
                b"outbox/",
                validator.as_slice(),
                &sequence.to_be_bytes(),
            ]
            .concat(),
            Self::ChainHead => [PREFIX, b"chain/head"].concat(),
            Self::ChainBlock { height } => {
                [PREFIX, b"chain/block/", &height.to_be_bytes()].concat()
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MetadataSnapshot {
    pub values: Vec<Option<Vec<u8>>>,
}

pub(crate) struct MetaChange {
    pub key: MetaKey,
    pub expected: Option<Vec<u8>>,
    pub value: Vec<u8>,
}

/// A read-only precondition, checked even when every mutation is already present.
pub(crate) struct MetaGuard {
    pub key: MetaKey,
    pub expected: Option<Vec<u8>>,
}

/// Private construction enforces storage bounds, NOT consensus authorization.
/// No Deserialize implementation may manufacture a checked transition.
pub(crate) struct MetaTransition {
    changes: Vec<MetaChange>,
    guards: Vec<MetaGuard>,
    retained_bytes: usize,
}

impl MetaTransition {
    pub(crate) fn new(changes: Vec<MetaChange>) -> Result<Self> {
        Self::with_guards(changes, Vec::new())
    }

    pub(crate) fn with_guards(changes: Vec<MetaChange>, guards: Vec<MetaGuard>) -> Result<Self> {
        ensure!(
            (1..=MAX_KEYS).contains(&changes.len()),
            "metadata transition key count invalid"
        );
        ensure!(
            guards.len() <= MAX_GUARDS && changes.len() + guards.len() <= MAX_KEYS,
            "metadata transition guard/total key count invalid"
        );
        let mut seen = BTreeSet::new();
        let mut values = 0usize;
        let mut keys = 0usize;
        for change in &changes {
            ensure!(
                seen.insert(&change.key),
                "duplicate metadata transition key"
            );
            ensure!(
                change.value.len() <= MAX_VALUE_BYTES
                    && change
                        .expected
                        .as_ref()
                        .is_none_or(|expected| expected.len() <= MAX_VALUE_BYTES),
                "metadata transition value exceeds bound"
            );
            ensure!(
                !matches!(
                    change.key,
                    MetaKey::ConsensusOutbox { .. } | MetaKey::ChainBlock { .. }
                ) || change.expected.is_none(),
                "consensus outbox/chain block is append-only; expected must be absent"
            );
            values = values
                .checked_add(change.value.len())
                .and_then(|sum| sum.checked_add(change.expected.as_ref().map_or(0, Vec::len)))
                .context("metadata transition byte count overflow")?;
            ensure!(
                values <= MAX_TRANSITION_BYTES,
                "metadata transition exceeds total byte budget"
            );
            keys = keys
                .checked_add(change.key.relative_key().len())
                .context("metadata key byte count overflow")?;
        }
        for guard in &guards {
            ensure!(
                seen.insert(&guard.key),
                "duplicate or mutated metadata guard key"
            );
            let expected = guard.expected.as_ref().map_or(0, Vec::len);
            ensure!(
                expected <= MAX_VALUE_BYTES,
                "metadata guard value exceeds bound"
            );
            values = values
                .checked_add(expected)
                .context("metadata guard byte count overflow")?;
            ensure!(
                values <= MAX_TRANSITION_BYTES,
                "metadata transition exceeds total byte budget"
            );
            keys = keys
                .checked_add(guard.key.relative_key().len())
                .context("metadata key byte count overflow")?;
        }
        Ok(Self {
            changes,
            guards,
            retained_bytes: values
                .checked_add(keys)
                .context("metadata retained size overflow")?,
        })
    }

    /// Retained logical keys and guard/mutation expected/new values, excluding
    /// allocator, native-wire and DB overhead. The reply is a fixed-size outcome.
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MetaOutcome {
    Applied,
    AlreadyPresent,
    Conflict,
}

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
            .guards
            .iter()
            .map(|guard| guard.key.clone())
            .chain(transition.changes.iter().map(|change| change.key.clone()))
            .collect();
        let actual = self.read_metadata(&keys)?.values;
        let (actual_guards, actual_changes) = actual.split_at(transition.guards.len());
        // A stale parent/head must reject even an exact replay. Guard keys are
        // never written, and all conditions share this non-yielding owner turn.
        if !actual_guards
            .iter()
            .zip(&transition.guards)
            .all(|(value, guard)| value == &guard.expected)
        {
            return Ok(MetaOutcome::Conflict);
        }
        if actual_changes
            .iter()
            .zip(&transition.changes)
            .all(|(value, change)| value.as_ref() == Some(&change.value))
        {
            return Ok(MetaOutcome::AlreadyPresent);
        }
        if !actual_changes
            .iter()
            .zip(&transition.changes)
            .all(|(value, change)| value == &change.expected)
        {
            return Ok(MetaOutcome::Conflict);
        }
        let writes: Vec<_> = transition
            .changes
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
                    .take(transition.guards.len())
                    .zip(&transition.guards)
                    .all(|(value, guard)| value == &guard.expected)
                    && snapshot
                        .values
                        .iter()
                        .skip(transition.guards.len())
                        .zip(&transition.changes)
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
