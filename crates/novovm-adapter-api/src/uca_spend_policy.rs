//! Pure policy preflight over CALLER-SUPPLIED state. No constructor here can
//! certify that state is authoritative. Native integration must read account,
//! spend grant, current spend epoch and nonce from the SAME captured parent.
//! Consume nonce/budget atomically with effects through the existing AOEM owner.
//! Never feed an App-provided snapshot into a production authorization decision.
use crate::unified_account::{UcaAccount, UcaStatus};
use novovm_protocol::uca_spend::UcaSpendIntentV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpendPolicyError {
    MalformedIntent,
    StateUnavailable,
    WrongParent,
    WrongDomain,
    AccountInactive,
    WrongAccount,
    WrongKey,
    WrongEpoch,
    NotSpendPurpose,
    Revoked,
    Expired,
    NonceMismatch,
    LimitExceeded,
}

/// Separate from identity management; an admin/communication key has no spend
/// grant merely because it exists in the UCA record.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SuppliedSpendGrantV1 {
    pub account_id: String,
    pub key_ref: [u8; 32],
    pub authorization_epoch: u64,
    pub permits_nov_transfer: bool,
    pub revoked: bool,
    pub not_before_height: u64,
    pub expires_at_height: u64,
    pub max_debit_per_intent: u128,
    /// Remaining authorized budget at this parent/prefix. It is not a balance,
    /// and incoming funds cannot increase it without an authorized grant change.
    pub remaining_debit_budget: u128,
}

pub struct SuppliedSpendStateV1<'a> {
    pub chain_id: u64,
    pub genesis: [u8; 32],
    pub parent_root: [u8; 32],
    pub account: &'a UcaAccount,
    pub grant: &'a SuppliedSpendGrantV1,
    /// Account-wide spending authorization generation at this parent. This is
    /// not the identity-management epoch or an account timestamp. None means
    /// unresolved; it must never be inferred from the submitted grant/intent.
    pub current_spend_epoch: Option<u64>,
    /// Explicit resolved nonce; None is unknown/unavailable, never nonce zero.
    pub next_spend_nonce: Option<u64>,
}

/// Check only the authority facts that remain fixed during a captured spend
/// batch: account, domain, purpose, revocation, current epoch and validity window.
/// This does not examine or reserve nonce, per-intent debit or remaining budget;
/// a later intent may depend on preceding effects in the same execution plan.
/// An Ok result is NOT a spend permit. The caller must establish state provenance,
/// verify the signature and check/consume all dynamic effects in that plan.
pub fn check_supplied_spend_authority_v1(
    intent: &UcaSpendIntentV1,
    state: Option<&SuppliedSpendStateV1<'_>>,
    expected_parent: &[u8; 32],
    execution_height: u64,
) -> Result<(), SpendPolicyError> {
    use SpendPolicyError::*;
    intent.validate().map_err(|_| MalformedIntent)?;
    let s = state.ok_or(StateUnavailable)?;
    if s.parent_root != *expected_parent || *expected_parent == [0; 32] {
        return Err(WrongParent);
    }
    if s.chain_id != intent.chain_id || s.genesis != intent.genesis {
        return Err(WrongDomain);
    }
    if s.account.uca_id != intent.account_id || s.grant.account_id != intent.account_id {
        return Err(WrongAccount);
    }
    if s.account.status != UcaStatus::Active {
        return Err(AccountInactive);
    }
    if s.grant.key_ref != intent.spend_key_ref {
        return Err(WrongKey);
    }
    let current_spend_epoch = s.current_spend_epoch.ok_or(StateUnavailable)?;
    if current_spend_epoch == 0
        || s.grant.authorization_epoch != current_spend_epoch
        || intent.authorization_epoch != current_spend_epoch
    {
        return Err(WrongEpoch);
    }
    if !s.grant.permits_nov_transfer {
        return Err(NotSpendPurpose);
    }
    if s.grant.revoked {
        return Err(Revoked);
    }
    if execution_height < intent.not_before_height
        || execution_height >= intent.expires_at_height
        || execution_height < s.grant.not_before_height
        || execution_height >= s.grant.expires_at_height
    {
        return Err(Expired);
    }
    Ok(())
}

/// An Ok result means only that the supplied values satisfy this policy. Caller
/// must separately establish their provenance, verify the signature, retain the
/// exact parent binding and execute/commit via the authoritative product path.
/// Reserving amount+max_fee bounds potential debit; actual fee settlement remains
/// in the original business plan and must release any unused reservation.
pub fn check_supplied_spend_state_v1(
    intent: &UcaSpendIntentV1,
    state: Option<&SuppliedSpendStateV1<'_>>,
    expected_parent: &[u8; 32],
    execution_height: u64,
) -> Result<(), SpendPolicyError> {
    use SpendPolicyError::*;
    check_supplied_spend_authority_v1(intent, state, expected_parent, execution_height)?;
    let s = state.ok_or(StateUnavailable)?;
    // Keep the original precedence: a missing/wrong nonce is reported before
    // either debit limit, even when both conditions would reject this intent.
    if s.next_spend_nonce.ok_or(StateUnavailable)? != intent.nonce {
        return Err(NonceMismatch);
    }
    let debit = intent
        .amount
        .checked_add(intent.max_fee)
        .ok_or(LimitExceeded)?;
    if debit > s.grant.max_debit_per_intent || debit > s.grant.remaining_debit_budget {
        return Err(LimitExceeded);
    }
    Ok(())
}

#[cfg(test)]
#[path = "uca_spend_policy_tests.rs"]
mod tests;
