#![forbid(unsafe_code)]

//! Checked NOV arithmetic for an authenticated, already quoted transfer.
//!
//! Migrated locally from `native_transfer_delta.rs` at repository baseline
//! `161f64d` (`legacy/supervm-20261002/crates/novovm-node/src/`). No legacy
//! module, Store, finalizer, scheduler or serialization format is imported.
//!
//! Outcomes are **pre-global-settlement**, not complete transaction receipts.
//! The caller must bind the signed intent, signer identity, parent state and
//! fee policy. Treasury distribution, global settlement rejection, dependent
//! outcome repair, persistence and publication are outside this operator.
//! An outcome is not an authorization token or a business validity proof.

/// Exact balance identity: 20-byte and 32-byte accounts are never aliased.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Account(Vec<u8>);

impl Account {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    fn hex(&self) -> String {
        use std::fmt::Write;
        let mut result = String::with_capacity(2 + 2 * self.0.len());
        result.push_str("0x");
        for byte in &self.0 {
            write!(&mut result, "{byte:02x}").expect("writing to String cannot fail");
        }
        result
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferIntent {
    pub tx_hash: [u8; 32],
    pub from: Account,
    pub to: Account,
    /// Caller-authenticated signer nonce identity, not the balance account.
    pub nonce_identity: String,
    pub nonce: u64,
    pub amount: u128,
    /// Integer base units from the existing fee policy, not a rate estimate.
    pub approved_fee: u128,
    /// Resolved cap: zero means zero, never an unlimited/automatic sentinel.
    pub fee_cap: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferSnapshot {
    pub payer_balance: u128,
    pub recipient_balance: u128,
    pub next_nonce: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceDelta {
    pub account: Account,
    pub before: u128,
    pub after: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferDelta {
    pub tx_hash: [u8; 32],
    pub payer: BalanceDelta,
    /// Equals payer for self-transfers; that balance must be applied only once.
    pub recipient: BalanceDelta,
    pub nonce_identity: String,
    pub nonce_before: u64,
    pub nonce_after: u64,
    /// Pending funding, not evidence that any treasury bucket was credited.
    pub fee_funding_delta: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNonceIdentity => f.write_str("missing signer nonce identity"),
            Self::NonceMismatch { expected, provided } => {
                write!(f, "nonce mismatch: expected {expected}, got {provided}")
            }
            Self::NonceExhausted => f.write_str("signer nonce exhausted"),
            Self::FeeCapExceeded {
                approved_fee,
                fee_cap,
            } => {
                write!(f, "approved fee {approved_fee} exceeds cap {fee_cap}")
            }
            Self::DebitOverflow => f.write_str("transfer amount plus fee overflows"),
            Self::InsufficientFunds {
                available,
                required,
            } => {
                write!(
                    f,
                    "insufficient NOV: available {available}, required {required}"
                )
            }
            Self::RecipientOverflow => f.write_str("recipient NOV balance overflows"),
            Self::InconsistentSelfBalance => f.write_str("self-transfer balance views disagree"),
            Self::FeeFundingOverflow => f.write_str("fee funding reduction overflows"),
        }
    }
}

impl std::error::Error for TransferError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferFailure {
    Fee(String),
    Business(TransferError),
}

/// Only the checked operator constructs an outcome. There is deliberately no
/// Deserialize implementation accepting externally manufactured transitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferOutcome {
    delta: TransferDelta,
    failure: Option<TransferFailure>,
}

impl TransferOutcome {
    pub fn delta(&self) -> &TransferDelta {
        &self.delta
    }

    pub fn failure(&self) -> Option<&TransferFailure> {
        self.failure.as_ref()
    }

    pub fn is_success(&self) -> bool {
        self.failure.is_none()
    }

