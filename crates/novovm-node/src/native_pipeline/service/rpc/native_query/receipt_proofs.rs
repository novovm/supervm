use super::*;
use crate::native_pipeline::consensus::statement::ReceiptFinalityProof;
use novovm_protocol::native_query::{ArchiveChunk, ArchiveSelector, ARCHIVE_CHUNK_BYTES};
use sha2::{Digest, Sha256};

/// One short-lived response buffer and one read, shared by the existing query
/// owner. The buffer is derived from the original archive and carries no head
/// or consensus authority. An admitted read drains even if its client leaves.
#[derive(Default)]
pub(in super::super) struct ReceiptProofQueries {
    reading: Option<(u64, ArchiveRead)>,
    completed: Option<Completed>,
}

struct Completed {
    height: u64,
    expires: Instant,
    proof: Option<EncodedProof>,
}

struct EncodedProof {
    digest: Hash,
    bytes: Vec<u8>,
}

impl ReceiptProofQueries {
    pub(super) fn poll(&mut self, node: &ResidentNode, now: Instant) {
        if self
            .completed
            .as_ref()
            .is_some_and(|done| done.expires <= now)
        {
            self.completed = None;
        }
        let Some((height, read)) = &mut self.reading else {
            return;
        };
        let block = match read.poll(&node.pipeline) {
            Ok(None) => return,
            Ok(Some(block)) => block,
            Err(_) => None,
        };
        let proof = block.and_then(|block| {
            let bytes = ReceiptFinalityProof::from_archive(&block)
                .ok()?
                .encode()
                .ok()?;
            Some(EncodedProof {
                digest: Sha256::digest(&bytes).into(),
                bytes,
            })
        });
        self.completed = Some(Completed {
            height: *height,
            expires: now + Duration::from_secs(30),
            proof,
        });
        self.reading = None;
    }

    pub(super) fn answer(&mut self, node: &ResidentNode, request: ArchiveSelector) -> Answer {
        let unavailable = Answer::Error(QueryError::Unavailable);
        if !node.public_asset_receipt_proofs {
            return unavailable;
        }
        if let Some(done) = self
            .completed
            .as_ref()
            .filter(|done| done.height == request.height)
        {
            let Some(proof) = &done.proof else {
                return unavailable;
            };
            let start = request.offset as usize;
            if start >= proof.bytes.len() || (start != 0 && request.digest != proof.digest) {
                return unavailable;
            }
            let limit = (start + ARCHIVE_CHUNK_BYTES).min(proof.bytes.len());
            return Answer::ReceiptProof(ArchiveChunk {
                digest: proof.digest,
                total_bytes: proof.bytes.len() as u32,
                data: proof.bytes[start..limit].to_vec(),
            });
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
        self.reading = Some((request.height, read));
        busy
    }
}
