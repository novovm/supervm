//! Bounded RPC/gossip signature admission on the existing resident compute
//! owner. Results grant no nonce lease, mempool ACK, state or signing authority.

use super::*;
use crate::native_pipeline::ingress::authentication::SignatureCheckedTransfer;
use crate::native_pipeline::ingress::batch::{
    admission_retained_bytes, admission_sizes, AdmissionRows, CheckedAdmission,
    SignatureCheckedBatch,
};
use crate::native_pipeline::profile;

/// Process-local diagnostic joins only. The original source Arc remains the
/// authority/retention boundary; this address is neither a content identity nor
/// stable across processes or after that source is released.
pub(super) fn admission_input_profile<'a>(
    inputs: impl IntoIterator<Item = &'a AdmissionInput>,
    include_indices: bool,
) -> Option<serde_json::Value> {
    let mut rows = 0usize;
    let mut source = None;
    let mut first = None;
    let mut last = None;
    let mut multiple_sources = false;
    let mut indices = include_indices.then(Vec::new);
    for input in inputs {
        rows += 1;
        if let AdmissionInput::Uca { batch, index } = input {
            let current = Arc::as_ptr(batch) as usize;
            if let Some(source) = source {
                multiple_sources |= source != current;
            } else {
                source = Some(current);
                first = Some(*index);
            }
            last = Some(*index);
            if let Some(indices) = &mut indices {
                indices.push(*index);
            }
        }
    }
    source.map(|source| {
        serde_json::json!({
            "source_ref":source, "rows":rows, "first_index":first, "last_index":last,
            "multiple_sources":multiple_sources, "indices":indices
        })
    })
}

fn profile_admission_inputs<'a>(
    event: &'static str,
    inputs: impl IntoIterator<Item = &'a AdmissionInput>,
) {
    if !profile::asset_enabled() {
        return;
    }
    if let Some(detail) = admission_input_profile(inputs, false) {
        profile::asset_event(event, 0, || detail);
    }
}

/// Immutable evidence from THIS pipeline's strict verifier. It grants neither
/// a nonce lease nor spend/publication authority. Its source must stay charged
/// in the existing pending-input owner; no ingress permit is retained here.
pub struct AdmittedTransfer {
    checked: Arc<CheckedAdmission>,
    owner: Arc<()>,
}

impl AdmittedTransfer {
    pub fn checked(&self) -> &SignatureCheckedTransfer {
        self.checked
            .v3()
            .expect("V3 admission accessor requires V3 contract")
    }

    pub fn checked_uca(&self) -> Option<&novovm_exec::uca_spend::AssetSpendSignatureFactV3> {
        self.checked.uca()
    }

    pub fn is_uca(&self) -> bool {
        self.checked_uca().is_some()
    }

    pub fn matches_uca_source(
        &self,
        batch: &Arc<novovm_protocol::uca_spend_batch::AssetSpendBatchV3>,
        index: usize,
    ) -> bool {
        matches!(self.checked.as_ref(), CheckedAdmission::Uca { batch: source, index: source_index, .. }
            if Arc::ptr_eq(source, batch) && *source_index == index)
    }

    /// Pool source accounting must cover the very dictionary this token pins,
    /// not merely a different dictionary containing an equal signed row.
    pub(crate) fn matches_apfl_source(&self, batch: &Arc<ApflTransferBatch>, index: usize) -> bool {
        self.checked
            .v3()
            .is_some_and(|checked| checked.matches_apfl_source(batch, index))
    }

    /// Additional logical metadata, excluding the shared immutable APFL source
    /// payload which the pending pool already charges once per dictionary.
    pub fn retained_metadata_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + std::mem::size_of::<CheckedAdmission>()
            + 2 * std::mem::size_of::<usize>()
            + self
                .checked_uca()
                .map_or(0, |checked| checked.intent().retained_heap_bytes())
    }
}

