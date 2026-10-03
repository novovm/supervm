//! Read-only native candidate qualification for the versioned round-BFT kernel.
//!
//! This is a diagnostic/pre-integration surface, NOT protocol activation. It
//! neither signs nor registers, promotes, publishes or finalizes a candidate.
//! The existing V3 publisher cannot consume its result. In particular a valid
//! decision below is not evidence of a durable local signer journal/decision.
//! Successors are deliberately unsupported until their versioned parent proof,
//! promotion intent, publication and recovery contracts are integrated together.

use crate::native_block_ledger::{
    NovNativeBlockExecutionEvidenceV1, NovNativeBlockHeaderV1, NovNativeDurableBlockV1,
};
use crate::tx_ingress::candidate_workspace::{self, IsolatedBlockArtifactV1};
use crate::tx_ingress::fresh_genesis::FreshGenesisConfigV1;
use anyhow::{ensure, Context as _, Result};
use novovm_consensus::round_bft::{wire, VerifiedDecision};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Separate from both the original native V3 seal and the replacement runtime's
/// business statement. Recognizing this name does not enable it in genesis.
pub const NATIVE_ROUND_BFT_PROFILE_V1: &str = "novovm-native-round-bft-profile/v1";
pub const NATIVE_ROUND_BFT_STATEMENT_V1: &str = "novovm-native-round-bft-statement/v1";

/// A freshly verified historical read, not a live current-parent capability.
/// No deserializer or public constructor can bypass durable candidate readback.
/// Local locator/digest fields remain in `artifact`, never in the signed value.
#[derive(Debug)]
pub struct ExecutedGenesisCandidateV1 {
    artifact: IsolatedBlockArtifactV1,
    context: wire::Context,
    value: wire::Hash,
    validators: wire::ValidatorSet,
}

impl ExecutedGenesisCandidateV1 {
    pub fn context(&self) -> wire::Context {
        self.context
    }

    pub fn value(&self) -> wire::Hash {
        self.value
    }

    pub fn profile_commitment(&self) -> wire::Hash {
        self.context.protocol_commitment
    }

    pub fn validators(&self) -> &wire::ValidatorSet {
        &self.validators
    }

    pub fn block(&self) -> &NovNativeDurableBlockV1 {
        self.artifact.block()
    }

    pub fn workspace_id(&self) -> [u8; 32] {
        self.artifact.workspace_id
    }

    pub fn plan_commitment(&self) -> [u8; 32] {
        self.artifact.plan_commitment
    }

    pub fn output_digest(&self) -> [u8; 32] {
        self.artifact.output_digest
    }
}

/// Verification result only. This carries no durable ACK, ledger membership,
/// current-parent lock, signer permission, publication or finality authority.
#[derive(Debug)]
pub struct VerifiedGenesisDecisionV1 {
    candidate: ExecutedGenesisCandidateV1,
    decision: VerifiedDecision,
}

impl VerifiedGenesisDecisionV1 {
    pub fn candidate(&self) -> &ExecutedGenesisCandidateV1 {
        &self.candidate
    }

    pub fn decision(&self) -> &VerifiedDecision {
        &self.decision
    }
}

fn hash_parts(domain: &[u8], parts: &[&[u8]]) -> wire::Hash {
    let mut hash = Sha256::new();
    hash.update(domain);
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    hash.finalize().into()
}

/// Compile an explicitly distinct diagnostic profile from the original pinned
/// business/genesis configuration and exact consensus/statement versions. This
/// does not mutate `config.protocol_config_commitment` or choose a protocol from
/// received evidence. Production activation needs a separate reviewed pin.
pub fn native_round_bft_profile_commitment_v1(config: &FreshGenesisConfigV1) -> Result<wire::Hash> {
    let genesis = config.compile()?;
    Ok(profile_commitment(
        config.protocol_config_commitment,
        genesis.config_commitment(),
    ))
}

fn profile_commitment(business: wire::Hash, genesis: wire::Hash) -> wire::Hash {
    hash_parts(
        b"novovm-native-round-bft-profile-commitment-v1\0",
        &[
            NATIVE_ROUND_BFT_PROFILE_V1.as_bytes(),
            wire::PROTOCOL.as_bytes(),
            NATIVE_ROUND_BFT_STATEMENT_V1.as_bytes(),
            &business,
            &genesis,
        ],
    )
}

