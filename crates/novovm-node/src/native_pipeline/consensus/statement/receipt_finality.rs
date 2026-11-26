//! Portable inclusion evidence for asset receipts in the existing linear receipt commitment.
use super::{BatchContext, BlockStatement, ConsensusContext, Hash, ParentPoint, ValidatorSet};
use crate::native_pipeline::{
    business::nov_transfer_batch::{
        authorization_code, authorization_error, receipt_codec, uca_receipt_codec, SpendOutput,
    },
    consensus::{chain::decision_id, wire, ArchiveBlock},
    execution::plan::executed_statement_commitment,
    persistence::{
        decode_asset_receipt_view,
        packet::{receipt_hash_with_spend, ReceiptView},
    },
};
use anyhow::{ensure, Context, Result};
use novovm_consensus::round_bft::VerifiedDecision;
use novovm_protocol::native_query::MAX_ARCHIVE_PROOF_BYTES as MAX_PROOF_BYTES;
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"NVRCPF01";
const MAX_RECEIPTS: usize = 65_536;

/// Untrusted evidence. Only `verify` associates a receipt with a pinned finalized chain.
pub struct ReceiptFinalityProof {
    header: ReceiptProofHeader,
    proposal: Vec<u8>,
    precommits: Vec<u8>,
    receipts: Vec<Vec<u8>>,
    spend_outputs: Option<Vec<SpendOutput>>,
}

#[derive(Serialize, Deserialize)]
struct ReceiptProofHeader {
    execution: BatchContext,
    candidate_id: Hash,
    plan_commitment: Hash,
    state_root: Hash,
    document_digest: Hash,
}

pub struct VerifiedReceiptFinality {
    receipt: ReceiptView,
    block: ParentPoint,
}

impl VerifiedReceiptFinality {
    pub fn receipt(&self) -> &ReceiptView {
        &self.receipt
    }

    pub fn block(&self) -> &ParentPoint {
        &self.block
    }
}