pub struct AdmittedRow {
    pub input: AdmissionInput,
    pub result: Result<AdmittedTransfer>,
}

/// A checked, regrouped batch branded for the pipeline which admitted every
/// signed row. Only the NEW structured dictionary is retained, not pool tokens.
pub struct AdmittedBatch {
    checked: AdmittedSignatures,
    owner: Arc<()>,
}

/// Signature facts only. Neither variant contains a parent-state permit.
/// The UCA variant retains the newly selected dictionary, not the old pool's
/// source owners. Only this module can create it after full-row rebinding.
pub(super) enum AdmittedSignatures {
    V3(SignatureCheckedBatch),
    Uca {
        source: BatchSource,
        transactions: Vec<novovm_exec::uca_spend::SignatureCheckedAssetSpendV3>,
    },
}

impl AdmittedBatch {
    pub fn retained_metadata_bytes(&self) -> usize {
        let bytes = match &self.checked {
            AdmittedSignatures::V3(checked) => checked.retained_metadata_bytes(),
            AdmittedSignatures::Uca { transactions, .. } => {
                transactions.capacity()
                    * std::mem::size_of::<novovm_exec::uca_spend::SignatureCheckedAssetSpendV3>()
                    + transactions
                        .iter()
                        .map(|checked| {
                            let signed = checked.signed();
                            signed.public_key.capacity()
                                + signed.signature.capacity()
                                + signed.intent.retained_heap_bytes()
                        })
                        .sum::<usize>()
            }
        };
        bytes + std::mem::size_of::<Self>()
    }

    pub(super) fn source(&self) -> &BatchSource {
        match &self.checked {
            AdmittedSignatures::V3(checked) => checked.source(),
            AdmittedSignatures::Uca { source, .. } => source,
        }
    }

    pub(super) fn ensure_owner(&self, owner: &Arc<()>) -> Result<()> {
        ensure!(
            Arc::ptr_eq(&self.owner, owner),
            "admitted batch belongs to a different pipeline"
        );
        Ok(())
    }

    pub(super) fn into_checked(
        self,
        chain: u64,
        genesis: NodeHash,
        budget: AuthenticationBudget,
    ) -> Result<AdmittedSignatures> {
        match &self.checked {
            AdmittedSignatures::V3(checked) => checked.validate_source(chain, budget)?,
            AdmittedSignatures::Uca {
                source,
                transactions,
            } => {
                let (bytes, max) = source.sizes()?;
                ensure!(
                    !source.is_empty()
                        && source.len() == transactions.len()
                        && source.len() <= budget.transactions
                        && max <= budget.transaction_bytes
                        && bytes <= budget.body_bytes
                        && transactions.iter().all(|checked| {
                            checked.intent().chain_id == chain
                                && checked.intent().genesis == genesis
                        }),
                    "admitted UCA domain or input budget mismatch"
                );
            }
        }
        Ok(self.checked)
    }
}

/// Deterministic test fixture still performs the REAL strict verifier. This
/// cannot manufacture accepted proof from fields or a canonical hash.
#[cfg(test)]
pub(crate) fn admitted_apfl_for_test(
    owner: Arc<()>,
    chain_id: u64,
    batch: Arc<ApflTransferBatch>,
    index: usize,
) -> Result<AdmittedTransfer> {
    Ok(AdmittedTransfer {
        checked: Arc::new(CheckedAdmission::V3(
            crate::native_pipeline::ingress::authentication::authenticate_apfl_row(
                batch, index, chain_id,
            )?,
        )),
        owner,
    })
}

#[cfg(test)]
pub(crate) fn admitted_for_test(
    pipeline: &CandidatePipeline,
    batch: Arc<ApflTransferBatch>,
    index: usize,
) -> Result<AdmittedTransfer> {
    admitted_apfl_for_test(
        pipeline.identity.clone(),
        pipeline.config.store.domain.chain_id,
        batch,
        index,
    )
}

