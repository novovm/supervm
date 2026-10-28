//! NUAS structural bodies share the original bounded channel and candidate
//! executor. This is neither asset admission nor a new publication authority.
use super::*;
use crate::native_pipeline::{
    business::{direct_nov_fee::DirectNovFeePolicy, nov_transfer_batch::*},
    persistence::StoredCandidate,
    pipeline::BatchRequest,
};
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};

const VERSION: u16 = 2;

fn batch_limits(limits: DecodeLimits) -> AssetSpendBatchLimitsV2 {
    AssetSpendBatchLimitsV2 {
        transactions: limits.transactions,
        transaction_bytes: limits.transaction_bytes,
        body_bytes: limits.body_bytes,
    }
}

fn uca_profile(context: &BatchContext) -> Result<()> {
    ensure!(
        context.business_program == uca_program_id()
            && context.receipt_codec == uca_receipt_codec()
            && context.semantic_version == SEMANTIC_VERSION,
        "NUAS body requires exact UCA program/receipt/semantic profile"
    );
    Ok(())
}

fn validate(batch: &AssetSpendBatchV2, limits: DecodeLimits) -> Result<()> {
    limits.validate()?;
    ensure!(
        !batch.is_empty()
            && batch.len() <= limits.transactions
            && batch.max_transaction_bytes() <= limits.transaction_bytes
            && batch.canonical_bytes() <= limits.body_bytes,
        "NUAS body exceeds canonical input budget"
    );
    Ok(())
}

pub(super) fn append_body(
    out: &mut Vec<u8>,
    context: &BatchContext,
    batch: &AssetSpendBatchV2,
    limits: DecodeLimits,
) -> Result<()> {
    uca_profile(context)?;
    validate(batch, limits)?;
    let bytes = batch.encode()?;
    ensure!(
        out.len()
            .checked_add(2 + CONTEXT_BYTES)
            .and_then(|n| n.checked_add(bytes.len()))
            .is_some_and(|n| n <= limits.message_bytes),
        "NUAS envelope exceeds message budget"
    );
    out.extend_from_slice(&VERSION.to_be_bytes());
    append_context(out, context);
    out.extend_from_slice(&bytes);
    Ok(())
}

/// The v2 body commits exact signed asset accounts and canonical input.
pub fn uca_body_id(
    context: &BatchContext,
    batch: &AssetSpendBatchV2,
    limits: DecodeLimits,
) -> Result<Hash> {
    let mut bytes = Vec::new();
    append_body(&mut bytes, context, batch, limits)?;
    ensure!(
        bytes.len().saturating_add(PREFIX_BYTES) <= limits.message_bytes,
        "NUAS envelope budget"
    );
    let mut digest = Sha256::new();
    digest.update(b"novovm-round-bft-transport/v1/nuas-body/2\0");
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
    Ok(digest.finalize().into())
}

pub(super) fn decode(reader: &mut Reader<'_>, limits: DecodeLimits) -> Result<Message> {
    ensure!(
        reader.u16()? == VERSION,
        "NUAS body payload version mismatch"
    );
    let context = read_context(reader)?;
    uca_profile(&context)?;
    let batch = AssetSpendBatchV2::decode(reader.take(reader.remaining())?, batch_limits(limits))?;
    validate(&batch, limits)?;
    Ok(Message::AssetBody {
        context,
        batch: Arc::new(batch),
    })
}

pub(super) fn append_transactions(
    out: &mut Vec<u8>,
    scope: &TransactionsScope,
    batch: &AssetSpendBatchV2,
    limits: DecodeLimits,
) -> Result<()> {
    scope.validate_shape()?;
    validate(batch, limits)?;
    // Reuse the same explicit version and exact session/sequence scope as the
    // existing native input flow. No additional delivery or account identity.
    transactions::append_scope(out, scope);
    let bytes = batch.encode()?;
    ensure!(
        out.len()
            .checked_add(bytes.len())
            .is_some_and(|n| n <= limits.message_bytes),
        "NUAS input envelope exceeds message budget"
    );
    out.extend_from_slice(&bytes);
    Ok(())
}

pub(super) fn decode_transactions(
    reader: &mut Reader<'_>,
    limits: DecodeLimits,
) -> Result<Message> {
    let scope = transactions::read_scope(reader)?;
    let batch = AssetSpendBatchV2::decode(reader.take(reader.remaining())?, batch_limits(limits))?;
    validate(&batch, limits)?;
    Ok(Message::AssetTransactions {
        scope,
        batch: Arc::new(batch),
    })
}

pub(crate) fn candidate_body_id(message: &Message, limits: DecodeLimits) -> Result<Hash> {
    match message {
        Message::Body {
            context,
            raw_transactions,
        } => body_id(context, raw_transactions, limits),
        Message::ApflBody { context, batch } => apfl_body_id(context, batch, limits),
        Message::AssetBody { context, batch } => uca_body_id(context, batch, limits),
        _ => anyhow::bail!("candidate input must be an exact body"),
    }
}

/// Shared by live body admission and cold replay. No signature-checked object
/// can be reconstructed from a peer assertion or a persisted result alone.
pub(crate) fn candidate_request(
    message: &Message,
    policy: &DirectNovFeePolicy,
) -> Result<BatchRequest> {
    match message {
        Message::AssetBody { context, batch } => {
            uca_profile(context)?;
            ensure!(
                context.effect_contract == uca_effect_contract(policy)?,
                "UCA body policy mismatch"
            );
            BatchRequest::from_uca_batch(batch.clone(), *context, policy.clone())
        }
        Message::Body {
            context,
            raw_transactions,
        } => {
            legacy_profile(context, policy)?;
            BatchRequest::new(raw_transactions.clone(), *context, policy.clone())
        }
        Message::ApflBody { context, batch } => {
            legacy_profile(context, policy)?;
            BatchRequest::from_apfl(batch.clone(), *context, policy.clone())
        }
        _ => anyhow::bail!("candidate input must be an exact body"),
    }
}

fn legacy_profile(context: &BatchContext, policy: &DirectNovFeePolicy) -> Result<()> {
    ensure!(
        context.business_program == program_id()
            && context.receipt_codec == receipt_codec()
            && context.semantic_version == SEMANTIC_VERSION
            && context.effect_contract == effect_contract(policy)?,
        "legacy body profile mismatch; NUAS cannot use V3 input"
    );
    Ok(())
}

/// Rebuild only transport input from a content-checked original archive. The
/// receiving pipeline must still verify signatures and read its own parent.
pub(crate) fn stored_body(stored: &StoredCandidate, limits: DecodeLimits) -> Result<Message> {
    let context = *stored.context();
    if context.business_program == uca_program_id() {
        uca_profile(&context)?;
        Ok(Message::AssetBody {
            context,
            batch: Arc::new(AssetSpendBatchV2::from_raw(
                stored.raw_transactions(),
                batch_limits(limits),
            )?),
        })
    } else {
        ensure!(
            context.business_program == program_id()
                && context.receipt_codec == receipt_codec()
                && context.semantic_version == SEMANTIC_VERSION,
            "unknown archived business profile"
        );
        Ok(Message::ApflBody {
            context,
            batch: Arc::new(ApflTransferBatch::from_raw(
                stored.raw_transactions(),
                ApflLimits {
                    transactions: limits.transactions,
                    transaction_bytes: limits.transaction_bytes,
                    body_bytes: limits.body_bytes,
                },
            )?),
        })
    }
}

#[cfg(test)]
#[path = "uca_tests.rs"]
pub(super) mod tests;
