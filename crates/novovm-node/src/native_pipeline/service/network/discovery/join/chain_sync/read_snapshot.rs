//! Serve a certified, immutable client cache without a resident validator.
use super::{
    checkpoint_root_in, checkpoint_validity, minimum_checkpoint, profile_base,
    read_installed_profile, validate_parent, wall_ms, FinalityVerifier, MinimumFinalizedCheckpoint,
    ParentPoint,
};
use crate::native_pipeline::{
    consensus::statement::finalized_header,
    service::rpc::hex,
    state::tree::{
        validate_state_node_bytes, verify_state_node_ancestry, visit_state_leaves, NodeHash,
        StateNodeReader,
    },
};
use anyhow::{ensure, Context, Result};
use novovm_protocol::native_query::{
    Answer, ArchiveChunk, ArchiveSelector, Query, QueryError, StateNodePayload, StateNodeSelector,
    Status, ARCHIVE_CHUNK_BYTES, MAX_STATE_NODE_BYTES,
};
use novovm_relay::transport_identity::load_transport_verifier;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

const MAX_DIGEST_INDEX_BYTES: u64 = 256 * 1024 * 1024;

pub struct ReadSnapshot {
    headers: PathBuf,
    nodes: SnapshotNodes,
    // One digest per height, on an anonymous private temporary file. Only one
    // verified header stays in memory while its chunks are being served.
    proof_index: fs::File,
    first_height: u64,
    cached_header: Option<(u64, [u8; 32], Vec<u8>)>,
    tip: ParentPoint,
    previous_tip: Option<ParentPoint>,
    minimum: Option<MinimumFinalizedCheckpoint>,
    verifier: Arc<FinalityVerifier>,
}

pub(in super::super) struct SnapshotRefresh {
    headers: PathBuf,
    nodes: PathBuf,
    first_height: u64,
    parent: ParentPoint,
    minimum: Option<MinimumFinalizedCheckpoint>,
    verifier: Arc<FinalityVerifier>,
}

pub(in super::super) struct SnapshotAdvance {
    parent: ParentPoint,
    tip: ParentPoint,
    suffix: fs::File,
}

impl ReadSnapshot {
    /// Opens a fixed snapshot through an explicitly selected certified height.
    /// The input is the installed Client data root. No identity file is opened.
    pub fn open(
        data: &Path,
        profile_path: &Path,
        profile_sha256: &str,
        height: u64,
        max_state_nodes: NonZeroUsize,
        verify_deadline: Instant,
    ) -> Result<Self> {
        Self::open_cancellable(
            data,
            profile_path,
            profile_sha256,
            height,
            max_state_nodes,
            verify_deadline,
            None,
        )
    }

    pub(in super::super) fn open_cancellable(
        data: &Path,
        profile_path: &Path,
        profile_sha256: &str,
        height: u64,
        max_state_nodes: NonZeroUsize,
        verify_deadline: Instant,
        cancelled: Option<Arc<AtomicBool>>,
    ) -> Result<Self> {
        let profile = read_installed_profile(profile_path, Some(profile_sha256))?;
        let (domain, anchor) = profile.verification_domain()?;
        validate_parent(&anchor)?;
        ensure!(
            height > anchor.height,
            "snapshot must contain a finalized successor"
        );
        // Relative to the trusted anchor: cap temporary disk, not chain height.
        ensure!(
            height - anchor.height <= MAX_DIGEST_INDEX_BYTES / 32,
            "snapshot digest index exceeds 256 MiB; use a nearer trusted anchor"
        );
        let minimum = minimum_checkpoint(&profile);
        checkpoint_validity(minimum, wall_ms()?)?;
        ensure!(
            minimum.is_none_or(|pin| height >= pin.height),
            "snapshot height precedes the required checkpoint"
        );
        let library = profile_base(profile_path)?.join(&profile.library);
        let verifier = Arc::new(FinalityVerifier::new(
            &domain,
            load_transport_verifier(&library, &profile.library_sha256)?,
        ));
        let headers = checkpoint_root_in(data, &profile, anchor)?;
        let nodes = headers.join("state-nodes");
        ensure!(
            data.is_absolute()
                && headers.strip_prefix(data).is_ok()
                && headers
                    .ancestors()
                    .take_while(|path| *path != data)
                    .all(plain_directory)
                && plain_directory(data)
                && plain_directory(&nodes),
            "snapshot cache must use existing directories without links"
        );
        // Canonical paths also allow ordinary Windows readers to use long cache names.
        let headers = headers.canonicalize()?;
        let nodes = nodes.canonicalize()?;
        ensure!(
            headers.starts_with(data.canonicalize()?) && nodes == headers.join("state-nodes"),
            "snapshot cache resolved outside its data root"
        );
        let mut nodes = SnapshotNodes {
            directory: nodes,
            verify_deadline: Some(verify_deadline),
            cancelled,
        };
        let mut proof_index = BufWriter::new(tempfile::tempfile()?);
        let first_height = anchor.height + 1;
        let mut tip = anchor;
        for next in first_height..=height {
            ensure!(
                nodes
                    .cancelled
                    .as_ref()
                    .is_none_or(|stop| !stop.load(Ordering::Relaxed)),
                "snapshot verification stopped"
            );
            ensure!(
                Instant::now() < verify_deadline,
                "snapshot verification deadline elapsed"
            );
            let bytes = read_cache_file(&header_path(&headers, next), finalized_header::MAX_BYTES)
                .with_context(|| format!("reading cached header at height {next}"))?;
            tip = verifier
                .verify_header(&bytes, &tip)
                .with_context(|| format!("verifying cached header at height {next}"))?
                .point();
            ensure!(tip.height == next, "cached header height mismatch");
            if let Some(pin) = minimum {
                pin.check(tip)?;
            }
            proof_index.write_all(&Sha256::digest(bytes))?;
        }
        visit_state_leaves(
            &nodes,
            tip.state_root,
            max_state_nodes.get(),
            &mut |_, _| Ok(()),
        )
        .context("verifying complete cached snapshot state")?;
        checkpoint_validity(minimum, wall_ms()?)?;
        ensure!(
            Instant::now() < verify_deadline,
            "snapshot verification deadline elapsed"
        );
        nodes.verify_deadline = None;
        nodes.cancelled = None;
        Ok(Self {
            headers,
            nodes,
            proof_index: proof_index.into_inner()?,
            first_height,
            cached_header: None,
            tip,
            previous_tip: None,
            minimum,
            verifier,
        })
    }

