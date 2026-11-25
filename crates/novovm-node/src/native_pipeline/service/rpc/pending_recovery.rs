//! Explicit local `existing` recovery. No client submission or fresh fanout.
use super::*;
use novovm_consensus::round_bft::wire::{
    decode_quorum, encode_quorum, evidence_wire_bounds, Quorum,
};
use novovm_protocol::uca_spend::MAX_SIGNED_SPEND_BYTES;
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};
use sha2::{Digest, Sha256};
use std::io::Read;

const MAX_BYTES: usize = 128 * 1024;
const MAX_ROWS: usize = 16;
const WAIT: Duration = Duration::from_secs(30);
const REPLAY_WAIT: Duration = Duration::from_secs(120);

pub struct NilPrecommitRecovery {
    quorum: Quorum,
    digest: Hash,
    replay_deadline: Instant,
}

impl NilPrecommitRecovery {
    pub fn from_env(mode: StartMode, validators: usize) -> Result<Option<Self>> {
        let Some((file, digest)) = recovery_pin(
            "NOVOVM_NATIVE_NIL_PRECOMMIT_FILE",
            "NOVOVM_NATIVE_NIL_PRECOMMIT_SHA256",
        )?
        else {
            return Ok(None);
        };
        ensure!(
            mode == StartMode::Existing,
            "nil precommit recovery requires START=existing"
        );
        let (_, _, bound) = evidence_wire_bounds(validators)?;
        let bytes = read_pinned_recovery(Path::new(&file), digest, bound)?;
        let quorum = decode_quorum(&bytes)?;
        ensure!(
            encode_quorum(&quorum)? == bytes,
            "noncanonical nil precommit certificate"
        );
        ensure!(
            quorum.votes[0].phase == novovm_consensus::round_bft::wire::Phase::Precommit
                && quorum.votes[0].value.is_none(),
            "startup recovery requires a nil Precommit certificate"
        );
        Ok(Some(Self {
            quorum,
            digest,
            replay_deadline: Instant::now() + REPLAY_WAIT,
        }))
    }
}

fn recovery_pin(file_var: &str, pin_var: &str) -> Result<Option<(std::ffi::OsString, Hash)>> {
    match (std::env::var_os(file_var), std::env::var_os(pin_var)) {
        (None, None) => Ok(None),
        (Some(file), Some(pin)) => {
            let pin = pin.to_str().context("invalid recovery SHA256 text")?;
            ensure!(
                pin.len() == 64,
                "expected 64 hex characters for recovery SHA256"
            );
            let mut digest = [0; 32];
            for (dst, pair) in digest.iter_mut().zip(pin.as_bytes().chunks_exact(2)) {
                *dst = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
            }
            Ok(Some((file, digest)))
        }
        _ => anyhow::bail!("startup recovery requires both {file_var} and {pin_var}"),
    }
}

fn read_pinned_recovery(path: &Path, digest: Hash, bound: usize) -> Result<Vec<u8>> {
    ensure!(
        path.is_absolute(),
        "pending recovery requires an absolute file path"
    );
    let metadata = std::fs::symlink_metadata(path).context("inspect pending recovery file")?;
    ensure!(
        metadata.is_file() && metadata.len() <= bound as u64,
        "pending recovery requires one bounded regular file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((bound + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= bound && <[u8; 32]>::from(Sha256::digest(&bytes)) == digest,
        "pending recovery file SHA256 or byte limit mismatch"
    );
    Ok(bytes)
}

/// A single operator-pinned, bounded copy of an original signed native batch.
/// Never opens or changes the database, signs, or grants spending permission.
pub struct PendingAssetRecovery {
    batch: Arc<AssetSpendBatchV3>,
    digest: Hash,
    replay_deadline: Instant,
}

impl PendingAssetRecovery {
    pub fn from_env(mode: StartMode) -> Result<Option<Self>> {
        recovery_pin(
            "NOVOVM_NATIVE_PENDING_RECOVERY_FILE",
            "NOVOVM_NATIVE_PENDING_RECOVERY_SHA256",
        )?
        .map(|(file, digest)| Self::load(mode, Path::new(&file), digest))
        .transpose()
    }