impl ReceiptFinalityProof {
    pub fn from_archive(block: &ArchiveBlock) -> Result<Self> {
        let stored = block.stored();
        let execution = *stored.context();
        check_receipt_codec(execution.receipt_codec)?;
        let proposal = wire::encode_proposal(block.proposal())?;
        let precommits = wire::encode_quorum(block.certificate())?;
        let receipts = stored.receipt_bytes();
        let outputs = stored.spend_outputs();
        check_witness_budget(
            execution.receipt_codec,
            receipts,
            outputs,
            proposal.len(),
            precommits.len(),
        )?;
        Ok(Self {
            header: ReceiptProofHeader {
                execution,
                candidate_id: stored.candidate_id(),
                plan_commitment: stored.plan_commitment(),
                state_root: stored.state_root(),
                document_digest: stored.document_digest(),
            },
            proposal,
            precommits,
            receipts: receipts.to_vec(),
            spend_outputs: outputs.map(<[_]>::to_vec),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.check_budget()?;
        let mut bytes = postcard::to_extend(&self.header, MAGIC.to_vec())?;
        append_blob(&mut bytes, &self.proposal);
        append_blob(&mut bytes, &self.precommits);
        bytes.extend_from_slice(&(self.receipts.len() as u32).to_be_bytes());
        for receipt in &self.receipts {
            append_blob(&mut bytes, receipt);
        }
        if let Some(outputs) = &self.spend_outputs {
            for output in outputs {
                bytes.extend_from_slice(&authorization_code(output.failure).to_be_bytes());
                bytes.extend_from_slice(&output.remaining.to_be_bytes());
            }
        }
        ensure!(
            bytes.len() <= MAX_PROOF_BYTES,
            "encoded receipt proof exceeds its byte limit"
        );
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_PROOF_BYTES,
            "encoded receipt proof exceeds its byte limit"
        );
        let payload = bytes
            .strip_prefix(MAGIC)
            .context("unrecognized receipt proof")?;
        let (header, mut tail): (ReceiptProofHeader, _) = postcard::take_from_bytes(payload)?;
        check_receipt_codec(header.execution.receipt_codec)?;
        let proposal = read_blob(&mut tail, wire::MAX_WIRE_BYTES)?;
        let precommits = read_blob(&mut tail, wire::MAX_WIRE_BYTES)?;
        let count = read_u32(&mut tail)?;
        ensure!(
            (1..=MAX_RECEIPTS).contains(&count),
            "receipt proof row limit"
        );
        let is_uca = header.execution.receipt_codec == uca_receipt_codec();
        let min_row_len = if is_uca { 37 } else { 5 };
        ensure!(
            count <= tail.len() / min_row_len,
            "truncated receipt proof rows"
        );
        let mut receipts = Vec::with_capacity(count);
        for _ in 0..count {
            receipts.push(read_blob(&mut tail, MAX_PROOF_BYTES)?.to_vec());
        }
        let spend_outputs = if is_uca {
            ensure!(
                tail.len() == count * 32,
                "invalid receipt proof spend output length"
            );
            let mut outputs = Vec::with_capacity(count);
            for record in tail.chunks_exact(32) {
                outputs.push(SpendOutput {
                    failure: authorization_error(u128::from_be_bytes(record[..16].try_into()?))?,
                    remaining: u128::from_be_bytes(record[16..].try_into()?),
                });
            }
            Some(outputs)
        } else {
            ensure!(tail.is_empty(), "trailing receipt proof bytes");
            None
        };
        let proof = Self {
            header,
            proposal: proposal.to_vec(),
            precommits: precommits.to_vec(),
            receipts,
            spend_outputs,
        };
        proof.check_budget()?;
        Ok(proof)
    }

    /// The caller supplies the chain context, validator set and verified parent independently.
    /// The result proves receipt inclusion and BFT finality under those pins.
    pub fn verify(
        &self,
        set: &ValidatorSet,
        expected_context: ConsensusContext,
        parent: &ParentPoint,
        expected_transaction: Hash,
    ) -> Result<VerifiedReceiptFinality> {
        let header = &self.header;
        self.check_budget()?;
        ensure!(expected_transaction != [0; 32], "missing transaction pin");
        ensure!(
            parent.height.checked_add(1) == Some(expected_context.height),
            "receipt proof does not extend the pinned parent height"
        );
        let receipt_batch_commitment = receipt_hash_with_spend(
            header.execution.receipt_codec,
            &self.receipts,
            self.spend_outputs.as_deref(),
        );
        let transaction_count = u64::try_from(self.receipts.len())?;
        let statement = BlockStatement {
            consensus: expected_context,
            execution: header.execution,
            candidate_id: header.candidate_id,
            state_root: header.state_root,
            receipt_batch_commitment,
            execution_statement: executed_statement_commitment(
                header.plan_commitment,
                header.state_root,
                receipt_batch_commitment,
            ),
            document_digest: header.document_digest,
            transaction_count,
            state_version: parent
                .state_version
                .checked_add(transaction_count)
                .context("receipt proof state version overflow")?,
            hash: [0; 32],
        }
        .checked(set, parent)?;
        VerifiedDecision::verify(
            &wire::decode_proposal(&self.proposal)?,
            &wire::decode_quorum(&self.precommits)?,
            set,
            expected_context,
            statement.hash(),
        )?;

        let mut matched = None;
        for bytes in &self.receipts {
            let receipt = decode_asset_receipt_view(bytes)?;
            if receipt.tx_hash == expected_transaction {
                ensure!(
                    matched.is_none(),
                    "transaction has multiple committed receipts"
                );
                matched = Some(receipt);
            }
        }
        Ok(VerifiedReceiptFinality {
            receipt: matched
                .context("expected transaction missing from the committed receipt list")?,
            block: ParentPoint {
                height: expected_context.height,
                block_hash: statement.hash(),
                state_root: header.state_root,
                receipt_batch_commitment,
                state_version: statement.state_version(),
                decision_hash: decision_id(&expected_context, statement.hash()),
            },
        })
    }

    fn check_budget(&self) -> Result<()> {
        check_witness_budget(
            self.header.execution.receipt_codec,
            &self.receipts,
            self.spend_outputs.as_deref(),
            self.proposal.len(),
            self.precommits.len(),
        )
    }
}

fn check_receipt_codec(codec: Hash) -> Result<()> {
    ensure!(
        codec == receipt_codec() || codec == uca_receipt_codec(),
        "unsupported asset receipt codec"
    );
    Ok(())
}

fn check_witness_budget(
    codec: Hash,
    receipts: &[Vec<u8>],
    spend_outputs: Option<&[SpendOutput]>,
    proposal: usize,
    precommits: usize,
) -> Result<()> {
    check_receipt_codec(codec)?;
    ensure!(
        (1..=MAX_RECEIPTS).contains(&receipts.len()),
        "receipt proof row limit"
    );
    ensure!(
        (1..=wire::MAX_WIRE_BYTES).contains(&proposal)
            && (1..=wire::MAX_WIRE_BYTES).contains(&precommits),
        "receipt proof consensus wire limit"
    );
    let output_bytes = match spend_outputs {
        Some(outputs) if codec == uca_receipt_codec() && outputs.len() == receipts.len() => {
            outputs.len() * 32
        }
        None if codec == receipt_codec() => 0,
        _ => anyhow::bail!("receipt proof outputs differ from the asset codec"),
    };
    // Reserve space for the fixed header and account for every row's length prefix.
    let mut bytes = 1024 + proposal + precommits + output_bytes;
    for receipt in receipts {
        ensure!(!receipt.is_empty(), "empty receipt proof row");
        bytes = bytes.saturating_add(4).saturating_add(receipt.len());
        ensure!(bytes <= MAX_PROOF_BYTES, "receipt proof byte limit");
    }
    Ok(())
}

fn append_blob(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    target.extend_from_slice(bytes);
}

fn read_u32(input: &mut &[u8]) -> Result<usize> {
    let (length, tail) = input
        .split_first_chunk::<4>()
        .context("truncated receipt proof length")?;
    *input = tail;
    Ok(u32::from_be_bytes(*length) as usize)
}

fn read_blob<'a>(input: &mut &'a [u8], limit: usize) -> Result<&'a [u8]> {
    let length = read_u32(input)?;
    ensure!((1..=limit).contains(&length), "receipt proof field limit");
    let (field, tail) = input
        .split_at_checked(length)
        .context("truncated receipt proof field")?;
    *input = tail;
    Ok(field)
}