    pub(in super::super) fn refresh(&self) -> SnapshotRefresh {
        SnapshotRefresh {
            headers: self.headers.clone(),
            nodes: self.nodes.directory.clone(),
            first_height: self.first_height,
            parent: self.tip,
            minimum: self.minimum,
            verifier: self.verifier.clone(),
        }
    }

    pub(in super::super) fn advance(&mut self, mut prepared: SnapshotAdvance) -> Result<u64> {
        ensure!(
            prepared.parent == self.tip,
            "snapshot refresh parent changed"
        );
        checkpoint_validity(self.minimum, wall_ms()?)?;
        self.proof_index.seek(SeekFrom::Start(
            (self.tip.height - self.first_height + 1) * 32,
        ))?;
        // Until the copy succeeds, any bytes beyond the current tip are invisible.
        io::copy(&mut prepared.suffix, &mut self.proof_index)?;
        self.previous_tip = Some(self.tip);
        self.tip = prepared.tip;
        Ok(self.tip.height)
    }

    pub fn answer(&mut self, query: &Query) -> Answer {
        let unavailable = Answer::Error(QueryError::Unavailable);
        if wall_ms()
            .and_then(|now| checkpoint_validity(self.minimum, now).map_err(Into::into))
            .is_err()
        {
            return unavailable;
        }
        match query {
            Query::Status => Answer::Status(self.status()),
            Query::FinalizedHeader(selector) => self.header(*selector).unwrap_or(unavailable),
            Query::StateNode(selector) => self.state_node(selector).unwrap_or(unavailable),
            _ => unavailable,
        }
    }

    fn status(&self) -> Status {
        let tip = self.tip;
        Status {
            height: tip.height,
            block_hash: tip.block_hash,
            state_root: tip.state_root,
            receipt_batch_commitment: tip.receipt_batch_commitment,
            state_version: tip.state_version,
            indexed_height: tip.height,
            current_height: tip.height,
            round: 0,
            recovering: false,
            projection_failed: false,
            pending: 0,
            apfl_rows: 0,
            integer_rows: 0,
            integer_plans: 0,
            rejected_inputs: 0,
        }
    }

    fn header(&mut self, selector: ArchiveSelector) -> Option<Answer> {
        let start = selector.offset as usize;
        if !(self.first_height..=self.tip.height).contains(&selector.height)
            || start >= finalized_header::MAX_BYTES
            || !start.is_multiple_of(ARCHIVE_CHUNK_BYTES)
            || (start == 0 && selector.digest != [0; 32])
        {
            return None;
        }
        if !self
            .cached_header
            .as_ref()
            .is_some_and(|(height, _, _)| *height == selector.height)
        {
            // Check the small immutable index before reading a proof. A wrong
            // continuation digest cannot trigger a full file read or hashing.
            let offset = (selector.height - self.first_height).checked_mul(32)?;
            self.proof_index.seek(SeekFrom::Start(offset)).ok()?;
            let mut digest = [0; 32];
            self.proof_index.read_exact(&mut digest).ok()?;
            if start != 0 && selector.digest != digest {
                return None;
            }
            self.cached_header = None;
            let bytes = read_cache_file(
                &header_path(&self.headers, selector.height),
                finalized_header::MAX_BYTES,
            )
            .ok()?;
            if start >= bytes.len() || <[u8; 32]>::from(Sha256::digest(&bytes)) != digest {
                return None;
            }
            self.cached_header = Some((selector.height, digest, bytes));
        }
        let (_, digest, bytes) = self.cached_header.as_ref()?;
        if start >= bytes.len() || (start != 0 && selector.digest != *digest) {
            return None;
        }
        let end = (start + ARCHIVE_CHUNK_BYTES).min(bytes.len());
        Some(Answer::FinalizedHeader(ArchiveChunk {
            digest: *digest,
            total_bytes: bytes.len() as u32,
            data: bytes[start..end].to_vec(),
        }))
    }

