//! Explicit local `existing` recovery. No client submission or fresh fanout.
use super::*;
use novovm_protocol::uca_spend::MAX_SIGNED_SPEND_BYTES;
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};
use sha2::{Digest, Sha256};
use std::io::Read;

const MAX_BYTES: usize = 128 * 1024;
const MAX_ROWS: usize = 16;
const WAIT: Duration = Duration::from_secs(30);
const REPLAY_WAIT: Duration = Duration::from_secs(120);

/// A single operator-pinned, bounded copy of an original signed native batch.
/// Never opens or changes the database, signs, or grants spending permission.
pub struct PendingAssetRecovery {
    batch: Arc<AssetSpendBatchV3>,
    digest: Hash,
    replay_deadline: Instant,
}

impl PendingAssetRecovery {
    pub fn from_env(mode: StartMode) -> Result<Option<Self>> {
        let file = std::env::var_os("NOVOVM_NATIVE_PENDING_RECOVERY_FILE");
        let pin = std::env::var_os("NOVOVM_NATIVE_PENDING_RECOVERY_SHA256");
        match (file, pin) {
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
                Ok(Some(Self::load(mode, Path::new(&file), digest)?))
            }
            _ => anyhow::bail!("pending recovery requires both file and SHA256"),
        }
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
        ensure!(
            path.is_absolute(),
            "pending recovery requires an absolute file path"
        );
        let metadata = std::fs::symlink_metadata(path).context("inspect pending recovery file")?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_BYTES as u64,
            "pending recovery requires one bounded regular file"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= MAX_BYTES && <[u8; 32]>::from(Sha256::digest(&bytes)) == digest,
            "pending recovery file SHA256 or byte limit mismatch"
        );
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
