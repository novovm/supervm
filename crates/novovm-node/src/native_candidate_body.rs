//! Bounded transaction-body transfer under an already authenticated proposal.
//! No signatures, state writes or finality. The caller bounds active assemblers
//! and their lifetime; source identities must come from authenticated transport.
use crate::native_block_ledger::{
    body_digest_v1, nov_native_ordered_tx_root_v1, NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1,
    NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1,
};
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_block_seal::round_wire::decode_nov_native_seal_round_wire_v1;
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use anyhow::{bail, Context, Result};
#[path = "native_candidate_body_network.rs"]
pub mod network;

const MAGIC: &[u8; 8] = b"NOVBODY1";
const HEADER: usize = 8 + 32 + 4;
const CHUNK: usize = 64 * 1024;
const _: () = assert!(HEADER + CHUNK <= crate::product_mainline_overlay::PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1);

/// Fields cannot be constructed from unchecked bytes. Execution and comparison
/// with the signed output subject remain mandatory before any vote.
pub struct VerifiedCandidateBodyV1 {
    pub(crate) message: Message,
    pub(crate) raw_txs: Vec<Vec<u8>>,
    pub(crate) authority_commitment: [u8; 32],
}

pub struct CandidateBodyAssemblerV1 {
    message: Message,
    source: String,
    authority_commitment: [u8; 32],
    total: usize,
    chunks: Vec<Option<Vec<u8>>>,
    received: usize,
    closed: bool,
}

impl CandidateBodyAssemblerV1 {
    /// Existing signed proposal wire is the manifest. Reject non-proposals,
    /// wrong leader/source/domain and excessive bounds before body allocation.
    pub fn new(
        proposal_wire: &[u8],
        authority: &NovNativeSealEpochAuthorityV1,
        height: u64,
        authenticated_source: &str,
    ) -> Result<Self> {
        let message = decode_nov_native_seal_round_wire_v1(
            proposal_wire,
            authority,
            height,
            authenticated_source,
        )?;
        let Message::Proposal { proposal, .. } = &message else {
            bail!("body transfer requires a signed proposal");
        };
        let subject = &proposal.subject;
        let count = subject.tx_count as usize;
        let bytes = usize::try_from(subject.body_bytes)?;
        if count == 0
            || count > NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1
            || bytes < count
            || bytes > NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1
        {
            bail!("body manifest exceeds transaction bounds");
        }
        let total = 4 + count * 4 + bytes;
        Ok(Self {
            message,
            source: authenticated_source.to_owned(),
            authority_commitment: authority.authority_commitment,
            total,
            chunks: vec![None; total.div_ceil(CHUNK)],
            received: 0,
            closed: false,
        })
    }

    /// Outbound chunks bind the signed proposal hash and exact byte offsets.
    pub fn encode_chunks(&self, raw_txs: &[Vec<u8>]) -> Result<Vec<Vec<u8>>> {
        self.validate_body(raw_txs)?;
        let mut body = Vec::with_capacity(self.total);
        body.extend_from_slice(&(raw_txs.len() as u32).to_be_bytes());
        for raw in raw_txs {
            body.extend_from_slice(&(raw.len() as u32).to_be_bytes());
            body.extend_from_slice(raw);
        }
        Ok(body
            .chunks(CHUNK)
            .enumerate()
            .map(|(index, chunk)| {
                let mut packet = Vec::with_capacity(HEADER + chunk.len());
                packet.extend_from_slice(MAGIC);
                packet.extend_from_slice(
                    &self
                        .message
                        .proposal()
                        .expect("proposal checked")
                        .proposal_hash,
                );
                packet.extend_from_slice(&(index as u32).to_be_bytes());
                packet.extend_from_slice(chunk);
                packet
            })
            .collect())
    }