/// The statement's v1 codec is compact serde JSON of these fixed-order typed
/// fields, with no maps/floats. Both embedded original structs are the fixed V1
/// block/evidence codecs. Changes to their serialized contract require a new
/// statement/profile version, not an unnoticed change of an existing value.
/// The full header binds slot/time, execution context, body/ordered-tx roots,
/// state/receipt roots and codecs, AOEM IDs/evidence and counts. Evidence adds
/// every per-transaction receipt commitment. The loader verifies the actual
/// body and AOEM output before these fields can enter this structure.
#[derive(Serialize)]
struct NativeStatementV1<'a> {
    schema: &'static str,
    context: wire::Context,
    header: &'a NovNativeBlockHeaderV1,
    execution_evidence: &'a NovNativeBlockExecutionEvidenceV1,
}

/// Re-read a completed AOEM workspace and independently verify its archived
/// inputs/output, authentication, roots, receipts and completion marker through
/// the original loader. Caller-supplied expected hashes are not accepted.
///
/// This is a historical read; the workspace can subsequently be retired or the
/// live head can change. A future signer/publisher MUST revalidate under the
/// original authority/ledger locks and prove durable decision ownership. This
/// function intentionally creates no bridge into those writable scopes.
pub fn load_executed_genesis_candidate_v1(
    config: &FreshGenesisConfigV1,
    workspace_id: [u8; 32],
    params: &serde_json::Value,
) -> Result<ExecutedGenesisCandidateV1> {
    let genesis = config.compile()?;
    let artifact =
        candidate_workspace::load_block_artifact_v1(config.chain_id, workspace_id, params)?
            .context("round-BFT candidate requires complete verified AOEM workspace output")?;
    ensure!(
        artifact.fresh_genesis_identity() == Some(&genesis.identity()),
        "round-BFT candidate differs from pinned fresh genesis identity"
    );
    let block = artifact.block();
    let header = &block.header;
    ensure!(
        header.chain_id == config.chain_id
            && header.height == 1
            && header.parent_block_hash == [0; 32]
            && header.aoem_parent.is_none(),
        "round-BFT read-only adapter supports fresh height one only; no V3 parent conversion"
    );
    let roots = genesis.root_codec_profile();
    ensure!(
        header.pre_state_root == genesis.state_root()
            && header.post_state_root_codec == roots.state_root_codec()
            && header.cumulative_receipt_root_codec == roots.receipt_root_codec(),
        "round-BFT candidate genesis state or root codec mismatch"
    );
    ensure!(
        header.aoem_readback_verified
            && !header.safe
            && !header.finalized
            && !header.proof_sealed
            && !block.execution_evidence.proof_sealed,
        "round-BFT adapter requires verified unsealed candidate evidence"
    );
    let validators = wire::ValidatorSet::new(
        config.chain_id,
        1,
        1,
        config
            .validators
            .iter()
            .map(|member| wire::Validator::new(member.public_key, member.weight))
            .collect::<Result<Vec<_>>>()?,
    )?;
    let context = wire::Context {
        chain_id: config.chain_id,
        genesis_config_commitment: genesis.config_commitment(),
        protocol_commitment: profile_commitment(
            config.protocol_config_commitment,
            genesis.config_commitment(),
        ),
        epoch: 1,
        validator_set_hash: validators.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    context.validate(&validators)?;
    let statement = serde_json::to_vec(&NativeStatementV1 {
        schema: NATIVE_ROUND_BFT_STATEMENT_V1,
        context,
        header,
        execution_evidence: &block.execution_evidence,
    })?;
    let value = hash_parts(
        b"novovm-native-round-bft-statement-value-v1\0",
        &[&statement],
    );
    Ok(ExecutedGenesisCandidateV1 {
        artifact,
        context,
        value,
        validators,
    })
}

/// Verify exact new-version evidence against a newly reloaded local candidate.
/// No V3 witness coercion, signing, journal ACK or original-ledger write occurs.
pub fn verify_genesis_decision_v1(
    config: &FreshGenesisConfigV1,
    workspace_id: [u8; 32],
    params: &serde_json::Value,
    proposal: &wire::Proposal,
    certificate: &wire::Quorum,
) -> Result<VerifiedGenesisDecisionV1> {
    let candidate = load_executed_genesis_candidate_v1(config, workspace_id, params)?;
    let decision = VerifiedDecision::verify(
        proposal,
        certificate,
        candidate.validators(),
        candidate.context(),
        candidate.value(),
    )?;
    Ok(VerifiedGenesisDecisionV1 {
        candidate,
        decision,
    })
}
