//! Optional, explicitly requested proof of an ACTUAL published RPC candidate.
//! ArchiveRead supplies the existing verified chain/QC/content boundary. One
//! separate owner builds the existing full relation, calls AOEM, verifies with
//! operator pins, then persists via the SAME I/O owner. Never publishes/signs.
//! Jobs are non-durable; completed receipts are durable. After restart a query
//! rechecks both archive and cryptography. Native proving is not cancellable.

use super::{ExecutionJournalV1, ExecutionProofPins, JOURNAL_BYTES};
use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use crate::native_pipeline::business::nov_transfer_batch::NovTransferPlan;
use crate::native_pipeline::consensus::{ArchiveBlock, ArchiveRead};
use crate::native_pipeline::ingress::batch::authenticate_batch_for_proof;
use crate::native_pipeline::persistence::io::{IoProofClient, IoTicket};
use crate::native_pipeline::service::ResidentNode;
use crate::native_pipeline::state::frontier::CaptureStep;
use anyhow::{ensure, Context, Result};
use novovm_exec::resident::{ReceiptLimits, ReceiptSession};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAGIC: &[u8; 8] = b"NVPROOF1";
const HEADER: usize = 8 + 32 + 32 + 32 + JOURNAL_BYTES;
const MAX_RECEIPT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofConfig {
    pub library: PathBuf,
    /// Reviewed RISC0 2.3.2 program binary, not an arbitrary uploaded program.
    pub guest: PathBuf,
    /// Eight native u32 words from the independently trusted guest build.
    pub image_id: [u32; 8],
}

impl ProofConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        ExecutionProofPins::new(self.image_id)?;
        ensure!(
            self.library.is_absolute() && self.library.is_file(),
            "explicit proof library missing/unresolved"
        );
        ensure!(
            self.guest.is_absolute() && self.guest.is_file(),
            "explicit proof guest missing/unresolved"
        );
        ensure!(
            (1..=64 * 1024 * 1024).contains(&std::fs::metadata(&self.guest)?.len()),
            "proof guest exceeds ABI limit"
        );
        Ok(())
    }
}

struct Job {
    block: ArchiveBlock,
    generate: bool,
}

enum Stage {
    Idle,
    Archive(Box<ArchiveRead>, bool),
    Working,
}

pub(crate) struct ProofService {
    sender: Option<mpsc::SyncSender<Job>>,
    replies: mpsc::Receiver<Result<Value>>,
    worker: Option<JoinHandle<()>>,
    stage: Stage,
    height: Option<u64>,
    latest: Value,
}

