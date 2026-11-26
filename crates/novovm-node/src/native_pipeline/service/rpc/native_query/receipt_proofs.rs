use super::*;
use crate::native_pipeline::consensus::statement::{finalized_header, ReceiptFinalityProof};
use novovm_protocol::native_query::{ArchiveChunk, ArchiveSelector, ARCHIVE_CHUNK_BYTES};
use sha2::{Digest, Sha256};

/// One short-lived response buffer and one read, shared by the existing query
/// owner. The buffer is derived from the original archive and carries no head
/// or consensus authority. An admitted read drains even if its client leaves.
#[derive(Default)]
pub(in super::super) struct ArchiveQueries {
    reading: Option<(u64, ProofKind, ArchiveRead)>,
    completed: Option<Completed>,
}

struct Completed {
    height: u64,
    kind: ProofKind,
    expires: Instant,
    proof: Option<EncodedProof>,
}

struct EncodedProof {
    digest: Hash,
    bytes: Vec<u8>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ProofKind {
    AssetReceipt,
    FinalizedHeader,
}

impl ArchiveQueries {
    pub(super) fn poll(&mut self, node: &ResidentNode, now: Instant) {
        if self
            .completed
            .as_ref()
            .is_some_and(|done| done.expires <= now)
        {
            self.completed = None;
        }
        let Some((height, kind, read)) = &mut self.reading else {
            return;
        };
        let block = match read.poll(&node.pipeline) {
            Ok(None) => return,
            Ok(Some(block)) => block,
            Err(_) => None,
        };
        let proof = block.and_then(|block| {
            let bytes = match kind {
                ProofKind::AssetReceipt => {
                    ReceiptFinalityProof::from_archive(&block).and_then(|proof| proof.encode())
                }
                ProofKind::FinalizedHeader => finalized_header::encode(&block),
            }
            .ok()?;
            Some(EncodedProof {
                digest: Sha256::digest(&bytes).into(),
                bytes,
            })
        });
        self.completed = Some(Completed {
            height: *height,
            kind: *kind,
            expires: now + Duration::from_secs(30),
            proof,
        });
        self.reading = None;
    }

    pub(super) fn answer(
        &mut self,
        node: &ResidentNode,
        request: ArchiveSelector,
        kind: ProofKind,
    ) -> Answer {
        let unavailable = Answer::Error(QueryError::Unavailable);
        if kind == ProofKind::AssetReceipt && !node.public_asset_receipt_proofs {
            return unavailable;
        }
        if let Some(done) = self
            .completed
            .as_ref()
            .filter(|done| done.height == request.height && done.kind == kind)
        {
            let Some(proof) = &done.proof else {
                return unavailable;
            };
            let start = request.offset as usize;
            if start >= proof.bytes.len() || (start != 0 && request.digest != proof.digest) {
                return unavailable;
            }
            let limit = (start + ARCHIVE_CHUNK_BYTES).min(proof.bytes.len());
            let chunk = ArchiveChunk {
                digest: proof.digest,
                total_bytes: proof.bytes.len() as u32,
                data: proof.bytes[start..limit].to_vec(),
            };
            return match kind {
                ProofKind::AssetReceipt => Answer::ReceiptProof(chunk),
                ProofKind::FinalizedHeader => Answer::FinalizedHeader(chunk),
            };
        }
        if request.offset != 0 {
            return unavailable;
        }
        let busy = Answer::Error(QueryError::Busy);
        if self.reading.is_some() {
            return busy;
        }
        let core = &node.controller;
        let Some(head) = core.head().filter(|head| request.height <= head.height) else {
            return unavailable;
        };
        let Ok(read) = ArchiveRead::new(
            request.height,
            head,
            core.context(),
            node.validators.clone(),
        ) else {
            return unavailable;
        };
        self.completed = None;
        self.reading = Some((request.height, kind, read));
        busy
    }
}
