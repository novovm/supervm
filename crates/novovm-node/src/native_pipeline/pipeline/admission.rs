//! Bounded RPC/gossip signature admission on the existing resident compute
//! owner. Results grant no nonce lease, mempool ACK, state or signing authority.

use super::*;
use crate::native_pipeline::ingress::batch::{
    admission_retained_bytes, admission_sizes, AdmissionRows,
};

pub struct SignatureAdmissionRequest {
    pub(super) inputs: Vec<AdmissionInput>,
    body_bytes: usize,
    retained_bytes: usize,
    max_transaction_bytes: usize,
}

impl SignatureAdmissionRequest {
    pub fn new(inputs: Vec<AdmissionInput>) -> Result<Self> {
        ensure!(!inputs.is_empty(), "empty signature admission request");
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
}

impl SignatureAdmissionOutput {
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
        let command = AdmissionCommand {
            request,
            reply,
            permit: permit.clone(),
        };
        match sender.try_send(DriverMessage::Admission(command)) {
            Ok(()) => {
                worker.thread().unpark();
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
        })))
    }
}

#[cfg(test)]
mod tests;