/// Test-only bridge still invokes the actual existing AOEM verifier. It cannot
/// construct signature evidence from a supplied digest or unchecked fields.
#[cfg(test)]
pub(crate) fn admitted_uca_for_test(
    owner: Arc<()>,
    facade: &novovm_exec::AoemExecFacade,
    batch: Arc<novovm_protocol::uca_spend_batch::AssetSpendBatchV3>,
    index: usize,
) -> Result<AdmittedTransfer> {
    let checked = facade
        .asset_spend_verifier_v3()?
        .verify(batch.row(index)?)?
        .into_signature_fact()?;
    Ok(AdmittedTransfer {
        checked: Arc::new(CheckedAdmission::Uca {
            checked,
            batch,
            index,
        }),
        owner,
    })
}

pub struct SignatureAdmissionRequest {
    pub(super) inputs: Vec<AdmissionInput>,
    body_bytes: usize,
    retained_bytes: usize,
    max_transaction_bytes: usize,
}

impl SignatureAdmissionRequest {
    pub(crate) fn profile_event(&self, event: &'static str) {
        profile_admission_inputs(event, &self.inputs);
    }

    pub fn new(inputs: Vec<AdmissionInput>) -> Result<Self> {
        ensure!(!inputs.is_empty(), "empty signature admission request");
        let uca = matches!(&inputs[0], AdmissionInput::Uca { .. });
        ensure!(
            inputs
                .iter()
                .all(|input| matches!(input, AdmissionInput::Uca { .. }) == uca),
            "mixed UCA and V3 signature admission is forbidden"
        );
        let (body_bytes, max_transaction_bytes) = admission_sizes(&inputs)?;
        let retained_bytes = admission_retained_bytes(&inputs)?;
        Ok(Self {
            inputs,
            body_bytes,
            retained_bytes,
            max_transaction_bytes,
        })
    }

    pub fn from_raw(raw: Vec<Vec<u8>>) -> Result<Self> {
        Self::new(raw.into_iter().map(AdmissionInput::Raw).collect())
    }

    pub fn from_apfl(batch: Arc<ApflTransferBatch>) -> Result<Self> {
        Self::new(
            (0..batch.len())
                .map(|index| AdmissionInput::Apfl {
                    batch: batch.clone(),
                    index,
                })
                .collect(),
        )
    }

    pub fn from_uca(
        batch: Arc<novovm_protocol::uca_spend_batch::AssetSpendBatchV3>,
    ) -> Result<Self> {
        Self::new(
            (0..batch.len())
                .map(|index| AdmissionInput::Uca {
                    batch: batch.clone(),
                    index,
                })
                .collect(),
        )
    }

    pub fn len(&self) -> usize {
        self.inputs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
    pub fn into_inputs(self) -> Vec<AdmissionInput> {
        self.inputs
    }

    fn reservation(&self, config: &PipelineConfig) -> Result<usize> {
        let budget = config.authentication;
        ensure!(
            self.len() <= budget.transactions
                && self.max_transaction_bytes <= budget.transaction_bytes
                && self.body_bytes <= budget.body_bytes
                && self.retained_bytes <= budget.body_bytes,
            "signature admission exceeds input budget"
        );
        // Source owners, successful decoded metadata / callbacks and the reply
        // remain charged until consumed or dropped. This is logical retained
        // content, not an allocator or AOEM native-memory cap.
        self.body_bytes
            .max(self.retained_bytes)
            .checked_mul(3)
            .and_then(|bytes| {
                self.len()
                    .checked_mul(512)
                    .and_then(|rows| bytes.checked_add(rows))
            })
            .and_then(|bytes| bytes.checked_add(4096))
            .context("signature admission retained-content overflow")
    }
}

pub enum SignatureAdmissionSubmission {
    Accepted(SignatureAdmissionTicket),
    Backpressured(SignatureAdmissionRequest),
}

pub struct RejectedSignatureAdmissionSubmission {
    pub request: SignatureAdmissionRequest,
    pub error: anyhow::Error,
}

impl std::fmt::Debug for RejectedSignatureAdmissionSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedSignatureAdmissionSubmission")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

pub struct SignatureAdmissionTicket {
    receiver: mpsc::Receiver<Result<SignatureAdmissionOutput>>,
    permit: Option<Arc<Permit>>,
}

impl SignatureAdmissionTicket {
    /// Nonblocking. Dropping this ticket does not cancel accepted AOEM work.
    pub fn try_take(&mut self) -> Result<Option<SignatureAdmissionOutput>> {
        ensure!(
            self.permit.is_some(),
            "signature admission ticket already consumed"
        );
        match self.receiver.try_recv() {
            Ok(result) => {
                self.permit.take();
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.permit.take();
                anyhow::bail!("signature admission owner disconnected; no received ACK allowed")
            }
        }
    }
}

pub struct SignatureAdmissionOutput {
    pub(super) rows: AdmissionRows,
    pub(super) _permit: Arc<Permit>,
    pub(super) owner: Arc<()>,
}

impl SignatureAdmissionOutput {
    pub(crate) fn profile_event(&self, event: &'static str) {
        profile_admission_inputs(event, self.rows.rows.iter().map(|row| &row.input));
    }

