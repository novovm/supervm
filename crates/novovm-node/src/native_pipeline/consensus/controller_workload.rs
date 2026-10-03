//! Test-only signed load and a pure, serial result oracle for controller tests.
//!
//! These deterministic keys are PUBLIC FIXTURES, never production identities.
//! This module creates no AOEM session, database, candidate, vote or certificate.
//! Generate/sign and evaluate the oracle outside the measured controller path;
//! expected values are assertions, not inputs authorizing business execution.

#![cfg(test)]

#[path = "controller_workload_tree_tests.rs"]
mod tree_batch_tests;

use crate::native_pipeline::business::direct_nov_fee::{
    quote_and_settle, quote_transfer, DirectNovFeePolicy, FeeState, TransferFeeRequest,
};
use crate::native_pipeline::business::nov_transfer_batch::{
    balance_key, fee_record_changes, nonce_key,
};
use crate::native_pipeline::business::quoted_transfer::{
    compute_outcome, Account, TransferIntent, TransferSnapshot,
};
use crate::native_pipeline::ingress::authentication::{
    authenticate_transfer_v3, check_nonce_sequence,
};
use crate::native_pipeline::ingress::wire::{
    encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use crate::native_pipeline::state::tree::StateChange;
use anyhow::{ensure, Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub const CHAIN_ID: u64 = 292;
pub const AMOUNT: u128 = 1;
pub const BATCH_SIZES: [usize; 3] = [32, 256, 1024];
const TRANSACTION_BYTES: usize = 1024;
const FUNDING_MARGIN: u128 = 1_000_000;

fn key_from_index(domain: &[u8], index: u64) -> SigningKey {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(index.to_be_bytes());
    SigningKey::from_bytes(&hash.finalize().into())
}

/// Full-width, domain-separated deterministic test key; index 256 is NOT 0.
pub fn signing_key(index: u64) -> SigningKey {
    key_from_index(b"novovm/controller-workload/sender/v1\0", index)
}

fn account_for_key(key: &SigningKey) -> Account {
    let digest = Sha256::digest(key.verifying_key().to_bytes());
    let address: [u8; 20] = digest[12..].try_into().expect("SHA-256 suffix width");
    Account::from(address)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sender {
    pub index: u64,
    pub account: Account,
    pub public_key: [u8; 32],
}

pub struct Workload {
    senders: Vec<Sender>,
    recipient: Account,
    heights: u64,
    policy: DirectNovFeePolicy,
    initial_sender_balance: u128,
}

/// Complete expected logical state after a prefix. Nonce absence at height zero
/// is retained (not encoded as a present zero); the recipient is initially absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedState {
    pub height: u64,
    pub transactions: u64,
    pub balances: BTreeMap<Account, u128>,
    pub nonces: BTreeMap<[u8; 32], u64>,
    pub fees: FeeState,
}

impl ExpectedState {
    /// Readback expectations using the SAME balance/nonce/paged-fee primitives
    /// as the installed business profile. Use the workload's original policy.
    /// This is a full logical snapshot, not a candidate patch or publish token.
    pub fn record_changes(&self, policy: &DirectNovFeePolicy) -> Result<Vec<StateChange>> {
        let mut changes = fee_record_changes(policy, &self.fees)?;
        changes.extend(
            self.balances
                .iter()
                .map(|(account, value)| StateChange::Put {
                    key: balance_key(account),
                    value: value.to_le_bytes().to_vec(),
                }),
        );
        changes.extend(
            self.nonces
                .iter()
                .map(|(identity, value)| StateChange::Put {
                    key: nonce_key(identity),
                    value: value.to_le_bytes().to_vec(),
                }),
        );
        Ok(changes)
    }
}

impl Workload {
    pub fn new(batch_size: usize, heights: u64, policy: DirectNovFeePolicy) -> Result<Self> {
        ensure!(
            BATCH_SIZES.contains(&batch_size),
            "unsupported workload batch size"
        );
        ensure!(heights > 0, "workload must contain a height");
        heights
            .checked_mul(u64::try_from(batch_size)?)
            .context("workload transaction count overflow")?;
        policy.validate()?;
        ensure!(
            !policy.settlement_paused,
            "successful workload requires enabled settlement"
        );
        let recipient = account_for_key(&key_from_index(
            b"novovm/controller-workload/recipient/v1\0",
            0,
        ));
        let senders: Vec<_> = (0..batch_size)
            .map(|index| {
                let index = index as u64;
                let key = signing_key(index);
                Sender {
                    index,
                    account: account_for_key(&key),
                    public_key: key.verifying_key().to_bytes(),
                }
            })
            .collect();
        let accounts: BTreeSet<_> = senders.iter().map(|sender| &sender.account).collect();
        let keys: BTreeSet<_> = senders.iter().map(|sender| sender.public_key).collect();
        ensure!(
            accounts.len() == batch_size
                && keys.len() == batch_size
                && !accounts.contains(&recipient),
            "workload identities are not independent"
        );
        let request = fee_request([0; 32], senders[0].account.clone(), recipient.clone());
        let quote = quote_transfer(&request, &policy, 0)??;
        let initial_sender_balance = AMOUNT
            .checked_add(quote.nov_amount)
            .and_then(|debit| debit.checked_mul(u128::from(heights)))
            .and_then(|debit| debit.checked_add(FUNDING_MARGIN))
            .context("workload funding overflow")?;
        Ok(Self {
            senders,
            recipient,
            heights,
            policy,
            initial_sender_balance,
        })
    }

    pub fn batch_size(&self) -> usize {
        self.senders.len()
    }

    pub fn heights(&self) -> u64 {
        self.heights
    }

    pub fn senders(&self) -> &[Sender] {
        &self.senders
    }

    pub fn recipient(&self) -> &Account {
        &self.recipient
    }

    pub fn initial_sender_balance(&self) -> u128 {
        self.initial_sender_balance
    }

    /// Only explicit test genesis records; no executed transaction or nonce is
    /// fabricated. Caller owns staging/installing these on the real test store.
    pub fn initial_changes(&self) -> Result<Vec<StateChange>> {
        self.initial_expected().record_changes(&self.policy)
    }

    /// One independently signed payment per sender, stable index order. Each
    /// later height spends the previous height's nonce, not a disjoint workload.
    pub fn raw_height(&self, height: u64) -> Result<Vec<Vec<u8>>> {
        ensure!(
            (1..=self.heights).contains(&height),
            "workload height out of range"
        );
        self.senders
            .iter()
            .map(|sender| {
                let key = signing_key(sender.index);
                let mut tx = TransferV3 {
                    chain_id: CHAIN_ID,
                    from: sender.account.as_bytes().to_vec(),
                    to: self.recipient.as_bytes().to_vec(),
                    asset: "NOV".into(),
                    amount: AMOUNT,
                    nonce: height - 1,
                    fee_policy: FeePolicy {
                        pay_asset: "NOV".into(),
                        max_pay_amount: 0,
                        slippage_bps: 0,
                    },
                    signature: Vec::new(),
                };
                let signature = key.sign(&signing_message(&tx)?);
                tx.signature = key.verifying_key().to_bytes().to_vec();
                tx.signature.extend_from_slice(&signature.to_bytes());
                encode_transfer_v3(&tx)
            })
            .collect()
    }

    fn initial_expected(&self) -> ExpectedState {
        ExpectedState {
            height: 0,
            transactions: 0,
            balances: self
                .senders
                .iter()
                .map(|sender| (sender.account.clone(), self.initial_sender_balance))
                .collect(),
            nonces: BTreeMap::new(),
            fees: FeeState::default(),
        }
    }

    /// Pure serial oracle, NEVER the production execution or measured workload.
    /// Timestamp must match the actual decided context at each height, including
    /// its effect on quote diagnostics/day windows; no clock/default is inferred.
    pub fn expected_through(
        &self,
        height: u64,
        timestamp_for_height: impl Fn(u64) -> u128,
    ) -> Result<ExpectedState> {
        ensure!(
            height <= self.heights,
            "expected workload height out of range"
        );
        let mut expected = self.initial_expected();
        for current in 1..=height {
            let now = timestamp_for_height(current);
            let authenticated = self
                .raw_height(current)?
                .iter()
                .map(|raw| authenticate_transfer_v3(raw, CHAIN_ID, TRANSACTION_BYTES))
                .collect::<Result<Vec<_>>>()?;
            let parent_nonces = authenticated
                .iter()
                .map(|tx| {
                    let identity = tx.nonce_identity();
                    (
                        identity,
                        expected.nonces.get(&identity).copied().unwrap_or(0),
                    )
                })
                .collect();
            let transitions = check_nonce_sequence(&authenticated, &parent_nonces)?;
            for ((tx, sender), transition) in
                authenticated.iter().zip(&self.senders).zip(transitions)
            {
                ensure!(
                    tx.public_key() == sender.public_key
                        && tx.transfer().from == sender.account.as_bytes(),
                    "authenticated workload sender changed"
                );
                let request =
                    fee_request(tx.tx_hash(), sender.account.clone(), self.recipient.clone());
                let snapshot = TransferSnapshot {
                    payer_balance: expected.balances[&sender.account],
                    recipient_balance: expected.balances.get(&self.recipient).copied().unwrap_or(0),
                    next_nonce: transition.before,
                };
                let quote = quote_transfer(&request, &self.policy, now)??;
                let mut identity = String::with_capacity(64);
                for byte in tx.nonce_identity() {
                    write!(&mut identity, "{byte:02x}")?;
                }
                let intent = TransferIntent {
                    tx_hash: tx.tx_hash(),
                    from: sender.account.clone(),
                    to: self.recipient.clone(),
                    nonce_identity: identity,
                    nonce: tx.transfer().nonce,
                    amount: AMOUNT,
                    approved_fee: quote.nov_amount,
                    fee_cap: quote.max_pay_amount,
                };
                let outcome = compute_outcome(&intent, &snapshot, None)?;
                let settled = quote_and_settle(
                    &request,
                    &self.policy,
                    &expected.fees,
                    snapshot.payer_balance,
                    now,
                )?;
                ensure!(
                    outcome.is_success() && settled.failure.is_none(),
                    "funded workload unexpectedly failed"
                );
                let delta = outcome.delta();
                ensure!(
                    delta.nonce_after == transition.after
                        && delta.fee_funding_delta == settled.charged_fee()
                        && delta.payer.after.checked_add(AMOUNT) == Some(settled.payer_after),
                    "workload fee/transfer/nonce primitives disagree"
                );
                expected
                    .balances
                    .insert(sender.account.clone(), delta.payer.after);
                expected
                    .balances
                    .insert(self.recipient.clone(), delta.recipient.after);
                expected
                    .nonces
                    .insert(tx.nonce_identity(), transition.after);
                expected.fees = settled.after_fee_state;
                expected.transactions = expected
                    .transactions
                    .checked_add(1)
                    .context("oracle count overflow")?;
            }
            expected.height = current;
        }
        expected.fees.validate()?;
        ensure!(
            expected.transactions == height * self.batch_size() as u64
                && expected.balances.get(&self.recipient).copied().unwrap_or(0)
                    == u128::from(expected.transactions) * AMOUNT,
            "workload oracle lost a successful payment"
        );
        Ok(expected)
    }
}

fn fee_request(tx_hash: [u8; 32], payer: Account, recipient: Account) -> TransferFeeRequest {
    TransferFeeRequest {
        tx_hash,
        payer,
        recipient,
        asset: "NOV".into(),
        amount: AMOUNT,
        pay_asset: "NOV".into(),
        max_pay_amount: 0,
        slippage_bps: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::consensus::tests::policy;
    use crate::native_pipeline::ingress::wire::decode_transfer_v3;

    fn now(height: u64) -> u128 {
        172_800_500 + u128::from(height)
    }

    fn records(changes: Vec<StateChange>) -> BTreeMap<Vec<u8>, Vec<u8>> {
        changes
            .into_iter()
            .filter_map(|change| match change {
                StateChange::Put { key, value } => Some((key, value)),
                StateChange::Delete { .. } => None,
            })
            .collect()
    }

    #[test]
    fn full_width_accounts_are_independent_and_all_supported_batches_authenticate() -> Result<()> {
        assert_ne!(signing_key(0).to_bytes(), signing_key(256).to_bytes());
        assert_ne!(signing_key(1).to_bytes(), signing_key(65_537).to_bytes());
        assert_eq!(
            signing_key(u64::MAX).to_bytes(),
            signing_key(u64::MAX).to_bytes()
        );
        for size in BATCH_SIZES {
            let workload = Workload::new(size, 2, policy())?;
            assert_eq!(workload.batch_size(), size);
            assert_eq!(workload.heights(), 2);
            let raw = workload.raw_height(1)?;
            assert_eq!(raw.len(), size);
            let mut identities = BTreeSet::new();
            for (raw, sender) in raw.iter().zip(workload.senders()) {
                let checked = authenticate_transfer_v3(raw, CHAIN_ID, TRANSACTION_BYTES)?;
                assert!(identities.insert(checked.nonce_identity()));
                assert_eq!(checked.public_key(), sender.public_key);
                assert_eq!(checked.transfer().from, sender.account.as_bytes());
                assert_eq!(checked.transfer().to, workload.recipient().as_bytes());
                assert_eq!(checked.transfer().nonce, 0);
                assert_eq!(checked.transfer().amount, AMOUNT);
                assert_ne!(sender.account, *workload.recipient());
            }
        }
        Ok(())
    }

    #[test]
    fn raw_signatures_are_deterministic_chain_bound_and_nonce_sequences_advance() -> Result<()> {
        let workload = Workload::new(32, 3, policy())?;
        let second = Workload::new(32, 3, policy())?;
        assert_eq!(workload.senders(), second.senders());
        assert_eq!(workload.recipient(), second.recipient());
        assert_eq!(workload.raw_height(1)?, second.raw_height(1)?);
        let mut nonces = BTreeMap::new();
        for height in 1..=3 {
            let raw = workload.raw_height(height)?;
            let checked = raw
                .iter()
                .map(|raw| authenticate_transfer_v3(raw, CHAIN_ID, TRANSACTION_BYTES))
                .collect::<Result<Vec<_>>>()?;
            if height == 1 {
                for tx in &checked {
                    nonces.insert(tx.nonce_identity(), 0);
                }
                assert!(
                    authenticate_transfer_v3(&raw[0], CHAIN_ID + 1, TRANSACTION_BYTES).is_err()
                );
                let mut tampered = decode_transfer_v3(&raw[0], TRANSACTION_BYTES)?;
                tampered.amount += 1;
                assert!(authenticate_transfer_v3(
                    &encode_transfer_v3(&tampered)?,
                    CHAIN_ID,
                    TRANSACTION_BYTES
                )
                .is_err());
            }
            for transition in check_nonce_sequence(&checked, &nonces)? {
                assert_eq!(transition.before, height - 1);
                assert_eq!(transition.after, height);
                nonces.insert(transition.identity, transition.after);
            }
            assert!(
                check_nonce_sequence(&checked, &nonces).is_err(),
                "same signed height replayed"
            );
        }
        assert!(nonces.values().all(|nonce| *nonce == 3));
        assert_ne!(workload.raw_height(1)?, workload.raw_height(2)?);
        Ok(())
    }

    #[test]
    fn funded_prefix_oracle_preserves_balances_nonces_and_full_fee_records() -> Result<()> {
        let workload = Workload::new(32, 3, policy())?;
        let initial = records(workload.initial_changes()?);
        assert!(!initial.contains_key(&balance_key(workload.recipient())));
        for sender in workload.senders() {
            assert_eq!(
                initial[&balance_key(&sender.account)],
                workload.initial_sender_balance().to_le_bytes()
            );
        }
        let zero = workload.expected_through(0, |_| panic!("zero prefix has no block time"))?;
        assert!(zero.nonces.is_empty());
        assert_eq!(records(zero.record_changes(&policy())?), initial);
        let expected = workload.expected_through(3, now)?;
        assert_eq!(expected, workload.expected_through(3, now)?);
        assert_eq!(expected.height, 3);
        assert_eq!(expected.transactions, 96);
        assert_eq!(expected.balances[workload.recipient()], 96);
        assert_eq!(expected.nonces.len(), 32);
        assert!(expected.nonces.values().all(|nonce| *nonce == 3));
        assert!(workload
            .senders()
            .iter()
            .all(|sender| expected.balances[&sender.account] == FUNDING_MARGIN));
        let accounting = &expected.fees.accounting;
        assert_eq!(accounting.settlements, 96);
        assert_eq!(accounting.journal_next_seq, 96);
        assert_eq!(
            accounting.treasury_reserve_nov,
            Some(accounting.settled_nov_total)
        );
        assert_eq!(
            accounting.reserve_bucket_nov + accounting.fee_bucket_nov + accounting.risk_buffer_nov,
            accounting.settled_nov_total
        );
        assert_eq!(
            expected.balances.values().sum::<u128>() + accounting.settled_nov_total,
            32 * workload.initial_sender_balance()
        );
        let final_records = records(expected.record_changes(&policy())?);
        for (identity, nonce) in &expected.nonces {
            assert_eq!(final_records[&nonce_key(identity)], nonce.to_le_bytes());
        }
        for (key, value) in records(fee_record_changes(&policy(), &expected.fees)?) {
            assert_eq!(final_records[&key], value);
        }
        let last_raw = workload.raw_height(3)?;
        let last = authenticate_transfer_v3(last_raw.last().unwrap(), CHAIN_ID, TRANSACTION_BYTES)?;
        let last_request = fee_request(
            last.tx_hash(),
            workload.senders().last().unwrap().account.clone(),
            workload.recipient().clone(),
        );
        assert_eq!(
            expected.fees.diagnostics.last_quote,
            Some(quote_transfer(&last_request, &policy(), now(3))??)
        );
        Ok(())
    }

    #[test]
    fn invalid_dimensions_and_incompatible_fee_policy_are_rejected() -> Result<()> {
        for size in [0, 31, 255, 1025] {
            assert!(Workload::new(size, 3, policy()).is_err());
        }
        assert!(Workload::new(32, 0, policy()).is_err());
        assert!(Workload::new(1024, u64::MAX, policy()).is_err());
        let mut paused = policy();
        paused.settlement_paused = true;
        assert!(Workload::new(32, 1, paused).is_err());
        let workload = Workload::new(32, 3, policy())?;
        assert!(workload.raw_height(0).is_err());
        assert!(workload.raw_height(4).is_err());
        assert!(workload.expected_through(4, now).is_err());
        Ok(())
    }
}
