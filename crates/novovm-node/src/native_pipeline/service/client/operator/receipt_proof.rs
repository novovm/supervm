use super::*;
use crate::native_pipeline::consensus::{
    statement::{finalized_header, ParentPoint, ReceiptFinalityProof},
    wire::Context as ConsensusContext,
};
use novovm_consensus::round_bft::crypto::{ConsensusVerifier, MLDSA65_CONTEXT};
use novovm_network::product_overlay::TransportVerifier;
use novovm_protocol::native_query::{
    ArchiveChunk, ArchiveSelector, QueryError, MAX_ARCHIVE_PROOF_BYTES,
};
use sha2::{Digest, Sha256};

struct ProofVerifier<V>(V);

impl<V: TransportVerifier> ConsensusVerifier for ProofVerifier<V> {
    fn verify_mldsa65(&self, public: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
        self.0.verify(public, message, MLDSA65_CONTEXT, signature)
    }
}

/// Fetch the next block's evidence using a separately verified local parent.
pub fn verified_receipt(config_path: &Path, parent_path: &Path, hash: &str) -> Result<Value> {
    let transaction = transaction_hash(hash)?;
    let parent = serde_json::from_slice(&read_bounded(parent_path, 4096)?)?;
    let config_path = config_path.canonicalize()?;
    let config = read_client_config(&config_path)?;
    let (identity, domain) = config.identity(config_path.parent().context("config directory")?)?;
    let target = ProofTarget::new(&domain, identity.clone(), parent, transaction)?;
    let mut client = config.connect(identity, domain)?;
    let deadline = Instant::now() + Duration::from_millis(config.timeout_ms);
    let mut download = Download::new(target.height());
    loop {
        let answer = authority::query_reply(&mut client, download.selector()?, deadline)?;
        let busy = matches!(answer, Answer::Error(QueryError::Busy));
        if let Some(bytes) = download.accept(answer)? {
            return target.verify(&bytes);
        }
        if busy {
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// Export the native certificate bytes. The activation reviewer verifies them
/// against its own pinned preceding checkpoint before deriving a subject.
pub fn activation_anchor(config_path: &Path, height: u64, output: &Path) -> Result<Value> {
    use std::io::Write;
    ensure!(height > 0, "activation anchor must be finalized");
    let config_path = config_path.canonicalize()?;
    let config = read_client_config(&config_path)?;
    let (identity, domain) = config.identity(config_path.parent().context("config directory")?)?;
    let mut client = config.connect(identity, domain)?;
    let deadline = Instant::now() + Duration::from_millis(config.timeout_ms);
    let mut download = Download::header(height);
    loop {
        let answer = authority::query_reply(&mut client, download.selector()?, deadline)?;
        let busy = matches!(answer, Answer::Error(QueryError::Busy));
        if let Some(bytes) = download.accept(answer)? {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            return Ok(json!({
                "kind":"native_finalized_header_export",
                "height":height,
                "sha256":format!("{:x}", Sha256::digest(&bytes)),
                "independently_verified_finality":false,
            }));
        }
        if busy {
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

pub(crate) fn transaction_hash(hash: &str) -> Result<[u8; 32]> {
    let transaction: [u8; 32] = crate::native_pipeline::service::config::decode_hex(hash)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected a 32-byte transaction hash"))?;
    ensure!(transaction != [0; 32], "missing transaction pin");
    Ok(transaction)
}

pub(crate) struct ProofTarget {
    context: ConsensusContext,
    parent: ParentPoint,
    verifier: FinalityVerifier,
    transaction: [u8; 32],
}

impl ProofTarget {
    pub(crate) fn new(
        domain: &MessageDomain,
        identity: TransportIdentity,
        parent: ParentPoint,
        transaction: [u8; 32],
    ) -> Result<Self> {
        validate_parent(&parent)?;
        let verifier = FinalityVerifier::new(domain, identity);
        let context = verifier.context(&parent)?;
        Ok(Self {
            context,
            parent,
            verifier,
            transaction,
        })
    }

    pub(crate) fn height(&self) -> u64 {
        self.context.height
    }

    pub(crate) fn verify(&self, bytes: &[u8]) -> Result<Value> {
        Ok(self.verify_point(bytes)?.1)
    }

    pub(crate) fn verify_point(&self, bytes: &[u8]) -> Result<(ParentPoint, Value)> {
        use crate::native_pipeline::service::rpc::hex;
        let proof = ReceiptFinalityProof::decode(bytes)?;
        let verified = proof.verify(
            &self.verifier.set,
            self.context,
            &self.parent,
            self.transaction,
        )?;
        let receipt = verified.receipt();
        let block = verified.block();
        Ok((
            *block,
            json!({
                "independently_verified_finality": true,
                "tx_hash": hex(&receipt.tx_hash),
                "success": receipt.success,
                "nonce_after": receipt.nonce_after,
                "charged_fee": receipt.charged_fee,
                "height": block.height,
                "block_hash": hex(&block.block_hash),
                "state_root": hex(&block.state_root),
                "proof_bytes": bytes.len(),
            }),
        ))
    }
}

pub(crate) fn validate_parent(parent: &ParentPoint) -> Result<()> {
    ensure!(
        parent.state_root != [0; 32]
            && parent.receipt_batch_commitment != [0; 32]
            && (parent.height == 0) == (parent.block_hash == [0; 32])
            && (parent.height == 0) == (parent.decision_hash == [0; 32]),
        "invalid separately verified parent checkpoint"
    );
    Ok(())
}

pub(crate) struct FinalityVerifier {
    domain: (u64, [u8; 32], [u8; 32]),
    set: ValidatorSet,
}

impl FinalityVerifier {
    pub(crate) fn new(domain: &MessageDomain, verifier: impl TransportVerifier + 'static) -> Self {
        Self {
            domain: (domain.chain_id, domain.genesis, domain.protocol),
            set: (*domain.validators)
                .clone()
                .with_verifier(Arc::new(ProofVerifier(verifier))),
        }
    }

    fn context(&self, parent: &ParentPoint) -> Result<ConsensusContext> {
        Ok(ConsensusContext {
            chain_id: self.domain.0,
            genesis_config_commitment: self.domain.1,
            protocol_commitment: self.domain.2,
            epoch: self.set.epoch(),
            validator_set_hash: self.set.hash(),
            height: parent
                .height
                .checked_add(1)
                .context("parent height exhausted")?,
            parent_block_hash: parent.block_hash,
            parent_decision_hash: parent.decision_hash,
        })
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn verify_header(
        &self,
        bytes: &[u8],
        parent: &ParentPoint,
    ) -> Result<finalized_header::VerifiedHeader> {
        finalized_header::verify(bytes, &self.set, self.context(parent)?, parent)
    }
}

pub(crate) struct Download {
    height: u64,
    expected_total: Option<usize>,
    digest: [u8; 32],
    bytes: Vec<u8>,
    header: bool,
}

impl Download {
    pub(crate) fn new(height: u64) -> Self {
        Self {
            height,
            expected_total: None,
            digest: [0; 32],
            bytes: Vec::new(),
            header: false,
        }
    }

    pub(crate) fn header(height: u64) -> Self {
        Self {
            header: true,
            ..Self::new(height)
        }
    }

    pub(crate) fn selector(&self) -> Result<Query> {
        let selector = ArchiveSelector {
            height: self.height,
            offset: u32::try_from(self.bytes.len())?,
            digest: self.digest,
        };
        Ok(if self.header {
            Query::FinalizedHeader(selector)
        } else {
            Query::ReceiptProof(selector)
        })
    }

    pub(crate) fn accept(&mut self, answer: Answer) -> Result<Option<Vec<u8>>> {
        let ArchiveChunk {
            total_bytes,
            digest,
            data,
        } = match answer {
            Answer::ReceiptProof(chunk) if !self.header => chunk,
            Answer::FinalizedHeader(chunk) if self.header => chunk,
            Answer::Error(QueryError::Busy) if self.bytes.is_empty() => return Ok(None),
            _ => anyhow::bail!("requested archive proof unavailable"),
        };
        let total = total_bytes as usize;
        let limit = if self.header {
            finalized_header::MAX_BYTES
        } else {
            MAX_ARCHIVE_PROOF_BYTES
        };
        ensure!(
            total <= limit
                && !data.is_empty()
                && data.len() <= total.saturating_sub(self.bytes.len()),
            "receipt proof byte limit"
        );
        if let Some(expected) = self.expected_total {
            ensure!(
                expected == total && digest == self.digest,
                "receipt proof changed during download"
            );
        } else {
            self.expected_total = Some(total);
            self.digest = digest;
        }
        self.bytes.extend_from_slice(&data);
        if self.bytes.len() == total {
            let observed_digest: [u8; 32] = Sha256::digest(&self.bytes).into();
            ensure!(
                observed_digest == self.digest,
                "receipt proof digest differs from response"
            );
            return Ok(Some(std::mem::take(&mut self.bytes)));
        }
        Ok(None)
    }
}