    /// Hand this result to the single bounded authority-capture stage, making
    /// the existing signature slot available without releasing any source byte
    /// charge. This is scheduling credit only, never asset authorization. The
    /// private permit always updates its original pipeline's accounting owner.
    pub(crate) fn park_for_authority(&mut self) -> Result<()> {
        self._permit.park_for_authority()
    }

    /// The original bounded source queue takes ownership of this completed
    /// output and any popped rows. Retain every byte, release only its active
    /// scheduling slot; rotating an already parked source is idempotent.
    pub(crate) fn park_for_source(&mut self) -> Result<()> {
        self._permit.park_for_source()
    }

    /// A checked source can resume only the original single authority slot.
    /// Busy leaves both its role and original retained-byte charge unchanged.
    pub(crate) fn resume_authority(&mut self) -> Result<bool> {
        self._permit.resume_authority()
    }

    pub fn len(&self) -> usize {
        self.rows.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.rows.is_empty()
    }
    /// Same input order, including individual signature errors. Pool admission
    /// must recheck the live nonce/projection/reservations before replying.
    /// Removing a row is consumption: the caller must immediately apply/drop
    /// it or account it in its own bounded queue. The ingress permit protects
    /// the remaining output, not rows retained after this output is dropped.
    pub fn pop_front(&mut self) -> Option<AdmissionRow> {
        self.rows.rows.pop_front()
    }

    /// Consume one actual verification result without keeping the single
    /// ingress permit alive in the pool. The caller must charge retained
    /// metadata and source in that pool before keeping a successful token.
    pub fn pop_admitted(&mut self) -> Option<AdmittedRow> {
        let row = self.rows.rows.pop_front()?;
        Some(AdmittedRow {
            input: row.input,
            result: row.result.map(|checked| AdmittedTransfer {
                checked: Arc::new(checked),
                owner: self.owner.clone(),
            }),
        })
    }
    pub fn peak_callbacks(&self) -> usize {
        self.rows.peak_callbacks
    }
}

pub(super) struct AdmissionCommand {
    pub request: SignatureAdmissionRequest,
    pub reply: mpsc::Sender<Result<SignatureAdmissionOutput>>,
    pub permit: Arc<Permit>,
}

impl CandidatePipeline {
    pub fn owns_admitted(&self, row: &AdmittedTransfer) -> bool {
        Arc::ptr_eq(&row.owner, &self.identity)
    }

