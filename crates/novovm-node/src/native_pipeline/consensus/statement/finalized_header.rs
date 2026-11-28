//! Wire evidence for the original signed block statement and its state root.
use super::{BatchContext, BlockStatement, ConsensusContext, Hash, ParentPoint, ValidatorSet};
use crate::native_pipeline::consensus::{wire, ArchiveBlock};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"NVFHDR01";
pub(crate) const MAX_BYTES: usize = 1024 + 2 * wire::MAX_WIRE_BYTES;

#[derive(Deserialize, Serialize)]
struct Header {
    execution: BatchContext,
    candidate_id: Hash,
    state_root: Hash,
    receipt_batch_commitment: Hash,
    execution_statement: Hash,
    document_digest: Hash,
    transaction_count: u64,
}

/// Produced only after authenticating the exact executed block against an
/// independently selected parent, validator set, proposal and Precommit QC.
pub(crate) struct VerifiedHeader {
    point: ParentPoint,
    statement: BlockStatement,
}

impl VerifiedHeader {
    pub(crate) fn point(&self) -> ParentPoint {
        self.point
    }

    pub(crate) fn execution_context(&self) -> &BatchContext {
        self.statement.execution_context()
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn check_replayed_candidate(
        &self,
        candidate: &crate::native_pipeline::persistence::PreparedCandidate,
    ) -> Result<()> {
        ensure!(
            self.statement.matches_packet(candidate),
            "independent UCA execution differs from the certified candidate"
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn timestamp_unix_ms(&self) -> u64 {
        self.statement.execution.timestamp_unix_ms
    }

    #[cfg(test)]
    pub(crate) fn direct_nov_expected_journal(
        &self,
        pinned_effect_contract: Hash,
    ) -> Result<crate::native_pipeline::proof::ExecutionJournalV1> {
        use crate::native_pipeline::{
            business::nov_transfer_batch::{program_id, receipt_codec, SEMANTIC_VERSION},
            proof::ExecutionJournalV1,
        };
        let statement = &self.statement;
        let context = statement.execution_context();
        ensure!(
            context.business_program == program_id()
                && context.semantic_version == SEMANTIC_VERSION
                && context.receipt_codec == receipt_codec()
                && context.effect_contract == pinned_effect_contract,
            "certified block does not match the pinned direct NOV proof relation"
        );
        Ok(ExecutionJournalV1::from_direct_statement_commitments(
            statement.candidate_id,
            statement.state_root,
            statement.receipt_batch_commitment,
            statement.execution_statement,
            statement.transaction_count,
            statement.state_version,
        ))
    }
}

pub(crate) fn encode(block: &ArchiveBlock) -> Result<Vec<u8>> {
    let stored = block.stored();
    let header = Header {
        execution: *stored.context(),
        candidate_id: stored.candidate_id(),
        state_root: stored.state_root(),
        receipt_batch_commitment: stored.receipt_batch_commitment(),
        execution_statement: stored.statement_commitment(),
        document_digest: stored.document_digest(),
        transaction_count: u64::try_from(stored.raw_transactions().len())?,
    };
    encode_evidence(header, block.proposal(), block.certificate())
}

fn encode_evidence(
    header: Header,
    proposal: &wire::Proposal,
    quorum: &wire::Quorum,
) -> Result<Vec<u8>> {
    let proposal = wire::encode_proposal(proposal)?;
    let precommits = wire::encode_quorum(quorum)?;
    let mut bytes = postcard::to_extend(&header, MAGIC.to_vec())?;
    bytes.extend_from_slice(&u32::try_from(proposal.len())?.to_be_bytes());
    bytes.extend(proposal);
    bytes.extend(precommits);
    ensure!(bytes.len() <= MAX_BYTES, "finalized header byte limit");
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::{
        persistence::{PacketBudget, PreparedCandidate},
        proof::{self, execute_to_journal},
    };
    use novovm_consensus::round_bft::test_vectors::{sign_proposal, sign_vote, FixtureSigningKey};

    #[test]
    fn certified_direct_nov_header_derives_the_exact_original_guest_journal() -> Result<()> {
        let fixture = proof::tests::fixture();
        let expected = execute_to_journal(&fixture.input)?;
        let packet = PreparedCandidate::from_executed(fixture.executed, PacketBudget::default())?;
        let execution = *packet.context();
        let key = FixtureSigningKey::from_bytes(&[46; 32]);
        let set = ValidatorSet::new_for_test(
            execution.chain_id,
            1,
            1,
            vec![wire::Validator::new(key.verifying_key().to_bytes(), 1)?],
        )?;
        let parent = ParentPoint {
            height: execution.parent_height,
            block_hash: execution.parent_block_hash,
            state_root: execution.parent_state_root,
            receipt_batch_commitment: execution.parent_receipt_root,
            state_version: execution.parent_state_version,
            decision_hash: [0; 32],
        };
        let context = ConsensusContext {
            chain_id: execution.chain_id,
            genesis_config_commitment: execution.genesis_config_commitment,
            protocol_commitment: execution.protocol_commitment,
            epoch: 1,
            validator_set_hash: set.hash(),
            height: execution.height,
            parent_block_hash: parent.block_hash,
            parent_decision_hash: parent.decision_hash,
        };
        let statement = BlockStatement::from_executed(&packet, context, &set, &parent)?;
        let proposal = sign_proposal(context, 0, statement.hash(), None, &set, &key)?;
        let vote = sign_vote(
            context,
            0,
            wire::Phase::Precommit,
            Some(statement.hash()),
            &set,
            &key,
        )?;
        let quorum = wire::Quorum::from_votes(&set, vec![vote])?;
        let header = |state_root| Header {
            execution,
            candidate_id: packet.candidate_id(),
            state_root,
            receipt_batch_commitment: packet.receipt_batch_commitment(),
            execution_statement: packet.statement_commitment(),
            document_digest: packet.document_digest(),
            transaction_count: u64::try_from(packet.transaction_count()).unwrap(),
        };
        let verified = verify(
            &encode_evidence(header(packet.state_root()), &proposal, &quorum)?,
            &set,
            context,
            &parent,
        )?;
        assert_eq!(
            verified
                .direct_nov_expected_journal(execution.effect_contract)?
                .encode(),
            expected.encode()
        );
        assert_eq!(verified.timestamp_unix_ms(), execution.timestamp_unix_ms);
        assert!(verified.direct_nov_expected_journal([0x13; 32]).is_err());
        assert!(verify(
            &encode_evidence(header([0x75; 32]), &proposal, &quorum)?,
            &set,
            context,
            &parent
        )
        .is_err());
        Ok(())
    }
}

/// Neither the sender nor the wire can select the expected chain, set or parent.
pub(crate) fn verify(
    bytes: &[u8],
    set: &ValidatorSet,
    context: ConsensusContext,
    parent: &ParentPoint,
) -> Result<VerifiedHeader> {
    ensure!(bytes.len() <= MAX_BYTES, "finalized header byte limit");
    ensure!(
        parent.height.checked_add(1) == Some(context.height),
        "header height skips verified parent"
    );
    let contents = bytes
        .strip_prefix(MAGIC)
        .context("unrecognized certified header")?;
    let (header, evidence): (Header, _) = postcard::take_from_bytes(contents)?;
    let (proposal_len, evidence) = evidence
        .split_first_chunk::<4>()
        .context("short header evidence")?;
    let (proposal, precommits) = evidence
        .split_at_checked(u32::from_be_bytes(*proposal_len) as usize)
        .context("short header proposal")?;
    ensure!(
        (1..=wire::MAX_WIRE_BYTES).contains(&proposal.len())
            && (1..=wire::MAX_WIRE_BYTES).contains(&precommits.len()),
        "header consensus evidence limit"
    );
    ensure!(
        header.transaction_count > 0
            && header.state_root != [0; 32]
            && header.receipt_batch_commitment != [0; 32]
            && header.document_digest != [0; 32],
        "invalid certified header fields"
    );
    let statement = BlockStatement {
        consensus: context,
        execution: header.execution,
        candidate_id: header.candidate_id,
        state_root: header.state_root,
        receipt_batch_commitment: header.receipt_batch_commitment,
        execution_statement: header.execution_statement,
        document_digest: header.document_digest,
        transaction_count: header.transaction_count,
        state_version: parent
            .state_version
            .checked_add(header.transaction_count)
            .context("header state version exhausted")?,
        hash: [0; 32],
    }
    .checked(set, parent)?;
    let (point, _) = statement.verified_point(set, proposal, precommits)?;
    Ok(VerifiedHeader { point, statement })
}
