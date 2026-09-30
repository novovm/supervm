#![forbid(unsafe_code)]

//! Pure, bounded NOV transfer arithmetic and conflict planning.
//!
//! This is not an ingress, fee schedule, authorization check, or a new ledger.
//! The caller authenticates the intent and supplies its already approved fee and
//! effective fee cap. A zero cap means zero here; any wire-level sentinel must
//! be resolved by the caller. Failed execution charging and nonce consumption
//! are deliberately left to the consensus execution policy: an error produces
//! no delta. All amounts are integer base units, not gas units.
//!
//! The only per-task state is two account balances and one signer nonce. Shared
//! fee funding contributions are reduced with checked arithmetic in original
//! transaction order after execution; they are not a false dependency between
//! otherwise independent tasks. A failed fee reduction must prevent
//! publication of the whole proposed batch.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type Account = [u8; 20];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferIntent {
    pub tx_hash: [u8; 32],
    pub from: Account,
    pub to: Account,
    /// Canonical signer nonce identity, already bound to the authenticated chain.
    pub nonce_identity: String,
    pub nonce: u64,
    pub amount: u128,
    /// Supplied by the fee policy, never estimated by this arithmetic component.
    pub approved_fee: u128,
    pub fee_cap: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferSnapshot {
    pub payer_balance: u128,
    pub recipient_balance: u128,
    pub next_nonce: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceDelta {
    pub account: Account,
    pub before: u128,
    pub after: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDelta {
    pub tx_hash: [u8; 32],
    pub payer: BalanceDelta,
    /// For a self-transfer this equals `payer`; apply that key only once.
    pub recipient: BalanceDelta,
    pub nonce_identity: String,
    pub nonce_before: u64,
    pub nonce_after: u64,
    /// Pending funding for common settlement, without choosing provider,
    /// treasury, burn, or any other distribution proportions.
    pub fee_funding_delta: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferError {
    MissingNonceIdentity,
    NonceMismatch { expected: u64, provided: u64 },
    NonceExhausted,
    FeeCapExceeded { approved_fee: u128, fee_cap: u128 },
    DebitOverflow,
    InsufficientFunds { available: u128, required: u128 },
    RecipientOverflow,
    InconsistentSelfBalance,
    FeeFundingOverflow,
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNonceIdentity => formatter.write_str("missing signer nonce identity"),
            Self::NonceMismatch { expected, provided } => {
                write!(
                    formatter,
                    "nonce mismatch: expected {expected}, got {provided}"
                )
            }
            Self::NonceExhausted => formatter.write_str("signer nonce exhausted"),
            Self::FeeCapExceeded {
                approved_fee,
                fee_cap,
            } => {
                write!(
                    formatter,
                    "approved fee {approved_fee} exceeds cap {fee_cap}"
                )
            }
            Self::DebitOverflow => formatter.write_str("transfer amount plus fee overflows"),
            Self::InsufficientFunds {
                available,
                required,
            } => {
                write!(
                    formatter,
                    "insufficient NOV: available {available}, required {required}"
                )
            }
            Self::RecipientOverflow => formatter.write_str("recipient NOV balance overflows"),
            Self::InconsistentSelfBalance => {
                formatter.write_str("self-transfer balance views disagree")
            }
            Self::FeeFundingOverflow => formatter.write_str("fee funding reduction overflows"),
        }
    }
}

impl std::error::Error for TransferError {}

/// Compute one complete transition without mutating the snapshot.
///
/// Affordability covers the entire amount plus fee, including self-transfers;
/// a successful self-transfer only changes its balance by the fee. Zero amount
/// and zero fee are representable, without deciding whether a product policy
/// permits them. The caller rejects any disallowed intent before this function.
pub fn compute_delta(
    intent: &TransferIntent,
    snapshot: &TransferSnapshot,
) -> Result<TransferDelta, TransferError> {
    if intent.nonce_identity.is_empty() {
        return Err(TransferError::MissingNonceIdentity);
    }
    if intent.nonce != snapshot.next_nonce {
        return Err(TransferError::NonceMismatch {
            expected: snapshot.next_nonce,
            provided: intent.nonce,
        });
    }
    let nonce_after = intent
        .nonce
        .checked_add(1)
        .ok_or(TransferError::NonceExhausted)?;
    if intent.approved_fee > intent.fee_cap {
        return Err(TransferError::FeeCapExceeded {
            approved_fee: intent.approved_fee,
            fee_cap: intent.fee_cap,
        });
    }
    let self_transfer = intent.from == intent.to;
    if self_transfer && snapshot.payer_balance != snapshot.recipient_balance {
        return Err(TransferError::InconsistentSelfBalance);
    }
    let debit = intent
        .amount
        .checked_add(intent.approved_fee)
        .ok_or(TransferError::DebitOverflow)?;
    let debited =
        snapshot
            .payer_balance
            .checked_sub(debit)
            .ok_or(TransferError::InsufficientFunds {
                available: snapshot.payer_balance,
                required: debit,
            })?;
    let (payer_after, recipient_after) = if self_transfer {
        // debited <= original balance - amount, so this addition is bounded;
        // keep the checked operation to make the invariant explicit.
        let after = debited
            .checked_add(intent.amount)
            .ok_or(TransferError::RecipientOverflow)?;
        (after, after)
    } else {
        let recipient = snapshot
            .recipient_balance
            .checked_add(intent.amount)
            .ok_or(TransferError::RecipientOverflow)?;
        (debited, recipient)
    };
    Ok(TransferDelta {
        tx_hash: intent.tx_hash,
        payer: BalanceDelta {
            account: intent.from,
            before: snapshot.payer_balance,
            after: payer_after,
        },
        recipient: BalanceDelta {
            account: intent.to,
            before: snapshot.recipient_balance,
            after: recipient_after,
        },
        nonce_identity: intent.nonce_identity.clone(),
        nonce_before: snapshot.next_nonce,
        nonce_after,
        fee_funding_delta: intent.approved_fee,
    })
}

/// Validate the ordered reduction before publishing any balance or nonce delta.
/// Fee distribution, buckets, receipts and hashes remain the caller's responsibility.
/// `funding_before` belongs to that policy; this function creates no new ledger.
pub fn checked_fee_funding_after(
    funding_before: u128,
    ordered_deltas: &[TransferDelta],
) -> Result<u128, TransferError> {
    ordered_deltas
        .iter()
        .try_fold(funding_before, |total, delta| {
            total
                .checked_add(delta.fee_funding_delta)
                .ok_or(TransferError::FeeFundingOverflow)
        })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AccessKey {
    NovBalance(Account),
    SignerNonce(String),
}

/// A conservative read/write set, sorted and deduplicated independently of
/// randomized hashing. It is limited to the two NOV balances and signer nonce.
pub fn access_set(intent: &TransferIntent) -> Vec<AccessKey> {
    BTreeSet::from([
        AccessKey::NovBalance(intent.from),
        AccessKey::NovBalance(intent.to),
        AccessKey::SignerNonce(intent.nonce_identity.clone()),
    ])
    .into_iter()
    .collect()
}

/// Earliest safe waves retaining every pair's original conflict order.
///
/// All tasks in a wave have disjoint read/write sets. A task is placed strictly
/// after the latest earlier task touching any of its keys. This also retains
/// transitive dependencies; a greedy "first non-conflicting wave" algorithm
/// could incorrectly move a later task before one of its earlier dependencies.
/// These are planning results, not evidence that a runtime executed in parallel.
pub fn conflict_waves(intents: &[TransferIntent]) -> Vec<Vec<usize>> {
    let mut latest_wave = BTreeMap::<AccessKey, usize>::new();
    let mut waves = Vec::<Vec<usize>>::new();
    for (index, intent) in intents.iter().enumerate() {
        let keys = access_set(intent);
        let wave = keys
            .iter()
            .filter_map(|key| latest_wave.get(key).map(|previous| previous + 1))
            .max()
            .unwrap_or(0);
        if waves.len() <= wave {
            waves.resize_with(wave + 1, Vec::new);
        }
        waves[wave].push(index);
        for key in keys {
            latest_wave.insert(key, wave);
        }
    }
    waves
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(from: u8, to: u8, nonce: u64) -> TransferIntent {
        TransferIntent {
            tx_hash: [from; 32],
            from: [from; 20],
            to: [to; 20],
            nonce_identity: format!("authenticated-signer-{from}"),
            nonce,
            amount: 30,
            approved_fee: 7,
            fee_cap: 7,
        }
    }

    fn snapshot() -> TransferSnapshot {
        TransferSnapshot {
            payer_balance: 100,
            recipient_balance: 20,
            next_nonce: 0,
        }
    }

    #[test]
    fn transfer_delta_conserves_nov_including_fee_and_keeps_input_immutable() {
        let input = intent(1, 2, 0);
        let state = snapshot();
        let original_state = state;
        let delta = compute_delta(&input, &state).unwrap();
        assert_eq!(delta.tx_hash, input.tx_hash);
        assert_eq!((delta.payer.before, delta.payer.after), (100, 63));
        assert_eq!((delta.recipient.before, delta.recipient.after), (20, 50));
        assert_eq!((delta.nonce_before, delta.nonce_after), (0, 1));
        assert_eq!(delta.nonce_identity, input.nonce_identity);
        assert_eq!(delta.fee_funding_delta, 7);
        assert_eq!(100 + 20, delta.payer.after + delta.recipient.after + 7);
        assert_eq!(state, original_state);
    }

    #[test]
    fn self_transfer_is_one_balance_delta_and_only_net_fee_is_debited() {
        let input = intent(1, 1, 0);
        let state = TransferSnapshot {
            recipient_balance: 100,
            ..snapshot()
        };
        let delta = compute_delta(&input, &state).unwrap();
        assert_eq!(delta.payer, delta.recipient);
        assert_eq!(delta.payer.after, 93);
        assert_eq!(delta.payer.after + delta.fee_funding_delta, 100);
        assert_eq!(access_set(&input).len(), 2);
        let inconsistent = TransferSnapshot {
            recipient_balance: 99,
            ..state
        };
        assert_eq!(
            compute_delta(&input, &inconsistent),
            Err(TransferError::InconsistentSelfBalance)
        );
    }

    #[test]
    fn insufficient_balance_never_produces_partial_delta() {
        let input = intent(1, 2, 0);
        let state = TransferSnapshot {
            payer_balance: 36,
            ..snapshot()
        };
        assert_eq!(
            compute_delta(&input, &state),
            Err(TransferError::InsufficientFunds {
                available: 36,
                required: 37
            })
        );
        assert_eq!(state.payer_balance, 36);
        let exact = TransferSnapshot {
            payer_balance: 37,
            ..state
        };
        assert_eq!(compute_delta(&input, &exact).unwrap().payer.after, 0);
        let self_input = intent(1, 1, 0);
        let self_state = TransferSnapshot {
            payer_balance: 36,
            recipient_balance: 36,
            next_nonce: 0,
        };
        assert!(matches!(
            compute_delta(&self_input, &self_state),
            Err(TransferError::InsufficientFunds { .. })
        ));
    }

    #[test]
    fn overflow_is_rejected_and_never_saturated() {
        let mut input = intent(1, 2, 0);
        input.amount = u128::MAX;
        let huge = TransferSnapshot {
            payer_balance: u128::MAX,
            recipient_balance: 0,
            next_nonce: 0,
        };
        assert_eq!(
            compute_delta(&input, &huge),
            Err(TransferError::DebitOverflow)
        );
        input.approved_fee = 0;
        assert_eq!(
            compute_delta(&input, &huge).unwrap().recipient.after,
            u128::MAX
        );
        let recipient_full = TransferSnapshot {
            recipient_balance: 1,
            ..huge
        };
        assert_eq!(
            compute_delta(&input, &recipient_full),
            Err(TransferError::RecipientOverflow)
        );
    }

    #[test]
    fn maximum_self_transfer_does_not_add_twice() {
        let mut input = intent(1, 1, 0);
        input.amount = u128::MAX;
        input.approved_fee = 0;
        let state = TransferSnapshot {
            payer_balance: u128::MAX,
            recipient_balance: u128::MAX,
            next_nonce: 0,
        };
        let delta = compute_delta(&input, &state).unwrap();
        assert_eq!(delta.payer.after, u128::MAX);
        assert_eq!(delta.payer, delta.recipient);
    }

    #[test]
    fn nonce_identity_sequence_and_exhaustion_are_checked() {
        let mut input = intent(1, 2, 1);
        assert_eq!(
            compute_delta(&input, &snapshot()),
            Err(TransferError::NonceMismatch {
                expected: 0,
                provided: 1
            })
        );
        input.nonce = u64::MAX;
        let exhausted = TransferSnapshot {
            next_nonce: u64::MAX,
            ..snapshot()
        };
        assert_eq!(
            compute_delta(&input, &exhausted),
            Err(TransferError::NonceExhausted)
        );
        input.nonce = u64::MAX - 1;
        let last = TransferSnapshot {
            next_nonce: u64::MAX - 1,
            ..snapshot()
        };
        assert_eq!(compute_delta(&input, &last).unwrap().nonce_after, u64::MAX);
        input.nonce_identity.clear();
        assert_eq!(
            compute_delta(&input, &last),
            Err(TransferError::MissingNonceIdentity)
        );
    }

    #[test]
    fn fee_cap_is_explicit_and_zero_is_not_an_unlimited_sentinel() {
        let mut input = intent(1, 2, 0);
        assert!(compute_delta(&input, &snapshot()).is_ok());
        input.fee_cap = 6;
        assert_eq!(
            compute_delta(&input, &snapshot()),
            Err(TransferError::FeeCapExceeded {
                approved_fee: 7,
                fee_cap: 6
            })
        );
        input.fee_cap = 0;
        assert!(matches!(
            compute_delta(&input, &snapshot()),
            Err(TransferError::FeeCapExceeded { .. })
        ));
        input.approved_fee = 0;
        input.amount = 0;
        let delta = compute_delta(&input, &snapshot()).unwrap();
        assert_eq!(delta.payer.before, delta.payer.after);
        assert_eq!(delta.recipient.before, delta.recipient.after);
        assert_eq!(delta.fee_funding_delta, 0);
    }

    #[test]
    fn fee_can_exceed_transfer_amount_without_inventing_an_economic_policy() {
        let mut input = intent(1, 2, 0);
        input.amount = 1;
        let delta = compute_delta(&input, &snapshot()).unwrap();
        assert_eq!(delta.payer.after, 92);
        assert_eq!(delta.recipient.after, 21);
        assert_eq!(delta.fee_funding_delta, 7);
    }

    #[test]
    fn ordered_fee_funding_reduction_is_checked_before_publication() {
        let delta = compute_delta(&intent(1, 2, 0), &snapshot()).unwrap();
        let deltas = vec![delta.clone(), delta];
        assert_eq!(checked_fee_funding_after(3, &deltas), Ok(17));
        assert_eq!(
            checked_fee_funding_after(u128::MAX - 14, &deltas),
            Ok(u128::MAX)
        );
        assert_eq!(
            checked_fee_funding_after(u128::MAX - 13, &deltas),
            Err(TransferError::FeeFundingOverflow)
        );
        assert_eq!(checked_fee_funding_after(u128::MAX, &[]), Ok(u128::MAX));
    }

    #[test]
    fn independent_transactions_share_a_wave_despite_shared_fee_funding() {
        let intents = vec![intent(1, 2, 0), intent(3, 4, 0), intent(5, 6, 0)];
        assert_eq!(conflict_waves(&intents), vec![vec![0, 1, 2]]);
        assert_eq!(conflict_waves(&[]), Vec::<Vec<usize>>::new());
    }

    #[test]
    fn shared_payer_recipient_and_nonce_identity_are_conflicts() {
        assert_eq!(
            conflict_waves(&[intent(1, 2, 0), intent(1, 3, 1)]),
            vec![vec![0], vec![1]]
        );
        assert_eq!(
            conflict_waves(&[intent(1, 3, 0), intent(2, 3, 0)]),
            vec![vec![0], vec![1]]
        );
        assert_eq!(
            conflict_waves(&[intent(1, 2, 0), intent(2, 3, 0)]),
            vec![vec![0], vec![1]]
        );
        let first = intent(1, 2, 0);
        let mut second = intent(3, 4, 1);
        second.nonce_identity = first.nonce_identity.clone();
        assert_eq!(conflict_waves(&[first, second]), vec![vec![0], vec![1]]);
    }

    #[test]
    fn transitive_dependencies_cannot_be_greedily_moved_to_an_earlier_wave() {
        let intents = vec![
            intent(1, 2, 0),
            intent(2, 3, 0),
            intent(3, 4, 0),
            intent(5, 6, 0),
        ];
        assert_eq!(conflict_waves(&intents), vec![vec![0, 3], vec![1], vec![2]]);
    }

    #[test]
    fn dense_deterministic_plans_preserve_all_original_conflict_edges() {
        let intents: Vec<_> = (0..128u8)
            .map(|index| intent(index % 17, index.wrapping_mul(7) % 19, u64::from(index)))
            .collect();
        let waves = conflict_waves(&intents);
        assert_eq!(waves, conflict_waves(&intents));
        let mut assigned = vec![None; intents.len()];
        for (wave, tasks) in waves.iter().enumerate() {
            let mut used = BTreeSet::new();
            for index in tasks {
                assert!(assigned[*index].replace(wave).is_none());
                for key in access_set(&intents[*index]) {
                    assert!(used.insert(key), "in-wave key conflict");
                }
            }
        }
        for earlier in 0..intents.len() {
            assert!(assigned[earlier].is_some());
            let keys: BTreeSet<_> = access_set(&intents[earlier]).into_iter().collect();
            for later in earlier + 1..intents.len() {
                if access_set(&intents[later])
                    .iter()
                    .any(|key| keys.contains(key))
                {
                    assert!(assigned[earlier] < assigned[later]);
                }
            }
        }
    }

    #[test]
    fn bounded_inputs_preserve_conservation_for_self_and_distinct_accounts() {
        for payer in 0..24u128 {
            for recipient in 0..12u128 {
                for amount in 0..26u128 {
                    for fee in 0..4u128 {
                        for self_transfer in [false, true] {
                            let mut input = intent(1, if self_transfer { 1 } else { 2 }, 0);
                            input.amount = amount;
                            input.approved_fee = fee;
                            input.fee_cap = fee;
                            let state = TransferSnapshot {
                                payer_balance: payer,
                                recipient_balance: if self_transfer { payer } else { recipient },
                                next_nonce: 0,
                            };
                            match compute_delta(&input, &state) {
                                Ok(delta) => {
                                    assert!(payer >= amount + fee);
                                    if self_transfer {
                                        assert_eq!(delta.payer, delta.recipient);
                                        assert_eq!(payer, delta.payer.after + fee);
                                    } else {
                                        assert_eq!(
                                            payer + recipient,
                                            delta.payer.after + delta.recipient.after + fee
                                        );
                                    }
                                }
                                Err(TransferError::InsufficientFunds { .. }) => {
                                    assert!(payer < amount + fee)
                                }
                                Err(error) => {
                                    panic!("unexpected bounded arithmetic error: {error}")
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
