#![forbid(unsafe_code)]

//! A local safety fence for upgrade-authorization signatures, not a block
//! voter, activation journal, or chain database. One durable store per validator
//! and chain namespace must be retained; cloned/reset stores are outside this
//! cooperative operator boundary. No secret key is persisted here.

use super::native_nonce_upgrade_authorization::{
    sign_nonce_upgrade_vote_unfenced_v1, NonceUpgradeAuthorizationVoteV1,
    VerifiedNonceUpgradeAuthorizationV1,
};
use super::*;
use ed25519_dalek::SigningKey;
use rocksdb::{Options, WriteBatch, WriteOptions, DB};
use std::io::{Read, Write};

const SCHEMA: &[u8] = b"novovm-native-nonce-upgrade-vote-store/v1";
const KEY_SCHEMA: &[u8] = b"upgrade-vote/schema";
const KEY_BINDING: &[u8] = b"upgrade-vote/binding";
const MAX_RECORD_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    schema: String,
    chain_id: u64,
    genesis_block_hash: [u8; 32],
    namespace_digest: [u8; 32],
    validator_id: [u8; 32],
    public_key: [u8; 32],
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Fence {
    schema: String,
    subject_hash: [u8; 32],
}

fn binding(
    verified: &VerifiedNonceUpgradeAuthorizationV1,
    public_key: [u8; 32],
) -> Result<Binding> {
    let validator = verified
        .authority()
        .validator_set
        .validators
        .iter()
        .find(|validator| validator.public_key == public_key)
        .context("upgrade signer is not a member of the pinned authority")?;
    let subject = verified.subject();
    Ok(Binding {
        schema: String::from_utf8(SCHEMA.to_vec())?,
        chain_id: subject.chain_id,
        genesis_block_hash: subject.genesis_block_hash,
        namespace_digest: subject.namespace_digest,
        validator_id: validator.validator_id,
        public_key,
    })
}

