//! Host-defined, checked algebraic effects for the existing Transfer semantics.
//! This is not a new wire, ledger, fee policy, or NOV-specific AOEM opcode.
//!
//! A balance that is never debited in this batch admits nonnegative credit
//! reduction IF parent + sum(all requested amounts) fits u128. Failed transfers
//! contribute zero, so every subset/prefix also fits. All other balance/nonce
//! dependencies retain the conservative evaluator. The guard is not a claim
//! that arbitrary checked debits, overflowing adds, or failures commute.

use super::{
    conflict_components_excluding_credits_v1, conflict_components_v1, Account, TransferError,
    TransferExecutionFailureV1, TransferExecutionOutcomeV1, TransferIntent, TransferSnapshot,
};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
struct CheckedCreditBoundV1 {
    parent: u128,
    maximum_credit: u128,
}

/// Constructed only from one authenticated parent view. The private guards
/// travel with the plan to the AOEM callback that performs ordered reduction.
#[derive(Debug, Clone)]
pub(crate) struct TransferEffectPlanV1 {
    pub components: Vec<Vec<usize>>,
    intents: Vec<TransferIntent>,
    credits: BTreeMap<Account, CheckedCreditBoundV1>,
}

impl TransferEffectPlanV1 {
    pub fn build(intents: &[TransferIntent], snapshots: &[TransferSnapshot]) -> Result<Self> {
        if intents.len() != snapshots.len() || intents.is_empty() || intents.len() > 1024 {
            bail!("transfer effect plan requires 1..=1024 matching inputs");
        }
        let mut balances = BTreeMap::new();
        let mut nonces = BTreeMap::new();
        let mut payers = BTreeSet::new();
        let mut requested = BTreeMap::<Account, (usize, Option<u128>)>::new();
        for (intent, snapshot) in intents.iter().zip(snapshots) {
            for (account, value) in [
                (&intent.from, snapshot.payer_balance),
                (&intent.to, snapshot.recipient_balance),
            ] {
                if balances
                    .insert(account.clone(), value)
                    .is_some_and(|old| old != value)
                {
                    bail!("transfer effect plan has inconsistent parent balance snapshots");
                }
            }
            if nonces
                .insert(&intent.nonce_identity, snapshot.next_nonce)
                .is_some_and(|old| old != snapshot.next_nonce)
            {
                bail!("transfer effect plan has inconsistent parent nonce snapshots");
            }
            payers.insert(intent.from.clone());
            let entry = requested.entry(intent.to.clone()).or_insert((0, Some(0)));
            entry.0 += 1;
            entry.1 = entry.1.and_then(|total| total.checked_add(intent.amount));
        }
        let mut credits = BTreeMap::new();
        for (account, (count, maximum_credit)) in requested {
            let Some(maximum_credit) = maximum_credit else {
                continue;
            };
            let parent = balances[&account];
            if count > 1
                && !payers.contains(&account)
                && parent.checked_add(maximum_credit).is_some()
            {
                credits.insert(
                    account,
                    CheckedCreditBoundV1 {
                        parent,
                        maximum_credit,
                    },
                );
            }
        }
        let components = if credits.is_empty() {
            conflict_components_v1(intents)
        } else {
            conflict_components_excluding_credits_v1(intents, &credits.keys().cloned().collect())
        };
        // If all credits already belong to one component, its own evaluator
        // produces the right prefix. Do not add a pointless reduction barrier.
        let mut first_component = BTreeMap::new();
        let mut shared = BTreeSet::new();
        for (component, indices) in components.iter().enumerate() {
            for &index in indices {
                let account = &intents[index].to;
                if credits.contains_key(account)
                    && first_component
                        .insert(account.clone(), component)
                        .is_some_and(|previous| previous != component)
                {
                    shared.insert(account.clone());
                }
            }
        }
        credits.retain(|account, _| shared.contains(account));
        Ok(Self {
            components,
            intents: intents.to_vec(),
            credits,
        })
    }

    pub fn has_credit_reduction(&self) -> bool {
        !self.credits.is_empty()
    }

    /// Run only inside AOEM compute, after every component has completed.
    /// Materialize the original-order observations without re-executing payer
    /// arithmetic. Even failed transactions observe the correct recipient
    /// prefix. The Host still checks its live prefix after global fee settlement;
    /// a rejected global fee can require its existing AOEM repair path.
    pub fn reduce_ordered(&self, outcomes: &mut [TransferExecutionOutcomeV1]) -> Result<()> {
        if outcomes.len() != self.intents.len() {
            bail!("transfer credit reduction outcome count mismatch");
        }
        let mut prefixes: BTreeMap<_, _> = self
            .credits
            .iter()
            .map(|(account, bound)| (account.clone(), (bound.parent, 0u128)))
            .collect();
        for (intent, outcome) in self.intents.iter().zip(outcomes) {
            let Some(bound) = self.credits.get(&intent.to) else {
                continue;
            };
            if outcome.delta.tx_hash != intent.tx_hash
                || outcome.delta.payer.account != intent.from
                || outcome.delta.recipient.account != intent.to
                || intent.from == intent.to
                || matches!(
                    outcome.failure(),
                    Some(TransferExecutionFailureV1::Business(
                        TransferError::RecipientOverflow
                    ))
                )
            {
                bail!("transfer credit reduction outcome binding mismatch");
            }
            let credit = if outcome.is_success() {
                intent.amount
            } else {
                0
            };
            if outcome
                .delta
                .recipient
                .after
                .checked_sub(outcome.delta.recipient.before)
                != Some(credit)
            {
                bail!("transfer credit reduction has an invalid nonnegative effect");
            }
            let (balance, accumulated) = prefixes
                .get_mut(&intent.to)
                .context("missing credit prefix")?;
            *accumulated = accumulated
                .checked_add(credit)
                .context("credit reduction overflow")?;
            if *accumulated > bound.maximum_credit {
                bail!("transfer credit reduction exceeded checked bound");
            }
            outcome.delta.recipient.before = *balance;
            *balance = balance
                .checked_add(credit)
                .context("checked credit prefix overflow")?;
            outcome.delta.recipient.after = *balance;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "native_transfer_effects_tests.rs"]
mod tests;