    /// Rare pool-pressure bridge: retain one exact original signed row instead
    /// of pinning its complete source dictionary. The caller still accounts
    /// this new source/fact in the existing pool before retaining it.
    pub(crate) fn project_admitted_uca_row(
        &self,
        original: &AdmittedTransfer,
    ) -> Result<(AdmissionInput, AdmittedTransfer)> {
        ensure!(self.owns_admitted(original), "foreign UCA verifier owner");
        let CheckedAdmission::Uca {
            checked,
            batch: source,
            index,
        } = original.checked.as_ref()
        else {
            anyhow::bail!("only original signed asset input can be projected");
        };
        ensure!(
            checked.intent().chain_id == self.config.store.domain.chain_id
                && checked.intent().genesis == self.config.store.domain.genesis_config_commitment,
            "projected UCA signature domain mismatch"
        );
        let view = source.view(*index)?;
        let budget = self.config.authentication;
        let batch = Arc::new(
            novovm_protocol::uca_spend_batch::AssetSpendBatchV3::from_views(
                &[view],
                novovm_protocol::uca_spend_batch::AssetSpendBatchLimitsV3 {
                    transactions: budget.transactions,
                    transaction_bytes: budget.transaction_bytes,
                    body_bytes: budget.body_bytes,
                },
            )?,
        );
        ensure!(batch.view(0)? == view, "projected UCA signed row changed");
        let admitted = AdmittedTransfer {
            checked: Arc::new(CheckedAdmission::Uca {
                checked: checked.clone(),
                batch: batch.clone(),
                index: 0,
            }),
            owner: self.identity.clone(),
        };
        Ok((AdmissionInput::Uca { batch, index: 0 }, admitted))
    }

    /// Structural rebinding only: this never authorizes a parent or executes
    /// business. Every new row must match its same-owner evidence in full,
    /// including signatures; canonical transaction hashes are insufficient.
    pub fn rebind_admitted_apfl(
        &self,
        batch: Arc<ApflTransferBatch>,
        admitted: &[&AdmittedTransfer],
    ) -> Result<AdmittedBatch> {
        ensure!(batch.len() == admitted.len(), "admitted row count mismatch");
        for row in admitted {
            ensure!(
                Arc::ptr_eq(&row.owner, &self.identity),
                "admitted transfer belongs to a different pipeline"
            );
            ensure!(
                row.checked
                    .v3()
                    .is_some_and(SignatureCheckedTransfer::is_apfl_view),
                "only retained structured admission can be reused"
            );
        }
        let checked = admitted.iter().map(|row| row.checked()).collect::<Vec<_>>();
        Ok(AdmittedBatch {
            checked: AdmittedSignatures::V3(SignatureCheckedBatch::rebind_apfl(
                self.config.store.domain.chain_id,
                batch,
                &checked,
                self.config.authentication,
            )?),
            owner: self.identity.clone(),
        })
    }

    /// Regroup immutable NUAS signature facts on the original pipeline owner.
    /// The complete signed object (including public key, signature and domain)
    /// and locator pair must match in the selected order. This caches no UCA
    /// authority: the candidate still captures and checks its actual parent.
    pub fn rebind_admitted_uca(
        &self,
        batch: Arc<novovm_protocol::uca_spend_batch::AssetSpendBatchV3>,
        admitted: &[&AdmittedTransfer],
    ) -> Result<AdmittedBatch> {
        let budget = self.config.authentication;
        ensure!(
            !batch.is_empty()
                && batch.len() == admitted.len()
                && batch.len() <= budget.transactions
                && batch.max_transaction_bytes() <= budget.transaction_bytes
                && batch.canonical_bytes() <= budget.body_bytes,
            "admitted UCA row count or input budget mismatch"
        );
        let mut transactions = Vec::with_capacity(batch.len());
        let mut seen = std::collections::BTreeSet::new();
        for (index, row) in admitted.iter().enumerate() {
            ensure!(self.owns_admitted(row), "foreign UCA verifier owner");
            let CheckedAdmission::Uca {
                checked,
                batch: original,
                index: original_index,
            } = row.checked.as_ref()
            else {
                anyhow::bail!("V3 admission cannot authorize a UCA body");
            };
            let view = batch.view(index)?;
            ensure!(
                checked.intent().chain_id == self.config.store.domain.chain_id
                    && checked.intent().genesis
                        == self.config.store.domain.genesis_config_commitment
                    && original.view(*original_index)? == view
                    && seen.insert(checked.intent().signing_bytes()?),
                "admitted UCA signed row, order, locator or domain mismatch"
            );
            transactions.push(checked.reattach(batch.row(index)?)?);
        }
        Ok(AdmittedBatch {
            checked: AdmittedSignatures::Uca {
                source: BatchSource::Uca(batch),
                transactions,
            },
            owner: self.identity.clone(),
        })
    }

