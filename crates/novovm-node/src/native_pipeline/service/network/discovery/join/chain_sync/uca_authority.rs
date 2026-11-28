//! Produce and independently verify a portable UCA authority receipt.
use super::{
    cached_header_paths, checkpoint_root, checkpoint_validity, minimum_checkpoint, profile_base,
    read_installed_profile, validate_parent, verify_cached_header, wall_ms, FinalityVerifier,
    ParentPoint,
};
use crate::native_pipeline::{
    consensus::statement::finalized_header,
    proof::uca_authority::{
        UcaAuthorityClaimV1, UcaAuthorityStatementV1, JOURNAL_BYTES, MAX_GUEST_INPUT_BYTES,
    },
    service::rpc::hex,
};
use anyhow::{ensure, Context, Result};
use novovm_exec::resident::{ReceiptLimits, ReceiptSession};
use novovm_relay::transport_identity::load_transport_verifier;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    num::NonZeroU64,
    path::{Path, PathBuf},
};

use super::read_snapshot::{read_cache_file, SnapshotNodes};

const PROOF_MAGIC: &[u8; 8] = b"NVUCAP01";
const RECEIPT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_UCA_AUTHORITY_PROOF_BYTES: usize =
    PROOF_MAGIC.len() + 4 + finalized_header::MAX_BYTES + RECEIPT_BYTES;
const REVIEW_BYTES: usize = 8 * 1024;
const RECEIPT_LIMITS: ReceiptLimits = ReceiptLimits {
    elf_bytes: 64 * 1024 * 1024,
    input_bytes: MAX_GUEST_INPUT_BYTES + 4,
    receipt_bytes: RECEIPT_BYTES,
    journal_bytes: JOURNAL_BYTES,
};

/// Values selected by the relying application under its own release/session policy.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UcaAuthorityReviewV1 {
    pub height: NonZeroU64,
    pub max_age_ms: NonZeroU64,
    pub image_id: [u32; 8],
    pub claim: UcaAuthorityClaimV1,
}

fn read_review(path: &Path, sha256: &str) -> Result<UcaAuthorityReviewV1> {
    let bytes = read_cache_file(path, REVIEW_BYTES)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == sha256,
        "UCA review SHA-256 mismatch"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

struct CertifiedAuthority {
    cache: PathBuf,
    local_header: Option<PathBuf>,
    verifier: FinalityVerifier,
    parent: ParentPoint,
    minimum: Option<super::MinimumFinalizedCheckpoint>,
    library: PathBuf,
    protocol: [u8; 32],
    review: UcaAuthorityReviewV1,
}

impl CertifiedAuthority {
    fn open(
        profile_path: &Path,
        profile_sha256: &str,
        review: UcaAuthorityReviewV1,
    ) -> Result<Self> {
        ensure!(
            review.image_id != [0; 8],
            "UCA proof image must be pinned by the relying client"
        );
        let profile = read_installed_profile(profile_path, Some(profile_sha256))?;
        let (domain, anchor) = profile.verification_domain()?;
        validate_parent(&anchor)?;
        ensure!(
            review.claim.domain() == (domain.chain_id, domain.genesis),
            "UCA proof claim differs from the installed chain"
        );
        let height = review.height.get();
        let minimum = minimum_checkpoint(&profile);
        ensure!(
            height > anchor.height && minimum.is_none_or(|pin| height >= pin.height),
            "UCA proof height is below the installed finality policy"
        );
        checkpoint_validity(minimum, wall_ms()?)?;
        let library = profile_base(profile_path)?
            .join(&profile.library)
            .canonicalize()?;
        let verifier = FinalityVerifier::new(
            &domain,
            load_transport_verifier(&library, &profile.library_sha256)?,
        );
        let cache = checkpoint_root(&profile, anchor)?;
        let history = cached_header_paths(&cache)?;
        ensure!(
            history
                .last_key_value()
                .is_none_or(|(&known, _)| known <= height),
            "UCA proof height precedes the locally known finalized head"
        );
        let mut parent = anchor;
        for (&number, path) in history.range(..height) {
            parent = verify_cached_header(&verifier, parent, number, path, minimum)?;
        }
        ensure!(
            parent.height.checked_add(1) == Some(height),
            "UCA proof requires the independently certified preceding header"
        );
        Ok(Self {
            cache,
            local_header: history.get(&height).cloned(),
            verifier,
            parent,
            minimum,
            library,
            protocol: domain.protocol,
            review,
        })
    }

    fn statement(&self, header: &[u8]) -> Result<UcaAuthorityStatementV1> {
        let certified = self.verifier.verify_header(header, &self.parent)?;
        let point = certified.point();
        if let Some(pin) = self.minimum {
            pin.check(point)?;
        }
        if let Some(path) = &self.local_header {
            ensure!(
                self.verifier
                    .verify_header(
                        &read_cache_file(path, finalized_header::MAX_BYTES)?,
                        &self.parent
                    )?
                    .point()
                    == point,
                "UCA proof conflicts with the locally known finalized head"
            );
        }
        let now = wall_ms()?;
        checkpoint_validity(self.minimum, now)?;
        let timestamp = certified.execution_context().timestamp_unix_ms;
        ensure!(
            now.checked_sub(timestamp)
                .is_some_and(|age| age <= self.review.max_age_ms.get()),
            "UCA finalized header is outside the local freshness window"
        );
        Ok(UcaAuthorityStatementV1 {
            protocol: self.protocol,
            height: point.height,
            block_hash: point.block_hash,
            state_root: point.state_root,
            claim: self.review.claim.clone(),
        })
    }
}

fn decode_proof(bytes: &[u8]) -> Result<(&[u8], &[u8])> {
    ensure!(
        (12..=MAX_UCA_AUTHORITY_PROOF_BYTES).contains(&bytes.len())
            && bytes.starts_with(PROOF_MAGIC),
        "invalid portable UCA proof envelope"
    );
    let len = usize::try_from(u32::from_be_bytes(bytes[8..12].try_into()?))?;
    ensure!(
        (1..=finalized_header::MAX_BYTES).contains(&len) && len < bytes.len() - 12,
        "invalid UCA proof header length"
    );
    let (header, receipt) = bytes[12..].split_at(len);
    ensure!(receipt.len() <= RECEIPT_BYTES, "UCA receipt byte limit");
    Ok((header, receipt))
}

fn verified_report(statement: &UcaAuthorityStatementV1, bytes: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "verification": "independent_uca_finalized_state_authority",
        "height": statement.height,
        "block_hash": hex(&statement.block_hash),
        "state_root": hex(&statement.state_root),
        "proof_sha256": format!("{:x}", Sha256::digest(bytes)),
    })
}