    pub(in crate::native_pipeline::service) fn load(
        mode: StartMode,
        path: &Path,
        digest: Hash,
    ) -> Result<Self> {
        ensure!(
            mode == StartMode::Existing,
            "pending recovery requires START=existing"
        );
        let bytes = read_pinned_recovery(path, digest, MAX_BYTES)?;
        let batch = AssetSpendBatchV3::decode(
            &bytes,
            AssetSpendBatchLimitsV3 {
                transactions: MAX_ROWS,
                transaction_bytes: MAX_SIGNED_SPEND_BYTES,
                body_bytes: MAX_BYTES,
            },
        )?;
        ensure!(
            batch.encode()? == bytes,
            "pending recovery batch is not canonical"
        );
        Ok(Self {
            batch: Arc::new(batch),
            digest,
            replay_deadline: Instant::now() + REPLAY_WAIT,
        })
    }
}

impl RpcLifecycle {
    pub fn poll_nil_precommit_recovery(
        &mut self,
        recovery: &mut Option<NilPrecommitRecovery>,
        now: Instant,
    ) -> Result<Option<Value>> {
        if self.node.controller.is_recovering() {
            ensure!(
                now < recovery
                    .as_ref()
                    .context("nil precommit certificate missing")?
                    .replay_deadline,
                "nil precommit recovery timed out waiting for cold consensus replay"
            );
            self.poll_consensus(now)?;
            if self.node.controller.is_recovering() {
                return Ok(None);
            }
        }
        let recovery = recovery
            .take()
            .context("nil precommit certificate missing")?;
        let weight = self
            .node
            .controller
            .progress
            .recover_nil_precommit(&recovery.quorum)?;
        let report = json!({
            "certificate_sha256": hex(&recovery.digest),
            "height": self.node.controller.context().height,
            "round": self.node.controller.round(),
            "verified_precommit_weight": weight
        });
        self.startup_nil_precommit = Some(report.clone());
        Ok(Some(report))
    }
    /// Advance cold consensus replay before opening the volatile admission pool.
    /// The caller retains the pinned batch while replay needs another tick.
    pub fn poll_pending_asset_recovery(
        &mut self,
        recovery: &mut Option<PendingAssetRecovery>,
        now: Instant,
    ) -> Result<Option<Value>> {
        ensure!(
            self.is_uca(),
            "pending recovery requires the native UCA profile"
        );
        if self.node.controller.is_recovering() {
            ensure!(
                now < recovery
                    .as_ref()
                    .context("pending recovery batch missing")?
                    .replay_deadline,
                "pending recovery timed out waiting for cold consensus replay"
            );
            self.poll_consensus(now)?;
            if self.node.controller.is_recovering() {
                return Ok(None);
            }
        }
        let recovery = recovery.take().context("pending recovery batch missing")?;
        self.restore_pending_assets(recovery).map(Some)
    }

    fn restore_pending_assets(&mut self, recovery: PendingAssetRecovery) -> Result<Value> {
        ensure!(
            self.incoming.is_none()
                && self.pending.is_empty()
                && !self.admission.has_pending_native_admission()
                && self.gossip_order.is_empty(),
            "pending recovery requires a fresh admission owner"
        );
        let mut hashes = BTreeSet::new();
        for index in 0..recovery.batch.len() {
            let row = recovery.batch.row(index)?;
            ensure!(
                hashes.insert(Hash::from(Sha256::digest(row.intent.signing_bytes()?))),
                "duplicate hash in recovery batch"
            );
        }
        let deadline = Instant::now() + WAIT;
        while self.indexed_height < self.node.controller.parent().height {
            ensure!(
                Instant::now() < deadline,
                "pending recovery projection deadline"
            );
            self.poll_projection()?;
            std::thread::sleep(Duration::from_millis(1));
        }
        ensure!(
            !hashes.iter().any(|hash| self.receipts.contains_key(hash)),
            "pending recovery batch contains an already indexed transaction"
        );

        self.incoming_from_client = false;
        self.incoming = Some((
            self.node
                .controller
                .uca_transactions_message(recovery.batch)?,
            0,
        ));
        while self.incoming.is_some() || self.admission.has_pending_native_admission() {
            ensure!(
                Instant::now() < deadline,
                "pending recovery admission deadline"
            );
            self.poll_admission()?;
            std::thread::sleep(Duration::from_millis(1));
        }
        ensure!(
            self.pending.len() == hashes.len()
                && hashes.iter().all(|hash| self.pending.contains_key(hash))
                && self.gossip_order.is_empty(),
            "pending recovery did not admit every original hash locally"
        );
        Ok(
            json!({"batch_sha256":hex(&recovery.digest), "recovered_pending_hashes":
            hashes.iter().map(|hash| hex(hash)).collect::<Vec<_>>(), "finalized":false}),
        )
    }
}