    /// Replace speculative money effects after a real global fee rejection.
    /// The valid nonce survives; dependent predictions must be revalidated by
    /// the caller. This does not itself settle a fee or repair later outcomes.
    pub fn reject_fee(mut self, reason: String) -> Self {
        self.delta.payer.after = self.delta.payer.before;
        self.delta.recipient.after = self.delta.recipient.before;
        self.delta.fee_funding_delta = 0;
        self.failure = Some(TransferFailure::Fee(reason));
        self
    }
}

fn next_nonce(intent: &TransferIntent, state: &TransferSnapshot) -> Result<u64, TransferError> {
    if intent.nonce_identity.is_empty() {
        return Err(TransferError::MissingNonceIdentity);
    }
    if intent.nonce != state.next_nonce {
        return Err(TransferError::NonceMismatch {
            expected: state.next_nonce,
            provided: intent.nonce,
        });
    }
    intent
        .nonce
        .checked_add(1)
        .ok_or(TransferError::NonceExhausted)
}

/// Full success arithmetic only. Errors here are not executed-failure receipts;
/// use `compute_outcome` to preserve fee and nonce effects on business failure.
/// Self-transfer affordability still covers amount plus fee before netting.
pub fn compute_delta(
    intent: &TransferIntent,
    state: &TransferSnapshot,
) -> Result<TransferDelta, TransferError> {
    let nonce_after = next_nonce(intent, state)?;
    if intent.approved_fee > intent.fee_cap {
        return Err(TransferError::FeeCapExceeded {
            approved_fee: intent.approved_fee,
            fee_cap: intent.fee_cap,
        });
    }
    let same = intent.from == intent.to;
    if same && state.payer_balance != state.recipient_balance {
        return Err(TransferError::InconsistentSelfBalance);
    }
    let debit = intent
        .amount
        .checked_add(intent.approved_fee)
        .ok_or(TransferError::DebitOverflow)?;
    let paid = state
        .payer_balance
        .checked_sub(debit)
        .ok_or(TransferError::InsufficientFunds {
            available: state.payer_balance,
            required: debit,
        })?;
    let (payer_after, recipient_after) = if same {
        let after = paid
            .checked_add(intent.amount)
            .ok_or(TransferError::RecipientOverflow)?;
        (after, after)
    } else {
        (
            paid,
            state
                .recipient_balance
                .checked_add(intent.amount)
                .ok_or(TransferError::RecipientOverflow)?,
        )
    };
    Ok(TransferDelta {
        tx_hash: intent.tx_hash,
        payer: BalanceDelta {
            account: intent.from.clone(),
            before: state.payer_balance,
            after: payer_after,
        },
        recipient: BalanceDelta {
            account: intent.to.clone(),
            before: state.recipient_balance,
            after: recipient_after,
        },
        nonce_identity: intent.nonce_identity.clone(),
        nonce_before: state.next_nonce,
        nonce_after,
        fee_funding_delta: intent.approved_fee,
    })
}

/// Evaluate a pre-fee snapshot. The caller supplies quotation/policy failures
/// (including the slippage-inclusive cap); this operator does not quote fees.
/// Invalid input is Err, whereas authenticated economic failure is an Outcome.
pub fn compute_outcome(
    intent: &TransferIntent,
    state: &TransferSnapshot,
    fee_rejection: Option<&str>,
) -> Result<TransferOutcome, TransferError> {
    let nonce_after = next_nonce(intent, state)?;
    if intent.from == intent.to && state.payer_balance != state.recipient_balance {
        return Err(TransferError::InconsistentSelfBalance);
    }
    let mut outcome = TransferOutcome {
        delta: TransferDelta {
            tx_hash: intent.tx_hash,
            payer: BalanceDelta {
                account: intent.from.clone(),
                before: state.payer_balance,
                after: state.payer_balance,
            },
            recipient: BalanceDelta {
                account: intent.to.clone(),
                before: state.recipient_balance,
                after: state.recipient_balance,
            },
            nonce_identity: intent.nonce_identity.clone(),
            nonce_before: state.next_nonce,
            nonce_after,
            fee_funding_delta: 0,
        },
        failure: None,
    };
    if let Some(reason) = fee_rejection {
        return Ok(outcome.reject_fee(reason.to_owned()));
    }
    if intent.approved_fee > intent.fee_cap {
        return Ok(outcome.reject_fee(format!(
            "fee.quote.max_pay_exceeded: approved_fee={} max_pay_amount={} pay_asset=NOV",
            intent.approved_fee, intent.fee_cap
        )));
    }
    let Some(after_fee) = state.payer_balance.checked_sub(intent.approved_fee) else {
        return Ok(outcome.reject_fee(format!(
            "fee.clearing.insufficient_user_balance: nov_fee_asset_debit_failed: account={} requested={} available={}",
            intent.from.hex(), intent.approved_fee, state.payer_balance
        )));
    };
    match compute_delta(intent, state) {
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
            outcome.failure = Some(TransferFailure::Business(error));
        }
        Err(error) => return Err(error),
    }
    Ok(outcome)
}

/// Check pending funding arithmetic, not global settlement or distribution.
/// No balance/nonce change should be published on aggregate overflow.
pub fn checked_fee_funding_after(
    funding_before: u128,
    ordered_deltas: &[TransferDelta],
) -> Result<u128, TransferError> {
    ordered_deltas
        .iter()
        .try_fold(funding_before, |sum, delta| {
            sum.checked_add(delta.fee_funding_delta)
                .ok_or(TransferError::FeeFundingOverflow)
        })
}

#[cfg(test)]
mod tests;