    fn state_node(&self, selector: &StateNodeSelector) -> Option<Answer> {
        let matches =
            |tip: ParentPoint| (selector.height, selector.root) == (tip.height, tip.state_root);
        if (!matches(self.tip) && !self.previous_tip.is_some_and(matches))
            || verify_state_node_ancestry(selector.root, selector.hash, &selector.ancestry).is_err()
        {
            return None;
        }
        let bytes = self.nodes.read_node(&selector.hash).ok()??;
        validate_state_node_bytes(&selector.hash, &bytes).ok()?;
        Some(Answer::StateNode(StateNodePayload(bytes)))
    }
}

impl SnapshotRefresh {
    pub(in super::super) fn prepare(
        self,
        max_nodes: NonZeroUsize,
        deadline: Instant,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Option<SnapshotAdvance>> {
        let mut tip = self.parent;
        let mut suffix: Option<BufWriter<fs::File>> = None;
        while let Some(next) = tip.height.checked_add(1) {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "snapshot refresh stopped"
            );
            ensure!(
                Instant::now() < deadline,
                "snapshot refresh deadline elapsed"
            );
            let bytes = match read_cache_file(
                &header_path(&self.headers, next),
                finalized_header::MAX_BYTES,
            ) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                Err(error) => return Err(error).context("reading cached snapshot successor"),
            };
            ensure!(
                next - self.first_height < MAX_DIGEST_INDEX_BYTES / 32,
                "snapshot digest index exceeds 256 MiB; use a nearer trusted anchor"
            );
            tip = self.verifier.verify_header(&bytes, &tip)?.point();
            ensure!(tip.height == next, "cached header height mismatch");
            if let Some(minimum) = self.minimum {
                minimum.check(tip)?;
            }
            let writer = match &mut suffix {
                Some(writer) => writer,
                None => suffix.insert(BufWriter::new(tempfile::tempfile()?)),
            };
            writer.write_all(&Sha256::digest(bytes))?;
        }
        let Some(suffix) = suffix else {
            return Ok(None);
        };
        let nodes = SnapshotNodes {
            directory: self.nodes,
            verify_deadline: Some(deadline),
            cancelled: Some(cancelled),
        };
        visit_state_leaves(&nodes, tip.state_root, max_nodes.get(), &mut |_, _| Ok(()))
            .context("verifying complete successor snapshot state")?;
        checkpoint_validity(self.minimum, wall_ms()?)?;
        ensure!(
            Instant::now() < deadline,
            "snapshot refresh deadline elapsed"
        );
        let mut suffix = suffix.into_inner()?;
        suffix.rewind()?;
        Ok(Some(SnapshotAdvance {
            parent: self.parent,
            tip,
            suffix,
        }))
    }
}

struct SnapshotNodes {
    directory: PathBuf,
    verify_deadline: Option<Instant>,
    cancelled: Option<Arc<AtomicBool>>,
}

impl StateNodeReader for SnapshotNodes {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        ensure!(
            self.cancelled
                .as_ref()
                .is_none_or(|stop| !stop.load(Ordering::Relaxed)),
            "snapshot refresh stopped"
        );
        ensure!(
            self.verify_deadline.is_none_or(|end| Instant::now() < end),
            "snapshot verification deadline elapsed"
        );
        let path = self.directory.join(format!("{}.node", hex(hash)));
        match read_cache_file(&path, MAX_STATE_NODE_BYTES) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

fn header_path(directory: &Path, height: u64) -> PathBuf {
    directory.join(format!("{height:020}.proof"))
}

fn plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir() && !is_link(&metadata))
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

fn read_cache_file(path: &Path, max: usize) -> io::Result<Vec<u8>> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid snapshot cache file");
    // Reject links and special files before opening, and check the opened handle again.
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || is_link(&metadata) || metadata.len() > max as u64 {
        return Err(invalid());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_link(&metadata) || metadata.len() > max as u64 {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take((max + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > max {
        return Err(invalid());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