    pub fn try_admit_signatures_owned(
        &self,
        request: SignatureAdmissionRequest,
    ) -> std::result::Result<SignatureAdmissionSubmission, RejectedSignatureAdmissionSubmission>
    {
        let prerequisites = (|| {
            Ok::<_, anyhow::Error>((
                request.reservation(&self.config)?,
                self.sender.as_ref().context("pipeline closed")?,
                self.worker
                    .as_ref()
                    .context("pipeline worker unavailable")?,
            ))
        })();
        let (bytes, sender, worker) = match prerequisites {
            Ok(values) => values,
            Err(error) => return Err(RejectedSignatureAdmissionSubmission { request, error }),
        };
        let permit = match self.reserve_admission(bytes) {
            Ok(Some(permit)) => permit,
            Ok(None) => return Ok(SignatureAdmissionSubmission::Backpressured(request)),
            Err(error) => return Err(RejectedSignatureAdmissionSubmission { request, error }),
        };
        let (reply, receiver) = mpsc::channel();
        let admission_profile = profile::asset_enabled()
            .then(|| admission_input_profile(&request.inputs, true))
            .flatten();
        let command = AdmissionCommand {
            request,
            reply,
            permit: permit.clone(),
        };
        match sender.try_send(DriverMessage::Admission(command)) {
            Ok(()) => {
                worker.thread().unpark();
                if let Some(detail) = admission_profile {
                    profile::asset_event("signature_submit_accepted", 0, || detail);
                }
                Ok(SignatureAdmissionSubmission::Accepted(
                    SignatureAdmissionTicket {
                        receiver,
                        permit: Some(permit),
                    },
                ))
            }
            Err(mpsc::TrySendError::Full(DriverMessage::Admission(command))) => {
                Ok(SignatureAdmissionSubmission::Backpressured(command.request))
            }
            Err(mpsc::TrySendError::Disconnected(DriverMessage::Admission(command))) => {
                Err(RejectedSignatureAdmissionSubmission {
                    request: command.request,
                    error: anyhow::anyhow!("pipeline unavailable; signature request not accepted"),
                })
            }
            Err(_) => unreachable!("typed signature command changed while sending"),
        }
    }

    fn reserve_admission(&self, bytes: usize) -> Result<Option<Arc<Permit>>> {
        ensure!(
            bytes <= self.config.max_retained_bytes,
            "signature request exceeds retained budget"
        );
        let current = BatchRequest::retained_reservation(
            self.config
                .authentication
                .body_bytes
                .min(self.config.plan.body_bytes),
            &self.config,
        )?;
        let mut used = match self.usage.try_lock() {
            Ok(used) => used,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => {
                anyhow::bail!("pipeline admission accounting poisoned")
            }
        };
        if used.ingress != 0
            || bytes.checked_add(current).is_none_or(|required| {
                required > self.config.max_retained_bytes.saturating_sub(used.bytes)
            })
        {
            return Ok(None);
        }
        used.ingress += 1;
        used.bytes += bytes;
        drop(used);
        Ok(Some(Arc::new(Permit {
            usage: self.usage.clone(),
            bytes,
            background: false,
            ingress: true,
            admission_role: AtomicU8::new(ADMISSION_INGRESS),
        })))
    }
}

#[cfg(test)]
mod tests;