    /// Out-of-order delivery and identical duplicates are supported. Conflicting
    /// duplicates or a completed invalid body close this assembly, never execute.
    pub fn push(
        &mut self,
        authenticated_source: &str,
        packet: &[u8],
    ) -> Result<Option<VerifiedCandidateBodyV1>> {
        if self.closed {
            bail!("body assembly is closed");
        }
        if authenticated_source != self.source
            || packet.len() <= HEADER
            || packet.len() > HEADER + CHUNK
            || &packet[..8] != MAGIC
            || packet[8..40]
                != self
                    .message
                    .proposal()
                    .expect("proposal checked")
                    .proposal_hash
        {
            bail!("body fragment source, domain or length mismatch");
        }
        let index = u32::from_be_bytes(packet[40..44].try_into()?) as usize;
        if index >= self.chunks.len()
            || packet.len() - HEADER != CHUNK.min(self.total - index * CHUNK)
        {
            bail!("body fragment offset or length mismatch");
        }
        let bytes = &packet[HEADER..];
        if let Some(previous) = &self.chunks[index] {
            if previous != bytes {
                self.closed = true;
                bail!("conflicting body fragment");
            }
            return Ok(None);
        }
        self.chunks[index] = Some(bytes.to_vec());
        self.received += 1;
        if self.received != self.chunks.len() {
            return Ok(None);
        }
        self.closed = true;
        let mut body = Vec::with_capacity(self.total);
        for chunk in &mut self.chunks {
            body.extend_from_slice(&chunk.take().context("body chunk missing")?);
        }
        let raw_txs = decode_body(&body)?;
        self.validate_body(&raw_txs)?;
        Ok(Some(VerifiedCandidateBodyV1 {
            message: self.message.clone(),
            raw_txs,
            authority_commitment: self.authority_commitment,
        }))
    }

    fn validate_body(&self, raw_txs: &[Vec<u8>]) -> Result<()> {
        let subject = &self
            .message
            .proposal()
            .context("body proposal missing")?
            .subject;
        if raw_txs.len() != subject.tx_count as usize
            || raw_txs.iter().any(Vec::is_empty)
            || raw_txs
                .iter()
                .try_fold(0usize, |n, raw| n.checked_add(raw.len()))
                != Some(subject.body_bytes as usize)
        {
            bail!("body does not match signed lengths");
        }
        let hashes = raw_txs
            .iter()
            .map(|raw| crate::tx_ingress::canonical_nov_native_tx_hash_from_payload_v1(raw))
            .collect::<Result<Vec<_>>>()?;
        if nov_native_ordered_tx_root_v1(&hashes)? != subject.ordered_tx_root
            || body_digest_v1(&hashes, raw_txs) != subject.body_digest
        {
            bail!("body does not match signed commitments");
        }
        Ok(())
    }
}

fn decode_body(mut body: &[u8]) -> Result<Vec<Vec<u8>>> {
    fn take_len(body: &mut &[u8]) -> Result<usize> {
        let bytes = body.get(..4).context("truncated body length")?;
        let len = u32::from_be_bytes(bytes.try_into()?) as usize;
        *body = &body[4..];
        Ok(len)
    }
    let count = take_len(&mut body)?;
    if count == 0 || count > NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1 {
        bail!("invalid body transaction count");
    }
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let len = take_len(&mut body)?;
        if len == 0 {
            bail!("empty body transaction");
        }
        result.push(
            body.get(..len)
                .context("truncated body transaction")?
                .to_vec(),
        );
        body = &body[len..];
    }
    if !body.is_empty() {
        bail!("trailing body bytes");
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_body_parser_rejects_truncation_counts_lengths_and_trailing_bytes() {
        let valid = [0, 0, 0, 1, 0, 0, 0, 1, 7];
        assert_eq!(decode_body(&valid).unwrap(), vec![vec![7]]);
        for length in 0..valid.len() {
            assert!(decode_body(&valid[..length]).is_err());
        }
        let mut trailing = valid.to_vec();
        trailing.push(8);
        assert!(decode_body(&trailing).is_err());
        for count in [0u32, 1025, u32::MAX] {
            assert!(decode_body(&count.to_be_bytes()).is_err());
        }
        for length in [0u32, u32::MAX] {
            let mut bytes = 1u32.to_be_bytes().to_vec();
            bytes.extend_from_slice(&length.to_be_bytes());
            assert!(decode_body(&bytes).is_err());
        }
    }
}
