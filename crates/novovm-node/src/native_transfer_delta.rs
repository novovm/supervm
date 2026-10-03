#![forbid(unsafe_code)]

//! Pure, bounded NOV transfer arithmetic and conflict planning.
//!
//! This is not an ingress, fee schedule, authorization check, or a new ledger.
//! The caller authenticates the intent and supplies its already approved fee and
//! effective fee cap. A zero cap means zero here; any wire-level sentinel and
//! slippage requirement must be resolved by the caller. Authenticated fee and
//! business failures consume their valid nonce; malformed state/nonce inputs
//! remain errors. All amounts are integer base units, not gas units.
//!
//! The only per-task state is two account balances and one signer nonce. Shared
//! fee funding contributions are reduced with checked arithmetic in original
//! transaction order after execution; they are not a false dependency between
//! otherwise independent tasks. A rejected global settlement replaces that
//! transaction with a nonce-only outcome. Component predictions are therefore
//! checked against the ordered state before they may be applied.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Preserve the exact supported balance identity. A public-key account is not
/// truncated or silently aliased to its derived 20-byte address.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Account(Vec<u8>);

impl Account {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_hex_prefixed(&self) -> String {
        use std::fmt::Write;
        let mut encoded = String::with_capacity(2 + self.0.len() * 2);
        encoded.push_str("0x");
        for byte in &self.0 {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
        }
        encoded
    }
}

impl TryFrom<Vec<u8>> for Account {
    type Error = &'static str;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        if !matches!(bytes.len(), 20 | 32) {
            return Err("native transfer account must contain exactly 20 or 32 bytes");
        }
        Ok(Self(bytes))
    }
}

impl TryFrom<&[u8]> for Account {
    type Error = &'static str;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if !matches!(bytes.len(), 20 | 32) {
            return Err("native transfer account must contain exactly 20 or 32 bytes");
        }
        Ok(Self(bytes.to_vec()))
    }
}

impl From<[u8; 20]> for Account {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes.to_vec())
    }
}

impl From<[u8; 32]> for Account {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes.to_vec())
    }
}

impl<'de> Deserialize<'de> for Account {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AccountVisitor;
        impl<'de> serde::de::Visitor<'de> for AccountVisitor {
            type Value = Account;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("exactly 20 or 32 account bytes")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Account, A::Error> {
                if seq.size_hint().is_some_and(|len| len > 32) {
                    return Err(serde::de::Error::custom(
                        "native transfer account exceeds 32 bytes",
                    ));
                }
                let mut bytes = Vec::with_capacity(32);
                while let Some(byte) = seq.next_element::<u8>()? {
                    if bytes.len() == 32 {
                        return Err(serde::de::Error::custom(
                            "native transfer account exceeds 32 bytes",
                        ));
                    }
                    bytes.push(byte);
                }
                Account::try_from(bytes).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_seq(AccountVisitor)
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Quotation/policy errors retain the caller's established receipt error code.
/// Business errors occur only after a locally affordable fee was accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TransferExecutionFailureV1 {
    Fee(String),
    Business(TransferError),
}

/// Only validated execution creates an outcome. The fields are private so a
/// caller cannot construct an apparent nonce transition without validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransferExecutionOutcomeV1 {
    delta: TransferDelta,
    failure: Option<TransferExecutionFailureV1>,
}

impl TransferExecutionOutcomeV1 {
    pub(crate) fn delta(&self) -> &TransferDelta {
        &self.delta
    }

    pub(crate) fn failure(&self) -> Option<&TransferExecutionFailureV1> {
        self.failure.as_ref()
    }

    pub(crate) fn is_success(&self) -> bool {
        self.failure.is_none()
    }

