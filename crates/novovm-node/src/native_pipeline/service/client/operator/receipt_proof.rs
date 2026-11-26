use super::*;
use crate::native_pipeline::consensus::{
    statement::{ParentPoint, ReceiptFinalityProof},
    wire::Context as ConsensusContext,
};
use novovm_consensus::round_bft::crypto::{ConsensusVerifier, MLDSA65_CONTEXT};
use novovm_protocol::native_query::{ArchiveSelector, QueryError, MAX_ARCHIVE_PROOF_BYTES};
use sha2::{Digest, Sha256};

struct ProofVerifier(TransportIdentity);

impl ConsensusVerifier for ProofVerifier {
    fn verify_mldsa65(&self, public: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
        self.0.verify(public, message, MLDSA65_CONTEXT, signature)
    }
}

/// Fetch the next block's evidence and independently verify one asset receipt.
/// The caller obtains the prior verified chain point from local trusted state,
/// independently of the responding service and of this proof download.
pub fn verified_receipt(config_path: &Path, parent_path: &Path, hash: &str) -> Result<Value> {
    use crate::native_pipeline::service::{config::decode_hex, rpc::hex};
    let transaction: [u8; 32] = decode_hex(hash)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected a 32-byte transaction hash"))?;
    ensure!(transaction != [0; 32], "missing transaction pin");
    let parent: ParentPoint = serde_json::from_slice(&read_bounded(parent_path, 4096)?)?;
    ensure!(
        parent.state_root != [0; 32]
            && parent.receipt_batch_commitment != [0; 32]
            && (parent.height == 0) == (parent.block_hash == [0; 32])
            && (parent.height == 0) == (parent.decision_hash == [0; 32]),
        "invalid separately verified parent checkpoint"
    );
    let config_path = config_path.canonicalize()?;
    let config = read_client_config(&config_path)?;
    let (identity, domain) = config.identity(config_path.parent().context("config directory")?)?;
    let set = (*domain.validators)
        .clone()
        .with_verifier(Arc::new(ProofVerifier(identity.clone())));
    let context = ConsensusContext {
        chain_id: domain.chain_id,
        genesis_config_commitment: domain.genesis,
        protocol_commitment: domain.protocol,
        epoch: set.epoch(),
        validator_set_hash: set.hash(),
        height: parent
            .height
            .checked_add(1)
            .context("parent height exhausted")?,
        parent_block_hash: parent.block_hash,
        parent_decision_hash: parent.decision_hash,
    };
    let mut client = config.connect(identity, domain)?;
    let bytes = fetch(
        &mut client,
        context.height,
        Instant::now() + Duration::from_millis(config.timeout_ms),
    )?;
    let proof = ReceiptFinalityProof::decode(&bytes)?;
    let verified = proof.verify(&set, context, &parent, transaction)?;
    let receipt = verified.receipt();
    let block = verified.block();
    Ok(json!({
        "independently_verified_finality": true,
        "tx_hash": hex(&receipt.tx_hash),
        "success": receipt.success,
        "nonce_after": receipt.nonce_after,
        "charged_fee": receipt.charged_fee,
        "height": block.height,
        "block_hash": hex(&block.block_hash),
        "state_root": hex(&block.state_root),
        "proof_bytes": bytes.len(),
    }))
}

fn fetch(client: &mut NativeApplicationClient, height: u64, deadline: Instant) -> Result<Vec<u8>> {
    let mut expected_total = None;
    let mut digest = [0; 32];
    let mut bytes = Vec::new();
    loop {
        let request = ArchiveSelector {
            height,
            offset: u32::try_from(bytes.len())?,
            digest,
        };
        let chunk = match authority::query_reply(client, Query::ReceiptProof(request), deadline)? {
            Answer::ReceiptProof(chunk) => chunk,
            Answer::Error(QueryError::Busy) if bytes.is_empty() => {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            _ => anyhow::bail!("requested receipt proof unavailable"),
        };
        let total = chunk.total_bytes as usize;
        ensure!(total <= MAX_ARCHIVE_PROOF_BYTES, "receipt proof byte limit");
        if let Some(expected) = expected_total {
            ensure!(
                expected == total && digest == chunk.digest,
                "receipt proof changed during download"
            );
        } else {
            expected_total = Some(total);
            digest = chunk.digest;
        }
        bytes.extend_from_slice(&chunk.data);
        if bytes.len() == total {
            let observed_digest: [u8; 32] = Sha256::digest(&bytes).into();
            ensure!(
                observed_digest == digest,
                "receipt proof digest differs from response"
            );
            return Ok(bytes);
        }
    }
}
