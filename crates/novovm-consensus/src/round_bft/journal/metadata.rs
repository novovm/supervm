//! Pure keys/bounds migrated from a7db795 persistence/metadata.rs.
//! Private, bounded conditional metadata updates on the existing storage owner.
//! This is NOT signature permission, a verified consensus transition or finality.
//! Callers must enforce protocol rules; bytes here are deliberately opaque.
//! Candidate node/document/completion keys cannot be addressed by this API.

use anyhow::{ensure, Context, Result};
use std::collections::BTreeSet;

pub const MAX_KEYS: usize = 8;
const MAX_GUARDS: usize = 2;
// Accommodates the bounded 1024-validator certificate/outbox profile while
// retaining the independent 1 MiB total expected-plus-new transition budget.
pub const MAX_VALUE_BYTES: usize = 512 * 1024;
pub const MAX_TRANSITION_BYTES: usize = 1024 * 1024;
const PREFIX: &[u8] = b"m/consensus/v1/";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MetaKey {
    ConsensusState([u8; 32]),
    ConsensusOutbox { validator: [u8; 32], sequence: u64 },
    ChainHead,
    ChainBlock { height: u64 },
}

impl MetaKey {
    pub fn relative_key(&self) -> Vec<u8> {
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
pub struct MetadataSnapshot {
    pub values: Vec<Option<Vec<u8>>>,
}

pub struct MetaChange {
    pub key: MetaKey,
    pub expected: Option<Vec<u8>>,
    pub value: Vec<u8>,
}

/// A read-only precondition, checked even when every mutation is already present.
pub struct MetaGuard {
    pub key: MetaKey,
    pub expected: Option<Vec<u8>>,
}

/// Private construction enforces storage bounds, NOT consensus authorization.
/// No Deserialize implementation may manufacture a checked transition.
pub struct MetaTransition {
    changes: Vec<MetaChange>,
    guards: Vec<MetaGuard>,
    retained_bytes: usize,
}

impl MetaTransition {
    pub fn new(changes: Vec<MetaChange>) -> Result<Self> {
        Self::with_guards(changes, Vec::new())
    }

    pub fn with_guards(changes: Vec<MetaChange>, guards: Vec<MetaGuard>) -> Result<Self> {
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

    pub fn changes(&self) -> &[MetaChange] {
        &self.changes
    }

    pub fn guards(&self) -> &[MetaGuard] {
        &self.guards
    }

    /// Retained logical keys and guard/mutation expected/new values, excluding
    /// allocator, native-wire and DB overhead. The reply is a fixed-size outcome.
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetaOutcome {
    Applied,
    AlreadyPresent,
    Conflict,
}