    /// Global settlement is reduced in original order. A rejected fee erases
    /// the speculative business and fee writes, not the authenticated nonce.
    /// Every replacement uses the original pre-fee balances, even if the
    /// speculative result was already a fee-paid business failure.
    pub(crate) fn reject_fee(mut self, reason: String) -> Self {
        self.delta.payer.after = self.delta.payer.before;
        self.delta.recipient.after = self.delta.recipient.before;
        self.delta.fee_funding_delta = 0;
        self.failure = Some(TransferExecutionFailureV1::Fee(reason));
        self
    }
}

fn validate_snapshot(
    intent: &TransferIntent,
    snapshot: &TransferSnapshot,
) -> Result<u64, TransferError> {
    if intent.nonce_identity.is_empty() {
        return Err(TransferError::MissingNonceIdentity);
    }
    if intent.nonce != snapshot.next_nonce {
        return Err(TransferError::NonceMismatch {
            expected: snapshot.next_nonce,
            provided: intent.nonce,
        });
    }
    let next = intent
        .nonce
        .checked_add(1)
        .ok_or(TransferError::NonceExhausted)?;
    if intent.from == intent.to && snapshot.payer_balance != snapshot.recipient_balance {
        return Err(TransferError::InconsistentSelfBalance);
    }
    Ok(next)
}

/// Execute from a pre-fee snapshot. The caller authenticates first and supplies
/// quotation failures (including slippage-inclusive cap failure) separately;
/// `fee_cap` is a final defense, not a replacement for the unified quote.
pub(crate) fn compute_outcome_v1(
    intent: &TransferIntent,
    snapshot: &TransferSnapshot,
    fee_rejection: Option<&str>,
) -> Result<TransferExecutionOutcomeV1, TransferError> {
    let nonce_after = validate_snapshot(intent, snapshot)?;
    let unchanged = TransferDelta {
        tx_hash: intent.tx_hash,
        payer: BalanceDelta {
            account: intent.from.clone(),
            before: snapshot.payer_balance,
            after: snapshot.payer_balance,
        },
        recipient: BalanceDelta {
            account: intent.to.clone(),
            before: snapshot.recipient_balance,
            after: snapshot.recipient_balance,
        },
        nonce_identity: intent.nonce_identity.clone(),
        nonce_before: snapshot.next_nonce,
        nonce_after,
        fee_funding_delta: 0,
    };
    let mut outcome = TransferExecutionOutcomeV1 {
        delta: unchanged,
        failure: None,
    };
    if let Some(reason) = fee_rejection {
        return Ok(outcome.reject_fee(reason.to_string()));
    }
    if intent.approved_fee > intent.fee_cap {
        return Ok(outcome.reject_fee(format!(
            "fee.quote.max_pay_exceeded: approved_fee={} max_pay_amount={} pay_asset=NOV",
            intent.approved_fee, intent.fee_cap
        )));
    }
    let Some(after_fee) = snapshot.payer_balance.checked_sub(intent.approved_fee) else {
        return Ok(outcome.reject_fee(format!(
            "fee.clearing.insufficient_user_balance: nov_fee_asset_debit_failed: account={} requested={} available={}",
            intent.from.to_hex_prefixed(), intent.approved_fee, snapshot.payer_balance
        )));
    };
    match compute_delta(intent, snapshot) {
        Ok(delta) => outcome.delta = delta,
        Err(
            error @ (TransferError::DebitOverflow
            | TransferError::InsufficientFunds { .. }
            | TransferError::RecipientOverflow),
        ) => {
            outcome.delta.payer.after = after_fee;
            if intent.from == intent.to {
                outcome.delta.recipient.after = after_fee;
            }
            outcome.delta.fee_funding_delta = intent.approved_fee;
            outcome.failure = Some(TransferExecutionFailureV1::Business(error));
        }
        Err(error) => return Err(error),
    }
    Ok(outcome)
}

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
            account: intent.from.clone(),
            before: snapshot.payer_balance,
            after: payer_after,
        },
        recipient: BalanceDelta {
            account: intent.to.clone(),
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
        AccessKey::NovBalance(intent.from.clone()),
        AccessKey::NovBalance(intent.to.clone()),
        AccessKey::SignerNonce(intent.nonce_identity.clone()),
    ])
    .into_iter()
    .collect()
}

/// Contiguous maximal independent segments in original transaction order.
/// Reduce all outcomes, including rejected global fees, before reading the
/// next segment's snapshots. Moving a later independent transaction ahead of
/// an earlier conflicting segment would break global settlement ordering.
pub fn conflict_segments(intents: &[TransferIntent]) -> Vec<Vec<usize>> {
    let mut used = BTreeSet::<AccessKey>::new();
    let mut segments = Vec::<Vec<usize>>::new();
    for (index, intent) in intents.iter().enumerate() {
        let keys = access_set(intent);
        if segments.is_empty() || keys.iter().any(|key| used.contains(key)) {
            segments.push(Vec::new());
            used.clear();
        }
        segments
            .last_mut()
            .expect("nonempty segment list")
            .push(index);
        used.extend(keys);
    }
    segments
}

/// Disjoint connected components of the exact balance/nonce access graph.
/// Components and their members are ordered by original transaction index;
/// only computation may run out of order, never settlement or publication.
/// A bridge touching two prior components joins both, even when noncontiguous.
/// This is deliberately not a maximal-parallelism DAG scheduler: each connected
/// component is computed sequentially on one AOEM callback.
pub(crate) fn conflict_components_v1(intents: &[TransferIntent]) -> Vec<Vec<usize>> {
    conflict_components_excluding_credits_v1(intents, &BTreeSet::new())
}