/// Works with the existing independently synchronized header and state cache.
pub fn prove_installed_uca_authority(
    profile: &Path,
    profile_sha256: &str,
    review: &Path,
    review_sha256: &str,
    guest: &Path,
    output: &Path,
) -> Result<serde_json::Value> {
    let authority =
        CertifiedAuthority::open(profile, profile_sha256, read_review(review, review_sha256)?)?;
    let header = read_cache_file(
        authority
            .local_header
            .as_ref()
            .context("UCA proof producer has no certified local head")?,
        finalized_header::MAX_BYTES,
    )?;
    let statement = authority.statement(&header)?;
    let input = statement.capture_guest_input(&SnapshotNodes::proof_source(
        authority.cache.join("state-nodes"),
    )?)?;
    let elf = read_cache_file(guest, RECEIPT_LIMITS.elf_bytes)?;
    let mut session = ReceiptSession::open(&authority.library, RECEIPT_LIMITS)?;
    let receipt = session.prove(&elf, &input, &authority.review.image_id)?;
    session.verify(&receipt, &authority.review.image_id, &statement.journal()?)?;
    let mut bytes = Vec::with_capacity(12 + header.len() + receipt.len());
    bytes.extend_from_slice(PROOF_MAGIC);
    bytes.extend_from_slice(&u32::try_from(header.len())?.to_be_bytes());
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(&receipt);
    let directory = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    authority.statement(&header)?;
    ensure!(
        cached_header_paths(&authority.cache)?
            .last_key_value()
            .is_none_or(|(&height, _)| height <= statement.height),
        "UCA proof height became stale during generation"
    );
    file.persist_noclobber(output)?;
    Ok(verified_report(&statement, &bytes))
}

/// Validate bytes received over an existing application carrier, or from a file.
/// No transport, custody key, prover executable, witness or state cache opens here.
pub fn verify_uca_authority_object(
    profile: &Path,
    profile_sha256: &str,
    review: UcaAuthorityReviewV1,
    bytes: &[u8],
) -> Result<UcaAuthorityStatementV1> {
    let (header, receipt) = decode_proof(bytes)?;
    let authority = CertifiedAuthority::open(profile, profile_sha256, review)?;
    let statement = authority.statement(header)?;
    ReceiptSession::open(&authority.library, RECEIPT_LIMITS)?.verify(
        receipt,
        &authority.review.image_id,
        &statement.journal()?,
    )?;
    authority.statement(header)?;
    ensure!(
        cached_header_paths(&authority.cache)?
            .last_key_value()
            .is_none_or(|(&height, _)| height <= statement.height),
        "UCA proof height became stale during verification"
    );
    Ok(statement)
}

pub fn verify_installed_uca_authority(
    profile: &Path,
    profile_sha256: &str,
    review: &Path,
    review_sha256: &str,
    proof: &Path,
) -> Result<serde_json::Value> {
    let review = read_review(review, review_sha256)?;
    let bytes = read_cache_file(proof, MAX_UCA_AUTHORITY_PROOF_BYTES)?;
    let statement = verify_uca_authority_object(profile, profile_sha256, review, &bytes)?;
    Ok(verified_report(&statement, &bytes))
}
