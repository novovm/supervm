//! Opt-in dev fixture helpers, NOT a product signing API. Raw production sign
//! methods and safety State stay crate-private even with this feature enabled.
use super::wire::{Context, Hash, Phase, Proposal, ValidatorSet, Vote};
use anyhow::Result;
use ed25519_dalek::SigningKey;

/// Advance a deterministic timer fixture without widening the live timer API.
pub fn round_wait_elapsed<B: super::journal::JournalBackend>(
    journal: &mut super::journal::ValidatorJournal<B>,
    expected_round: u64,
) -> Result<()> {
    journal.round_wait_elapsed(expected_round)
}

pub fn sign_proposal(
    context: Context,
    round: u64,
    value: Hash,
    valid_round: Option<u64>,
    set: &ValidatorSet,
    key: &SigningKey,
) -> Result<Proposal> {
    Proposal::sign(context, round, value, valid_round, set, key)
}

pub fn sign_vote(
    context: Context,
    round: u64,
    phase: Phase,
    value: Option<Hash>,
    set: &ValidatorSet,
    key: &SigningKey,
) -> Result<Vote> {
    Vote::sign(context, round, phase, value, set, key)
}