/// Only the checked effect planner may remove credit-only keys. Payer and
/// nonce dependencies are never removed, including self-transfers.
fn conflict_components_excluding_credits_v1(
    intents: &[TransferIntent],
    credit_keys: &BTreeSet<Account>,
) -> Vec<Vec<usize>> {
    fn root(parents: &mut [usize], mut index: usize) -> usize {
        while parents[index] != index {
            parents[index] = parents[parents[index]];
            index = parents[index];
        }
        index
    }
    let mut parents: Vec<_> = (0..intents.len()).collect();
    let mut previous = BTreeMap::new();
    for (index, intent) in intents.iter().enumerate() {
        for key in access_set(intent) {
            if matches!(&key, AccessKey::NovBalance(account)
                if account != &intent.from && credit_keys.contains(account))
            {
                continue;
            }
            if let Some(earlier) = previous.insert(key, index) {
                let a = root(&mut parents, earlier);
                let b = root(&mut parents, index);
                parents[a.max(b)] = a.min(b);
            }
        }
    }
    let mut components = BTreeMap::<usize, Vec<usize>>::new();
    for index in 0..intents.len() {
        components
            .entry(root(&mut parents, index))
            .or_default()
            .push(index);
    }
    components.into_values().collect()
}

#[path = "native_transfer_effects.rs"]
pub(crate) mod effects;

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(from: u8, to: u8, nonce: u64) -> TransferIntent {
        TransferIntent {
            tx_hash: [from; 32],
            from: [from; 20].into(),
            to: [to; 20].into(),
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
    fn account_identity_is_bounded_and_preserves_twenty_and_thirty_two_byte_keys() {
        for len in [0, 1, 19, 21, 31, 33, 128] {
            assert!(Account::try_from(vec![7; len]).is_err());
            let encoded = serde_json::to_vec(&vec![7u8; len]).unwrap();
            assert!(serde_json::from_slice::<Account>(&encoded).is_err());
        }
        let short = Account::from([7; 20]);
        let long = Account::from([7; 32]);
        assert_ne!(short, long);
        for account in [short, long] {
            assert_eq!(
                account.to_hex_prefixed(),
                format!("0x{}", "07".repeat(account.as_bytes().len()))
            );
            assert_eq!(Account::try_from(account.as_bytes()).unwrap(), account);
            let encoded = serde_json::to_vec(&account).unwrap();
            assert_eq!(
                serde_json::from_slice::<Account>(&encoded).unwrap(),
                account
            );
        }
        let mut input = intent(1, 2, 0);
        input.from = [1; 32].into();
        input.to = [2; 32].into();
        let outcome = compute_outcome_v1(&input, &snapshot(), None).unwrap();
        assert_eq!(outcome.delta().payer.account.as_bytes(), &[1; 32]);
        assert_eq!(outcome.delta().recipient.account.as_bytes(), &[2; 32]);
    }

    #[test]
    fn quoted_cap_and_insufficient_fee_failures_consume_nonce_without_payment() {
        let mut input = intent(1, 2, 0);
        let state = snapshot();
        let quote_reason = "fee.quote.max_pay_exceeded: required_with_slippage=8 max_pay_amount=7";
        // Base fee fits, but the unified quote's slippage-inclusive cap does not.
        let rejected = compute_outcome_v1(&input, &state, Some(quote_reason)).unwrap();
        assert_eq!(
            rejected.failure(),
            Some(&TransferExecutionFailureV1::Fee(quote_reason.into()))
        );
        input.fee_cap = 6;
        let cap_rejected = compute_outcome_v1(&input, &state, None).unwrap();
        input.fee_cap = 7;
        let fee_short = TransferSnapshot {
            payer_balance: 6,
            ..state
        };
        let unpaid = compute_outcome_v1(&input, &fee_short, None).unwrap();
        for outcome in [rejected, cap_rejected, unpaid] {
            assert!(matches!(
                outcome.failure(),
                Some(TransferExecutionFailureV1::Fee(_))
            ));
            let delta = outcome.delta();
            assert_eq!(delta.payer.before, delta.payer.after);
            assert_eq!(delta.recipient.before, delta.recipient.after);
            assert_eq!(delta.fee_funding_delta, 0);
            assert_eq!((delta.nonce_before, delta.nonce_after), (0, 1));
        }
    }

    #[test]
    fn business_failures_keep_only_the_fee_and_valid_nonce() {
        let input = intent(1, 2, 0);
        for state in [
            TransferSnapshot {
                payer_balance: 7,
                ..snapshot()
            },
            TransferSnapshot {
                payer_balance: 36,
                ..snapshot()
            },
            TransferSnapshot {
                recipient_balance: u128::MAX,
                ..snapshot()
            },
        ] {
            let outcome = compute_outcome_v1(&input, &state, None).unwrap();
            assert!(matches!(
                outcome.failure(),
                Some(TransferExecutionFailureV1::Business(_))
            ));
            assert_eq!(outcome.delta().payer.after, state.payer_balance - 7);
            assert_eq!(outcome.delta().recipient.after, state.recipient_balance);
            assert_eq!(outcome.delta().fee_funding_delta, 7);
            assert_eq!(outcome.delta().nonce_after, 1);
            // Global settlement rejection must use the original, not post-fee, state.
            let rejected = outcome.reject_fee("fee.settlement.amount_overflow".into());
            assert_eq!(rejected.delta().payer.after, state.payer_balance);
            assert_eq!(rejected.delta().recipient.after, state.recipient_balance);
            assert_eq!(rejected.delta().fee_funding_delta, 0);
            assert_eq!(rejected.delta().nonce_after, 1);
        }
        let mut overflowing = input;
        overflowing.amount = u128::MAX;
        let state = TransferSnapshot {
            payer_balance: u128::MAX,
            recipient_balance: 0,
            next_nonce: 0,
        };
        let overflow = compute_outcome_v1(&overflowing, &state, None).unwrap();
        assert_eq!(
            overflow.failure(),
            Some(&TransferExecutionFailureV1::Business(
                TransferError::DebitOverflow
            ))
        );
        assert_eq!(overflow.delta().payer.after, u128::MAX - 7);
    }

    #[test]
    fn self_transfer_failure_and_settlement_rejection_keep_one_balance() {
        let input = intent(1, 1, 0);
        for balance in [6, 7, 36, 37, 100] {
            let state = TransferSnapshot {
                payer_balance: balance,
                recipient_balance: balance,
                next_nonce: 0,
            };
            let outcome = compute_outcome_v1(&input, &state, None).unwrap();
            assert_eq!(outcome.delta().payer, outcome.delta().recipient);
            assert_eq!(
                outcome.delta().payer.after + outcome.delta().fee_funding_delta,
                balance
            );
            let rejected = outcome.reject_fee("fee.settlement.settlement_paused".into());
            assert_eq!(rejected.delta().payer, rejected.delta().recipient);
            assert_eq!(rejected.delta().payer.after, balance);
            assert_eq!(rejected.delta().nonce_after, 1);
        }
    }

    #[test]
    fn quote_failure_cannot_turn_invalid_nonce_or_state_into_an_executed_failure() {
        let state = snapshot();
        let mut input = intent(1, 2, 1);
        assert!(matches!(
            compute_outcome_v1(&input, &state, Some("quote rejected")),
            Err(TransferError::NonceMismatch { .. })
        ));
        input.nonce = u64::MAX;
        assert_eq!(
            compute_outcome_v1(
                &input,
                &TransferSnapshot {
                    next_nonce: u64::MAX,
                    ..state
                },
                Some("quote rejected")
            ),
            Err(TransferError::NonceExhausted)
        );
        input.nonce = 0;
        input.nonce_identity.clear();
        assert_eq!(
            compute_outcome_v1(&input, &state, None),
            Err(TransferError::MissingNonceIdentity)
        );
        let self_input = intent(1, 1, 0);
        assert_eq!(
            compute_outcome_v1(&self_input, &state, Some("quote rejected")),
            Err(TransferError::InconsistentSelfBalance)
        );
    }

    #[test]
    fn next_segment_uses_reduced_state_after_a_global_fee_rejection() {
        let first = intent(1, 2, 0);
        let mut dependent = intent(2, 3, 0);
        dependent.amount = 30;
        let independent = intent(4, 5, 0);
        assert_eq!(
            conflict_segments(&[first.clone(), dependent.clone(), independent]),
            vec![vec![0], vec![1, 2]]
        );
        let speculative = compute_outcome_v1(&first, &snapshot(), None).unwrap();
        assert_eq!(speculative.delta().recipient.after, 50);
        let settled = speculative.reject_fee("fee.settlement.amount_overflow".into());
        let fresh_snapshot = TransferSnapshot {
            payer_balance: settled.delta().recipient.after,
            recipient_balance: 0,
            next_nonce: 0,
        };
        let next = compute_outcome_v1(&dependent, &fresh_snapshot, None).unwrap();
        assert!(matches!(
            next.failure(),
            Some(TransferExecutionFailureV1::Business(
                TransferError::InsufficientFunds { .. }
            ))
        ));
        assert_eq!(next.delta().payer.after, 13);
        assert_eq!(next.delta().recipient.after, 0);
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
    fn independent_transactions_share_a_segment_despite_shared_fee_funding() {
        let intents = vec![intent(1, 2, 0), intent(3, 4, 0), intent(5, 6, 0)];
        assert_eq!(conflict_segments(&intents), vec![vec![0, 1, 2]]);
        assert_eq!(conflict_segments(&[]), Vec::<Vec<usize>>::new());
    }

    #[test]
    fn shared_payer_recipient_and_nonce_identity_are_conflicts() {
        assert_eq!(
            conflict_segments(&[intent(1, 2, 0), intent(1, 3, 1)]),
            vec![vec![0], vec![1]]
        );
        assert_eq!(
            conflict_segments(&[intent(1, 3, 0), intent(2, 3, 0)]),
            vec![vec![0], vec![1]]
        );
        assert_eq!(
            conflict_segments(&[intent(1, 2, 0), intent(2, 3, 0)]),
            vec![vec![0], vec![1]]
        );
        let first = intent(1, 2, 0);
        let mut second = intent(3, 4, 1);
        second.nonce_identity = first.nonce_identity.clone();
        assert_eq!(conflict_segments(&[first, second]), vec![vec![0], vec![1]]);
    }

    #[test]
    fn later_independent_work_does_not_jump_earlier_global_fee_reduction() {
        let intents = vec![
            intent(1, 2, 0),
            intent(2, 3, 0),
            intent(3, 4, 0),
            intent(5, 6, 0),
        ];
        assert_eq!(
            conflict_segments(&intents),
            vec![vec![0], vec![1], vec![2, 3]]
        );
    }

    #[test]
    fn dense_deterministic_plans_preserve_all_original_conflict_edges() {
        let intents: Vec<_> = (0..128u8)
            .map(|index| intent(index % 17, index.wrapping_mul(7) % 19, u64::from(index)))
            .collect();
        let waves = conflict_segments(&intents);
        assert_eq!(waves, conflict_segments(&intents));
        assert_eq!(waves.concat(), (0..intents.len()).collect::<Vec<_>>());
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
    fn components_join_noncontiguous_bridges_but_preserve_order() {
        let intents = vec![
            intent(1, 2, 0),
            intent(3, 4, 0),
            intent(5, 6, 0),
            intent(2, 3, 0),
            intent(1, 4, 1),
            intent(5, 6, 1),
        ];
        assert_eq!(
            conflict_components_v1(&intents),
            vec![vec![0, 1, 3, 4], vec![2, 5]]
        );
        assert!(conflict_components_v1(&[]).is_empty());
        assert_eq!(
            conflict_components_v1(&intents),
            conflict_components_v1(&intents)
        );
    }

    #[test]
    fn components_keep_exact_account_width_and_nonce_identity() {
        let first = intent(1, 2, 0);
        let mut wide = intent(3, 4, 0);
        wide.from = [1; 32].into();
        wide.to = [2; 32].into();
        assert_eq!(
            conflict_components_v1(&[first.clone(), wide.clone()]),
            vec![vec![0], vec![1]]
        );
        wide.nonce_identity = first.nonce_identity.clone();
        assert_eq!(conflict_components_v1(&[first, wide]), vec![vec![0, 1]]);
    }

    #[test]
    fn grouped_signer_chains_have_one_component_each_not_one_global_wave_per_nonce() {
        let intents: Vec<_> = (1..=16)
            .flat_map(|signer| (0..8).map(move |nonce| intent(signer, signer + 32, nonce)))
            .collect();
        let components = conflict_components_v1(&intents);
        assert_eq!(components.len(), 16);
        assert!(components.iter().all(|component| component.len() == 8));
        assert_eq!(components.concat(), (0..128).collect::<Vec<_>>());
        assert!(conflict_segments(&intents).len() > components.len());
        for (a, left) in components.iter().enumerate() {
            let keys: BTreeSet<_> = left
                .iter()
                .flat_map(|index| access_set(&intents[*index]))
                .collect();
            for right in &components[a + 1..] {
                assert!(right.iter().all(|index| access_set(&intents[*index])
                    .iter()
                    .all(|key| !keys.contains(key))));
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
