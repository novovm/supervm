//! Explicit asset-account v2 archive extension on the original packet/store.
//! These are bounded content/continuity checks, not cryptographic verification,
//! a trusted-parent proof or a reconstruction of the economic execution plan.
use super::*;
pub(super) use crate::native_pipeline::business::nov_transfer_batch::{
    authorization_code, authorization_error,
};
use crate::native_pipeline::business::nov_transfer_batch::{
    balance_key, nonce_key, uca_program_id, uca_receipt_codec,
};
use crate::native_pipeline::business::quoted_transfer::Account;
use novovm_adapter_api::uca_spend_policy::SpendPolicyError;
use novovm_protocol::uca_spend::SignedAssetSpendV2;

pub(super) fn is_profile(context: &BatchContext) -> bool {
    context.business_program == uca_program_id()
        && context.receipt_codec == uca_receipt_codec()
        && context.semantic_version == 1
}

pub(super) fn receipt_hash_with_spend(
    codec: NodeHash,
    receipts: &[Vec<u8>],
    outputs: Option<&[SpendOutput]>,
) -> NodeHash {
    let Some(outputs) = outputs else {
        return receipt_hash(codec, receipts);
    };
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/nov-receipt-batch/v1\0");
    hash.update(codec);
    hash.update((receipts.len() as u64).to_be_bytes());
    for bytes in receipts {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hash.update(b"asset-authorized-effects/v2\0");
    for output in outputs {
        hash.update(authorization_code(output.failure).to_be_bytes());
        hash.update(output.remaining.to_be_bytes());
    }
    hash.finalize().into()
}

pub(super) fn validate_receipts(
    plan: &BatchPlan,
    receipts: &[Vec<u8>],
    outputs: &[SpendOutput],
    budget: PacketBudget,
) -> Result<()> {
    ensure!(
        is_profile(plan.context()),
        "unsupported UCA candidate profile"
    );
    ensure!(
        receipts.len() == plan.transaction_count() && outputs.len() == receipts.len(),
        "UCA receipt/output/body count mismatch"
    );
    let mut hashes = BTreeSet::new();
    let mut nonces = BTreeMap::new();
    let mut remaining = BTreeMap::<(NodeHash, NodeHash), u128>::new();
    let mut balances = BTreeMap::new();
    let mut journal_seq = None;
    let writable: BTreeSet<&[u8]> = plan
        .declared_access()
        .iter()
        .filter(|d| d.may_put)
        .map(|d| d.key.as_slice())
        .collect();
    for (index, (bytes, output)) in receipts.iter().zip(outputs).enumerate() {
        let raw = plan.canonical_transaction(index)?;
        ensure!(
            raw.len() <= budget.max_transaction_bytes,
            "UCA transaction exceeds budget"
        );
        let signed = SignedAssetSpendV2::decode(raw.as_ref())?;
        ensure!(
            signed.encode()?.as_slice() == raw.as_ref(),
            "noncanonical UCA transaction"
        );
        let intent = &signed.intent;
        ensure!(
            intent.chain_id == plan.context().chain_id
                && intent.genesis == plan.context().genesis_config_commitment,
            "UCA candidate transaction domain mismatch"
        );
        let tx_hash: NodeHash = Sha256::digest(intent.signing_bytes()?).into();
        ensure!(hashes.insert(tx_hash), "candidate duplicate UCA intent");
        let identity = intent.nonce_identity()?;
        let (receipt, tail): (ReceiptRef<'_>, _) = postcard::take_from_bytes(bytes)?;
        ensure!(
            tail.is_empty() && postcard::to_allocvec(&receipt)? == *bytes,
            "noncanonical UCA receipt"
        );
        ensure!(
            receipt.delta.payer.account == intent.payer.as_slice()
                && receipt.delta.recipient.account == intent.recipient.as_slice(),
            "asset receipt accounts differ from signed transaction"
        );
        ensure!(
            receipt.tx_hash == tx_hash
                && receipt.delta.tx_hash == tx_hash
                && receipt.signer_identity == identity
                && receipt.delta.nonce_identity == identity_hex(&identity),
            "UCA receipt transaction/nonce binding mismatch"
        );
        for delta in [&receipt.delta.payer, &receipt.delta.recipient] {
            let account = Account::try_from(delta.account).map_err(anyhow::Error::msg)?;
            ensure!(
                writable.contains(balance_key(&account).as_slice()),
                "UCA receipt balance outside declared state"
            );
            if let Some(before) = balances.get(delta.account) {
                ensure!(
                    *before == delta.before,
                    "UCA receipt balance prefix mismatch"
                );
            }
        }
        ensure!(
            writable.contains(nonce_key(&identity).as_slice()),
            "UCA receipt nonce outside declared state"
        );
        if let Some(before) = nonces.insert(identity, receipt.delta.nonce_after) {
            ensure!(
                before == receipt.delta.nonce_before,
                "UCA nonce prefix mismatch"
            );
        }
        let debit = if let Some(error) = output.failure {
            ensure!(
                matches!(&receipt.failure, Some(FailureRef::Authorization(reason))
                if *reason == format!("{error:?}")),
                "UCA authorization result mismatch"
            );
            ensure!(
                receipt.delta.nonce_before == receipt.delta.nonce_after
                    && receipt.delta.payer.before == receipt.delta.payer.after
                    && receipt.delta.recipient.before == receipt.delta.recipient.after
                    && receipt.delta.fee_funding_delta == 0
                    && receipt.quote.is_none()
                    && receipt.fee_failure.is_none()
                    && receipt.journal.is_none()
                    && !receipt.clear_clearing_candidates,
                "rejected UCA authorization has economic effects"
            );
            if error == SpendPolicyError::NonceMismatch {
                ensure!(
                    intent.nonce != receipt.delta.nonce_before,
                    "UCA nonce rejection has matching nonce"
                );
            }
            0
        } else {
            ensure!(
                !matches!(&receipt.failure, Some(FailureRef::Authorization(_))),
                "authorized UCA output has authorization failure"
            );
            ensure!(
                receipt.delta.nonce_before == intent.nonce
                    && intent.nonce.checked_add(1) == Some(receipt.delta.nonce_after),
                "authorized UCA nonce transition mismatch"
            );
            ensure!(
                plan.context().height >= intent.not_before_height
                    && plan.context().height < intent.expires_at_height,
                "authorized UCA intent outside validity window"
            );
            ensure!(
                receipt.delta.fee_funding_delta <= intent.max_fee,
                "UCA actual fee exceeds signed cap"
            );
            validate_fee_receipt(
                &receipt,
                tx_hash,
                receipt.delta.payer.account,
                plan.context(),
                &mut journal_seq,
            )?;
            let success = receipt.failure.is_none();
            let fee = receipt.delta.fee_funding_delta;
            let same = receipt.delta.payer.account == receipt.delta.recipient.account;
            if same {
                ensure!(
                    receipt.delta.payer.before == receipt.delta.recipient.before
                        && receipt.delta.payer.after == receipt.delta.recipient.after,
                    "UCA self-transfer balance views differ"
                );
            }
            let balance_debit = if success && !same {
                intent
                    .amount
                    .checked_add(fee)
                    .context("UCA actual debit overflow")?
            } else {
                fee
            };
            ensure!(
                receipt.delta.payer.before.checked_sub(balance_debit)
                    == Some(receipt.delta.payer.after),
                "UCA payer debit mismatch"
            );
            if !same {
                let expected = if success {
                    receipt.delta.recipient.before.checked_add(intent.amount)
                } else {
                    Some(receipt.delta.recipient.before)
                };
                ensure!(
                    expected == Some(receipt.delta.recipient.after),
                    "UCA recipient effect mismatch"
                );
            }
            if success {
                intent
                    .amount
                    .checked_add(fee)
                    .context("UCA authorized debit overflow")?
            } else {
                fee
            }
        };
        if let Some(update) = &intent.authority_update {
            ensure!(
                output.remaining == 0,
                "asset management cannot report spend budget"
            );
            if output.failure.is_none()
                && receipt.failure.is_none()
                && receipt.fee_failure.is_none()
            {
                for grant in &update.grants {
                    remaining.insert((identity, grant.key_ref), grant.remaining_debit_budget);
                }
            }
            balances.insert(
                receipt.delta.payer.account.to_vec(),
                receipt.delta.payer.after,
            );
            balances.insert(
                receipt.delta.recipient.account.to_vec(),
                receipt.delta.recipient.after,
            );
            continue;
        }
        // Grant identity intentionally excludes submitted epoch, as the captured
        // grant and its budget survive an obsolete rejected intent in this batch.
        let grant = (identity, intent.spend_key_ref);
        if let Some(previous) = remaining.insert(grant, output.remaining) {
            ensure!(
                previous.checked_sub(debit) == Some(output.remaining),
                "UCA grant budget prefix mismatch"
            );
            if output.failure.is_none() {
                ensure!(
                    intent
                        .amount
                        .checked_add(intent.max_fee)
                        .is_some_and(|reserve| reserve <= previous),
                    "UCA authorization exceeds prefix reservation"
                );
            }
        } else {
            let before = output
                .remaining
                .checked_add(debit)
                .context("UCA grant budget overflow")?;
            if output.failure.is_none() {
                ensure!(
                    intent
                        .amount
                        .checked_add(intent.max_fee)
                        .is_some_and(|reserve| reserve <= before),
                    "UCA authorization exceeds initial reservation"
                );
            }
        }
        balances.insert(
            receipt.delta.payer.account.to_vec(),
            receipt.delta.payer.after,
        );
        balances.insert(
            receipt.delta.recipient.account.to_vec(),
            receipt.delta.recipient.after,
        );
    }
    Ok(())
}
