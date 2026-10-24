//! Candidate-local invariant quotation, shared by AOEM component predictions
//! and ordered settlement. Neither type can be decoded or constructed from a
//! caller-supplied quote. No balance, nonce or treasury approval is cached.
use super::*;
use std::sync::Arc;

pub(in crate::native_pipeline::business) struct FeeQuotationContext {
    policy: DirectNovFeePolicy,
    now: u128,
    source: String,
    contract: String,
}

impl FeeQuotationContext {
    pub fn new(policy: &DirectNovFeePolicy, now: u128) -> Result<Arc<Self>> {
        policy.validate()?;
        Ok(Arc::new(Self {
            policy: policy.clone(),
            now,
            source: policy.normalized_source(),
            contract: policy.contract_id(),
        }))
    }

    pub fn prepare(self: &Arc<Self>, request: &TransferFeeRequest) -> Result<PreparedTransferFee> {
        Ok(PreparedTransferFee {
            quotation: quote_transfer_validated(request, &self.policy, self.now)?,
            request: request.clone(),
            context: Arc::clone(self),
        })
    }

    /// A signed UCA cap is literal, including zero. Retain the legacy sentinel
    /// only for the existing reference transaction contract.
    pub fn prepare_strict_cap(
        self: &Arc<Self>,
        request: &TransferFeeRequest,
    ) -> Result<PreparedTransferFee> {
        Ok(PreparedTransferFee {
            quotation: quote_transfer_with_cap(request, &self.policy, self.now, false)?,
            request: request.clone(),
            context: Arc::clone(self),
        })
    }
}

pub(in crate::native_pipeline::business) struct PreparedTransferFee {
    request: TransferFeeRequest,
    context: Arc<FeeQuotationContext>,
    quotation: std::result::Result<FeeQuote, FeeFailure>,
}

impl PreparedTransferFee {
    /// Immutable candidate-bound quote, not a balance or settlement approval.
    pub fn quotation(&self) -> &std::result::Result<FeeQuote, FeeFailure> {
        &self.quotation
    }
    pub fn intent_terms(&self) -> (u128, u128, Option<String>) {
        match &self.quotation {
            Ok(quote) => (quote.nov_amount, quote.max_pay_amount, None),
            Err(error) => (0, 0, Some(error.to_string())),
        }
    }

    pub fn settle(
        &self,
        request: &TransferFeeRequest,
        policy: &DirectNovFeePolicy,
        before: &FeeState,
        payer_before: u128,
        now: u128,
    ) -> Result<FeeEffects> {
        ensure!(
            request == &self.request && policy == &self.context.policy && now == self.context.now,
            "prepared fee belongs to another request, policy or candidate time"
        );
        before.validate()?;
        match &self.quotation {
            Ok(quote) => settle_verified_quote(
                request,
                quote,
                policy,
                before,
                payer_before,
                now,
                Some((&self.context.source, &self.context.contract)),
            ),
            Err(rejected) => {
                let mut after = before.clone();
                after.diagnostics.quote_max_pay_exceeded =
                    after.diagnostics.quote_max_pay_exceeded.saturating_add(1);
                after.diagnostics.last_quote_failure = Some(rejected.clone());
                Ok(FeeEffects {
                    after_fee_state: after,
                    payer_after: payer_before,
                    quote: None,
                    failure: Some(rejected.clone()),
                    journal: None,
                    clear_clearing_candidates: false,
                })
            }
        }
    }
}