impl ProofService {
    /// Startup only. A single proof job (including its retained response) is
    /// permitted. No unchecked input or unbounded proof backlog is admitted.
    pub(crate) fn start(
        config: ProofConfig,
        node: &ResidentNode,
        policy: DirectNovFeePolicy,
    ) -> Result<Self> {
        config.validate()?;
        let io = node.pipeline.take_proof_io()?;
        let (sender, jobs) = mpsc::sync_channel::<Job>(1);
        let (reply, replies) = mpsc::sync_channel(1);
        let (ready, initialized) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("novovm-proof-owner".into())
            .spawn(move || {
                let startup = (|| -> Result<_> {
                    let mut guest = Vec::new();
                    std::fs::File::open(&config.guest)?
                        .take(64 * 1024 * 1024 + 1)
                        .read_to_end(&mut guest)?;
                    ensure!(
                        !guest.is_empty() && guest.len() <= 64 * 1024 * 1024,
                        "proof guest changed/exceeds limit"
                    );
                    let session = ReceiptSession::open(&config.library, ReceiptLimits::default())?;
                    let pins = ExecutionProofPins::new(config.image_id)?;
                    Ok((guest, session, pins))
                })();
                match startup {
                    Ok((guest, mut session, pins)) => {
                        if ready.send(Ok(())).is_err() {
                            return;
                        }
                        for job in jobs {
                            let result = process(
                                &io,
                                &mut session,
                                &pins,
                                &config.image_id,
                                &guest,
                                &policy,
                                job,
                            );
                            if reply.send(result).is_err() {
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            })?;
        if let Err(error) = initialized
            .recv()
            .context("proof owner failed during initialization")?
        {
            let _ = worker.join();
            return Err(error);
        }
        Ok(Self {
            sender: Some(sender),
            replies,
            worker: Some(worker),
            stage: Stage::Idle,
            height: None,
            latest: json!({"state":"idle","verified":false,"persisted":false}),
        })
    }

    pub(crate) fn request(
        &mut self,
        node: &ResidentNode,
        height: u64,
        generate: bool,
    ) -> Result<Value> {
        ensure!(
            !node.controller.is_recovering(),
            "proof source chain is recovering"
        );
        if self.height == Some(height)
            && (!matches!(self.stage, Stage::Idle) || !generate || self.latest["verified"] == true)
        {
            return Ok(self.latest.clone());
        }
        ensure!(
            matches!(self.stage, Stage::Idle),
            "proof owner busy; one job only, request not accepted"
        );
        let head = node
            .controller
            .head()
            .context("no locally published block")?;
        let archive = ArchiveRead::new(
            height,
            head,
            node.controller.context(),
            node.validators.clone(),
        )?;
        self.stage = Stage::Archive(Box::new(archive), generate);
        self.height = Some(height);
        self.latest = json!({"height":height,"state":"reading_verified_archive","verified":false,
            "persisted":false,"job_durable":false,"changes_finality":false});
        Ok(self.latest.clone())
    }

    pub(crate) fn poll(&mut self, node: &ResidentNode) {
        if let Err(error) = self.poll_inner(node) {
            self.stage = Stage::Idle;
            // An accepted atomic write can complete before its response is
            // lost. Never report "not persisted" or blindly retry that write.
            self.latest = json!({"height":self.height,"state":"failed","verified":false,
                "persisted":Value::Null,"outcome":"unknown_or_unverified",
                "error":format!("{error:#}"),"changes_finality":false});
        }
    }

    fn poll_inner(&mut self, node: &ResidentNode) -> Result<()> {
        match &mut self.stage {
            Stage::Idle => (),
            Stage::Archive(archive, generate) => {
                if let Some(block) = archive.poll(&node.pipeline)? {
                    let block = block.context("published proof source missing")?;
                    // Only one job can be in flight. Full is an invariant error,
                    // never a reason to drop an accepted job or start a new one.
                    self.sender
                        .as_ref()
                        .context("proof owner closed")?
                        .try_send(Job {
                            block,
                            generate: *generate,
                        })
                        .map_err(|_| anyhow::anyhow!("proof owner disconnected/full"))?;
                    self.stage = Stage::Working;
                    self.latest["state"] = json!("proof_owner_pending");
                }
            }
            Stage::Working => match self.replies.try_recv() {
                Ok(result) => {
                    self.latest = result?;
                    self.stage = Stage::Idle;
                }
                Err(mpsc::TryRecvError::Empty) => (),
                Err(mpsc::TryRecvError::Disconnected) => {
                    anyhow::bail!("proof owner disconnected; outcome unknown")
                }
            },
        }
        Ok(())
    }

    pub(crate) fn status(&self) -> Value {
        json!({"configured":true,"mode":"on_demand_async_optional",
            "max_jobs":1,"native_prove_cancellable":false,"job_durable":false,
            "receipt_store":"same_aoem_candidate_database","latest":self.latest})
    }

    /// Administrative drain before pipeline.shutdown. Does not cancel native
    /// proving. Runtime/working memory inside the backend are not bounded by
    /// our input/receipt/one-job admission caps.
    pub(crate) fn shutdown(mut self) -> Result<()> {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("proof owner panicked; recover stored result"))?;
        }
        Ok(())
    }
}

fn admitted<T>(mut submit: impl FnMut() -> Result<Option<IoTicket<T>>>) -> Result<T> {
    // This wait belongs ONLY to the proof owner, never the node poll. Accepted
    // I/O is not cancelled on timeout; unknown write outcomes are not retried.
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(ticket) = submit()? {
            return ticket.wait();
        }
        ensure!(
            Instant::now() < until,
            "proof I/O admission remained backpressured (not accepted)"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn process(
    io: &IoProofClient,
    session: &mut ReceiptSession,
    pins: &ExecutionProofPins,
    image: &[u32; 8],
    guest: &[u8],
    policy: &DirectNovFeePolicy,
    job: Job,
) -> Result<Value> {
    let started = Instant::now();
    let block = job.block;
    let stored = block.stored();
    let expected = ExecutionJournalV1::from_stored(stored)?;
    let candidate = stored.candidate_id();
    let previous = admitted(|| io.try_read_proof(candidate, *image))?;
    let (verified, recovered, prove_ms, verify_ms) = if let Some(blob) = previous {
        let receipt = decode_record(
            &blob,
            candidate,
            stored.document_digest(),
            image,
            &expected.encode(),
        )?;
        let verify = Instant::now();
        let verified = pins.verify(session, receipt, &expected)?;
        (verified, true, 0, verify.elapsed().as_millis())
    } else if job.generate {
        let input = capture_input(io, stored, policy.clone())?;
        let prove = Instant::now();
        // This entry already verifies the backend result independently against
        // the exact archive outputs. Report combined cost, not kernel timing.
        let verified = pins.prove(session, guest, &input, &expected)?;
        (verified, false, prove.elapsed().as_millis(), 0)
    } else {
        return Ok(
            json!({"height":block.point().height,"candidate_id":candidate,
            "state":"not_stored","verified":false,"persisted":false,"changes_finality":false}),
        );
    };
    if !recovered {
        let blob = Arc::new(encode_record(
            candidate,
            stored.document_digest(),
            image,
            verified.journal(),
            verified.receipt(),
        )?);
        admitted(|| io.try_write_proof(candidate, *image, blob.clone()))?;
    }
    Ok(
        json!({"height":block.point().height,"block_hash":block.point().block_hash,
        "candidate_id":candidate,"document_digest":stored.document_digest(),"image_id":image,
        "state":"verified_durable","verified":true,"persisted":true,"recovered":recovered,
        "transactions":stored.raw_transactions().len(),"receipt_bytes":verified.receipt().len(),
        "receipt_sha256":format!("{:x}",Sha256::digest(verified.receipt())),
        "journal_sha256":format!("{:x}",Sha256::digest(verified.journal())),
        "prove_and_verify_ms":prove_ms,"recovery_verify_ms":verify_ms,"total_ms":started.elapsed().as_millis(),
        "changes_finality":false,"business_gpu_certified":false,
        "relation":"NVEXIN01/NVEXEC01 complete direct NOV; not privacy or PQ"}),
    )
}

fn capture_input(
    io: &IoProofClient,
    stored: &crate::native_pipeline::persistence::StoredCandidate,
    policy: DirectNovFeePolicy,
) -> Result<Vec<u8>> {
    // Canonical bytes are a deliberate proof boundary, not APFL wire inflation
    // in the hot pipeline. Guest independently verifies again; no output writes
    // or caller-supplied authoritative effects enter this relation.
    let batch = authenticate_batch_for_proof(
        stored.context().chain_id,
        stored.raw_transactions().to_vec(),
        super::auth_budget(),
    )?;
    let plan = NovTransferPlan::compile(batch, *stored.context(), policy, super::plan_budget())?;
    ensure!(
        plan.commitment() == stored.plan_commitment(),
        "proof policy/raw/context differs from archived plan"
    );
    let mut capture = plan.begin_capture(super::capture_budget())?;
    loop {
        match capture.advance(64)? {
            CaptureStep::More => (),
            CaptureStep::Complete => return capture.finish()?.execution_proof_input(),
            CaptureStep::NeedRead => {
                let hashes = capture
                    .next_request()?
                    .context("proof capture missing read request")?;
                let values = admitted(|| io.try_read_nodes(hashes.clone()))?;
                capture.accept(values)?;
            }
        }
    }
}

fn encode_record(
    candidate: [u8; 32],
    document: [u8; 32],
    image: &[u32; 8],
    journal: &[u8; JOURNAL_BYTES],
    receipt: &[u8],
) -> Result<Vec<u8>> {
    ensure!(
        receipt.starts_with(b"AORCP002") && receipt.len() <= MAX_RECEIPT,
        "invalid proof receipt envelope/bound"
    );
    let mut out = Vec::with_capacity(HEADER + receipt.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&candidate);
    out.extend_from_slice(&document);
    for word in image {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out.extend_from_slice(journal);
    out.extend_from_slice(receipt);
    Ok(out)
}

fn decode_record<'a>(
    blob: &'a [u8],
    candidate: [u8; 32],
    document: [u8; 32],
    image: &[u32; 8],
    journal: &[u8; JOURNAL_BYTES],
) -> Result<&'a [u8]> {
    ensure!(
        (HEADER + 8..=HEADER + MAX_RECEIPT).contains(&blob.len()),
        "stored proof length invalid"
    );
    let expected = encode_record(candidate, document, image, journal, b"AORCP002")?;
    ensure!(
        blob[..HEADER] == expected[..HEADER],
        "stored proof candidate/document/image/journal mismatch"
    );
    ensure!(
        blob[HEADER..].starts_with(b"AORCP002"),
        "stored proof version mismatch"
    );
    Ok(&blob[HEADER..])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_receipt_binding_rejects_each_field_and_bad_bounds() {
        let journal = [4; JOURNAL_BYTES];
        let blob = encode_record([1; 32], [2; 32], &[3; 8], &journal, b"AORCP002test").unwrap();
        assert_eq!(
            decode_record(&blob, [1; 32], [2; 32], &[3; 8], &journal).unwrap(),
            b"AORCP002test"
        );
        for offset in [0, 8, 40, 72, 104, 136, 168, 200, 240, HEADER] {
            let mut bad = blob.clone();
            bad[offset] ^= 1;
            assert!(decode_record(&bad, [1; 32], [2; 32], &[3; 8], &journal).is_err());
        }
        assert!(decode_record(&blob[..HEADER], [1; 32], [2; 32], &[3; 8], &journal).is_err());
        assert!(encode_record([1; 32], [2; 32], &[3; 8], &journal, b"AORCP001").is_err());
        // Envelope checks are NOT cryptographic verification. Payload tampering
        // must reach the real AOEM verifier, not be blessed by this codec.
        let mut altered = blob;
        *altered.last_mut().unwrap() ^= 1;
        assert!(decode_record(&altered, [1; 32], [2; 32], &[3; 8], &journal).is_ok());
    }
}