fn read_marker(path: &Path, expected: &[u8]) -> Result<()> {
    let meta = fs::symlink_metadata(path).context("upgrade vote store marker missing")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != expected.len() as u64 {
        bail!("upgrade vote store marker is invalid");
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes != expected {
        bail!("upgrade vote store belongs to another validator or chain namespace");
    }
    Ok(())
}

/// RocksDB owns an exclusive process lock while this handle is open. Separate
/// handles (including in the same process) cannot concurrently publish votes.
pub struct NonceUpgradeVoteStoreV1 {
    db: DB,
    binding: Binding,
    poisoned: bool,
}

impl NonceUpgradeVoteStoreV1 {
    /// Create ONLY an explicitly new isolated directory, never adopt a live DB.
    pub fn create(
        path: &Path,
        verified: &VerifiedNonceUpgradeAuthorizationV1,
        public_key: [u8; 32],
    ) -> Result<Self> {
        let binding = binding(verified, public_key)?;
        let bytes = serde_json::to_vec(&binding)?;
        if path.to_str().is_none() || path.file_name().is_none() {
            bail!("upgrade vote store must name a new Unicode directory");
        }
        fs::create_dir(path).context("create new isolated upgrade vote store")?;
        let mut marker = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join("store.json"))?;
        marker.write_all(&bytes)?;
        marker.sync_all()?;
        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, path.join("votes.rocksdb"))?;
        let mut batch = WriteBatch::default();
        batch.put(KEY_SCHEMA, SCHEMA);
        batch.put(KEY_BINDING, &bytes);
        let mut store = Self {
            db,
            binding,
            poisoned: false,
        };
        store.write_sync(batch)?;
        store.validate_binding()?;
        Ok(store)
    }

    /// Open a previously initialized, matching store. Missing metadata/data is
    /// an error, not permission to initialize another signing history.
    pub fn open_existing(
        path: &Path,
        verified: &VerifiedNonceUpgradeAuthorizationV1,
        public_key: [u8; 32],
    ) -> Result<Self> {
        let binding = binding(verified, public_key)?;
        let meta = fs::symlink_metadata(path).context("upgrade vote store directory missing")?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            bail!("upgrade vote store must be a real directory");
        }
        read_marker(&path.join("store.json"), &serde_json::to_vec(&binding)?)?;
        let db_path = path.join("votes.rocksdb");
        let meta = fs::symlink_metadata(&db_path).context("upgrade signing history missing")?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            bail!("upgrade signing history must be a real directory");
        }
        let db = DB::open(&Options::default(), db_path)
            .context("open exclusive upgrade vote history")?;
        let store = Self {
            db,
            binding,
            poisoned: false,
        };
        store.validate_binding()?;
        Ok(store)
    }

    fn read(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let value = self.db.get(key)?;
        if value
            .as_ref()
            .is_some_and(|value| value.len() > MAX_RECORD_BYTES)
        {
            bail!("upgrade vote record exceeds its byte limit");
        }
        Ok(value)
    }

    fn validate_binding(&self) -> Result<()> {
        if self.read(KEY_SCHEMA)?.as_deref() != Some(SCHEMA)
            || self.read(KEY_BINDING)? != Some(serde_json::to_vec(&self.binding)?)
        {
            bail!("upgrade vote store schema or binding mismatch; refusing reinitialization");
        }
        Ok(())
    }

    fn write_sync(&mut self, batch: WriteBatch) -> Result<()> {
        let mut options = WriteOptions::default();
        options.set_sync(true);
        if let Err(error) = self.db.write_opt(batch, &options) {
            self.poisoned = true;
            return Err(error)
                .context("upgrade vote persistence failed; reopen and validate before any retry");
        }
        Ok(())
    }

    /// Persist the source-boundary fence BEFORE signing, and persist/read back
    /// the vote before returning it. A retry can return only the identical vote.
    pub fn sign(
        &mut self,
        verified: &VerifiedNonceUpgradeAuthorizationV1,
        key: &SigningKey,
    ) -> Result<NonceUpgradeAuthorizationVoteV1> {
        self.sign_with_hook(verified, key, &mut |_| Ok(()))
    }

    fn sign_with_hook(
        &mut self,
        verified: &VerifiedNonceUpgradeAuthorizationV1,
        key: &SigningKey,
        hook: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<NonceUpgradeAuthorizationVoteV1> {
        if self.poisoned {
            bail!("upgrade vote store is poisoned by a persistence error");
        }
        self.validate_binding()?;
        if self.binding != binding(verified, key.verifying_key().to_bytes())? {
            bail!("upgrade signer key or chain namespace differs from the persistent binding");
        }
        // Deliberately EXCLUDES subject, target protocol, authority hash, epoch
        // and source fork hash: changing them cannot evade this boundary fence.
        let height = verified.subject().activation_height;
        let mut fence_key = b"upgrade-vote/fence/".to_vec();
        fence_key.extend_from_slice(&height.to_be_bytes());
        let mut vote_key = b"upgrade-vote/vote/".to_vec();
        vote_key.extend_from_slice(&height.to_be_bytes());
        let existing_fence = self.read(&fence_key)?;
        let existing_vote = self.read(&vote_key)?;
        if let Some(bytes) = existing_fence {
            let fence: Fence = serde_json::from_slice(&bytes)?;
            if fence.schema != "novovm-native-nonce-upgrade-signing-fence/v1"
                || fence.subject_hash != verified.subject().subject_hash
            {
                bail!(
                    "upgrade validator is already fenced to a conflicting subject at this boundary"
                );
            }
        } else {
            if existing_vote.is_some() {
                bail!("upgrade vote has no durable safety fence");
            }
            let fence = Fence {
                schema: "novovm-native-nonce-upgrade-signing-fence/v1".into(),
                subject_hash: verified.subject().subject_hash,
            };
            let bytes = serde_json::to_vec(&fence)?;
            let mut batch = WriteBatch::default();
            batch.put(&fence_key, &bytes);
            self.write_sync(batch)?;
            if self.read(&fence_key)? != Some(bytes) {
                self.poisoned = true;
                bail!("upgrade safety fence readback failed; no signature released");
            }
        }
        hook("fence.persisted")?;
        if let Some(bytes) = existing_vote {
            let vote: NonceUpgradeAuthorizationVoteV1 = serde_json::from_slice(&bytes)?;
            vote.verify(verified)?;
            if vote.validator_id != self.binding.validator_id {
                bail!("persisted upgrade vote belongs to another signer");
            }
            return Ok(vote);
        }
        let vote = sign_nonce_upgrade_vote_unfenced_v1(verified, key)?;
        let bytes = serde_json::to_vec(&vote)?;
        if bytes.len() > MAX_RECORD_BYTES {
            bail!("upgrade vote exceeds record bound");
        }
        let mut batch = WriteBatch::default();
        batch.put(&vote_key, &bytes);
        self.write_sync(batch)?;
        hook("vote.persisted")?;
        if self.read(&vote_key)? != Some(bytes) {
            self.poisoned = true;
            bail!("upgrade vote readback failed; no signature released");
        }
        // Validate the persisted guard as well as the signature before release.
        let fence: Fence = serde_json::from_slice(
            &self
                .read(&fence_key)?
                .context("upgrade safety fence disappeared")?,
        )?;
        if fence.schema != "novovm-native-nonce-upgrade-signing-fence/v1"
            || fence.subject_hash != verified.subject().subject_hash
        {
            self.poisoned = true;
            bail!("upgrade fence changed before vote release");
        }
        vote.verify(verified)?;
        Ok(vote)
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_nonce_upgrade_authorization::native_nonce_upgrade_authorization_fixture_v1;
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nonce-authorization-votes-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn native_nonce_upgrade_authorization_cli_fixture_matches_durable_signers() {
        use super::super::native_nonce_upgrade_authorization::NonceUpgradeAuthorizationCertificateV1;
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../novovmctl/tests/fixtures/native_nonce_upgrade_authorization_v1.json"
        ))
        .unwrap();
        let target = expected["target_protocol_commitment"].as_str().unwrap();
        let verified = fixture.prepare(target);
        let path = root();
        let votes = fixture
            .keys
            .iter()
            .take(3)
            .enumerate()
            .map(|(index, key)| {
                NonceUpgradeVoteStoreV1::create(
                    &path.join(format!("signer-{index}")),
                    &verified,
                    key.verifying_key().to_bytes(),
                )
                .unwrap()
                .sign(&verified, key)
                .unwrap()
            })
            .collect();
        let certificate =
            NonceUpgradeAuthorizationCertificateV1::from_votes(&verified, votes).unwrap();
        let json = serde_json::json!({
            "authority": fixture.authority,
            "certificate": certificate,
            "expected_authority_commitment": verified.subject().authority_commitment.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "target_protocol_commitment": target,
        });
        assert_eq!(
            json, expected,
            "static CLI fixture must match real durable test-key signing"
        );
    }

    #[test]
    fn native_nonce_upgrade_authorization_vote_fence_survives_reopen_and_blocks_conflicts() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(&"ef".repeat(32));
        let conflicting = fixture.prepare(&"12".repeat(32));
        let path = root().join("signer");
        let key = &fixture.keys[0];
        let mut store =
            NonceUpgradeVoteStoreV1::create(&path, &verified, key.verifying_key().to_bytes())
                .unwrap();
        let vote = store.sign(&verified, key).unwrap();
        assert_eq!(store.sign(&verified, key).unwrap(), vote);
        assert!(store.sign(&conflicting, key).is_err());
        assert!(store.sign(&verified, &fixture.keys[1]).is_err());
        assert!(NonceUpgradeVoteStoreV1::open_existing(
            &path,
            &verified,
            key.verifying_key().to_bytes()
        )
        .is_err());
        drop(store);
        let mut reopened = NonceUpgradeVoteStoreV1::open_existing(
            &path,
            &verified,
            key.verifying_key().to_bytes(),
        )
        .unwrap();
        assert!(reopened.sign(&conflicting, key).is_err());
        assert_eq!(reopened.sign(&verified, key).unwrap(), vote);
    }

    #[test]
    fn native_nonce_upgrade_authorization_vote_interruption_retains_fence_before_release() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(&"ef".repeat(32));
        let conflicting = fixture.prepare(&"12".repeat(32));
        let key = &fixture.keys[0];
        for point in ["fence.persisted", "vote.persisted"] {
            let path = root().join("signer");
            let mut store =
                NonceUpgradeVoteStoreV1::create(&path, &verified, key.verifying_key().to_bytes())
                    .unwrap();
            let mut stop = |event: &str| {
                if event == point {
                    bail!("injected signer interruption");
                }
                Ok(())
            };
            assert!(store.sign_with_hook(&verified, key, &mut stop).is_err());
            drop(store);
            let mut reopened = NonceUpgradeVoteStoreV1::open_existing(
                &path,
                &verified,
                key.verifying_key().to_bytes(),
            )
            .unwrap();
            assert!(reopened.sign(&conflicting, key).is_err());
            reopened
                .sign(&verified, key)
                .unwrap()
                .verify(&verified)
                .unwrap();
        }
    }

    #[test]
    fn native_nonce_upgrade_authorization_vote_missing_or_corrupt_history_never_reinitializes() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(&"ef".repeat(32));
        let key = &fixture.keys[0];
        let path = root().join("missing");
        assert!(NonceUpgradeVoteStoreV1::open_existing(
            &path,
            &verified,
            key.verifying_key().to_bytes()
        )
        .is_err());
        assert!(!path.exists());
        let mut store =
            NonceUpgradeVoteStoreV1::create(&path, &verified, key.verifying_key().to_bytes())
                .unwrap();
        store.sign(&verified, key).unwrap();
        assert!(
            NonceUpgradeVoteStoreV1::create(&path, &verified, key.verifying_key().to_bytes())
                .is_err()
        );
        let mut fence_key = b"upgrade-vote/fence/".to_vec();
        fence_key.extend_from_slice(&verified.subject().activation_height.to_be_bytes());
        store.db.delete(&fence_key).unwrap();
        assert!(store.sign(&verified, key).is_err());
        drop(store);
        let mut reopened = NonceUpgradeVoteStoreV1::open_existing(
            &path,
            &verified,
            key.verifying_key().to_bytes(),
        )
        .unwrap();
        assert!(reopened.sign(&verified, key).is_err());
    }

    #[test]
    #[ignore = "only invoked by upgrade authorization signer process parent"]
    fn native_nonce_upgrade_authorization_vote_process_worker() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(&"ef".repeat(32));
        let key = &fixture.keys[0];
        let path = PathBuf::from(std::env::var_os("NOV_AUTH_VOTE_TEST_PATH").unwrap());
        let action = std::env::var("NOV_AUTH_VOTE_TEST_ACTION").unwrap();
        if action == "resume" {
            let mut store = NonceUpgradeVoteStoreV1::open_existing(
                &path,
                &verified,
                key.verifying_key().to_bytes(),
            )
            .unwrap();
            assert!(store.sign(&fixture.prepare(&"12".repeat(32)), key).is_err());
            let vote = store.sign(&verified, key).unwrap();
            fs::write(
                path.with_extension("released-vote.json"),
                serde_json::to_vec(&vote).unwrap(),
            )
            .unwrap();
        } else {
            let mut store =
                NonceUpgradeVoteStoreV1::create(&path, &verified, key.verifying_key().to_bytes())
                    .unwrap();
            store
                .sign_with_hook(&verified, key, &mut |event| {
                    if event == action {
                        std::process::exit(74);
                    }
                    Ok(())
                })
                .unwrap();
            panic!("expected signer process interruption");
        }
    }

    #[test]
    fn native_nonce_upgrade_authorization_vote_real_process_exit_retains_no_double_sign_fence() {
        let fixture = native_nonce_upgrade_authorization_fixture_v1();
        let verified = fixture.prepare(&"ef".repeat(32));
        for point in ["fence.persisted", "vote.persisted"] {
            let path = root().join("signer");
            let run = |action: &str| {
                let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["tx_ingress::native_nonce_upgrade_vote_store::tests::native_nonce_upgrade_authorization_vote_process_worker", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
                    .env("NOV_AUTH_VOTE_TEST_PATH", &path).env("NOV_AUTH_VOTE_TEST_ACTION", action)
                    .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(30);
                while child.try_wait().unwrap().is_none() {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("authorization worker timed out");
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                child.wait_with_output().unwrap()
            };
            let interrupted = run(point);
            assert_eq!(
                interrupted.status.code(),
                Some(74),
                "{}",
                String::from_utf8_lossy(&interrupted.stderr)
            );
            assert!(!path.with_extension("released-vote.json").exists());
            let resumed = run("resume");
            assert!(
                resumed.status.success(),
                "{}",
                String::from_utf8_lossy(&resumed.stderr)
            );
            let vote: NonceUpgradeAuthorizationVoteV1 = serde_json::from_slice(
                &fs::read(path.with_extension("released-vote.json")).unwrap(),
            )
            .unwrap();
            vote.verify(&verified).unwrap();
        }
    }
}
