//! Strict, explicit configuration for the single-candidate seal service.
//! Loading is read-only: it never creates a database, generates a key, or signs.

use crate::database_path::native_database_path;
use crate::native_block_seal::{NovNativeSealValidatorSetV1, NovNativeSealValidatorV1};
use crate::native_block_seal_overlay::{
    NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1,
};
use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

pub const NOV_NATIVE_SEAL_SERVICE_SCHEMA_V1: &str = "novovm-native-seal-service/v1";
const MAX_CONFIG_BYTES: usize = 64 * 1024;
const MAX_AUTHORITY_BYTES: usize = 256 * 1024;

/// Intentionally neither Debug nor Serialize: this contains the operator's key.
/// Callers cannot bypass validation by constructing a public configuration.
pub struct NovNativeSealServiceConfigV1 {
    pub(crate) fresh_genesis_config_commitment: Option<[u8; 32]>,
    pub(crate) finalized_parent_workspace_id: Option<[u8; 32]>,
    pub(crate) isolated_workspace_id: Option<[u8; 32]>,
    pub(crate) decision_v3_enabled: bool,
    pub(crate) commit_v2_enabled: bool,
    pub(crate) chain_id: u64,
    pub(crate) height: u64,
    pub(crate) block_hash: [u8; 32],
    pub(crate) justify_qc_hash: Option<[u8; 32]>,
    pub(crate) authority: NovNativeSealEpochAuthorityV1,
    pub(crate) signer: SigningKey,
    pub(crate) local_validator_id: [u8; 32],
    pub(crate) seal_store_path: PathBuf,
    pub(crate) protected_paths: Vec<PathBuf>,
    pub(crate) round_timeout: Duration,
    pub(crate) poll_interval: Duration,
    pub(crate) ingress_per_source_per_second: usize,
    pub(crate) ingress_per_poll: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceFile {
    #[serde(default)]
    finalized_parent_workspace_id: Option<String>,
    #[serde(default)]
    fresh_genesis_config_commitment: Option<String>,
    #[serde(default)]
    isolated_workspace_id: Option<String>,
    #[serde(default)]
    decision_v3_enabled: bool,
    #[serde(default)]
    commit_v2_enabled: bool,
    schema: String,
    enabled: bool,
    chain_id: u64,
    height: u64,
    block_hash: String,
    justify_qc_hash: Option<String>,
    authority_path: PathBuf,
    signer_key_path: PathBuf,
    seal_store_path: PathBuf,
    round_timeout_ms: u64,
    poll_interval_ms: u64,
    ingress_per_source_per_second: usize,
    ingress_per_poll: usize,
}

// Authority's existing wire types tolerate unknown JSON members. At this
// operator configuration boundary reject unknown members at EVERY level.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityFile {
    schema: String,
    authority_kind: String,
    chain_id: u64,
    genesis_block_hash: [u8; 32],
    protocol_config_commitment: [u8; 32],
    epoch: u64,
    activation_height: u64,
    validator_set: ValidatorSetFile,
    transport_bindings: Vec<TransportBindingFile>,
    leader_schedule: String,
    authority_commitment: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidatorSetFile {
    schema: String,
    chain_id: u64,
    epoch: u64,
    activation_height: u64,
    validators: Vec<ValidatorFile>,
    total_weight: u64,
    quorum_weight: u64,
    validator_set_hash: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidatorFile {
    validator_id: [u8; 32],
    public_key: [u8; 32],
    weight: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportBindingFile {
    validator_id: [u8; 32],
    transport_peer_id: String,
}

impl From<AuthorityFile> for NovNativeSealEpochAuthorityV1 {
    fn from(value: AuthorityFile) -> Self {
        let set = value.validator_set;
        Self {
            schema: value.schema,
            authority_kind: value.authority_kind,
            chain_id: value.chain_id,
            genesis_block_hash: value.genesis_block_hash,
            protocol_config_commitment: value.protocol_config_commitment,
            epoch: value.epoch,
            activation_height: value.activation_height,
            validator_set: NovNativeSealValidatorSetV1 {
                schema: set.schema,
                chain_id: set.chain_id,
                epoch: set.epoch,
                activation_height: set.activation_height,
                validators: set
                    .validators
                    .into_iter()
                    .map(|validator| NovNativeSealValidatorV1 {
                        validator_id: validator.validator_id,
                        public_key: validator.public_key,
                        weight: validator.weight,
                    })
                    .collect(),
                total_weight: set.total_weight,
                quorum_weight: set.quorum_weight,
                validator_set_hash: set.validator_set_hash,
            },
            transport_bindings: value
                .transport_bindings
                .into_iter()
                .map(|binding| NovNativeSealValidatorTransportBindingV1 {
                    validator_id: binding.validator_id,
                    transport_peer_id: binding.transport_peer_id,
                })
                .collect(),
            leader_schedule: value.leader_schedule,
            authority_commitment: value.authority_commitment,
        }
    }
}

impl NovNativeSealServiceConfigV1 {
    /// Prepare the next local candidate from this service's finalized block.
    /// Reuses the operator's key, paths and authority; does not sign or publish.
    /// The caller owns transaction selection/transport and must retain the exact
    /// slot, timestamp and ordered batch for retry. No wall clock is inferred.
    pub fn prepare_fresh_successor(
        self,
        slot: u64,
        timestamp_unix_ms: u64,
        raw_txs: Vec<Vec<u8>>,
        params: &serde_json::Value,
    ) -> Result<Self> {
        self.prepare_fresh_successor_inner(slot, timestamp_unix_ms, raw_txs, params, None)
    }

    /// A received body is only an input claim. Re-execute/read back locally and
    /// compare the complete signed output subject before registering or voting.
    pub fn prepare_received_successor(
        self,
        body: crate::native_candidate_body::VerifiedCandidateBodyV1,
        params: &serde_json::Value,
    ) -> Result<Self> {
        let proposal = body
            .message
            .proposal()
            .context("received body proposal missing")?;
        let subject = &proposal.subject;
        if body.authority_commitment != self.authority.authority_commitment
            || subject.parent_block_hash != self.block_hash
            || self.height.checked_add(1) != Some(subject.height)
        {
            bail!("received body does not extend configured authority and parent");
        }
        self.prepare_fresh_successor_inner(
            subject.slot,
            subject.timestamp_unix_ms,
            body.raw_txs,
            params,
            Some(subject),
        )
    }

    fn prepare_fresh_successor_inner(
        mut self,
        slot: u64,
        timestamp_unix_ms: u64,
        raw_txs: Vec<Vec<u8>>,
        params: &serde_json::Value,
        expected: Option<&crate::native_block_seal::NovNativeSealSubjectV1>,
    ) -> Result<Self> {
        use crate::tx_ingress::candidate_workspace as workspace;
        self.validate(self.chain_id)?;
        let pin = self
            .fresh_genesis_config_commitment
            .context("successor requires fresh genesis")?;
        let parent_id = self
            .isolated_workspace_id
            .context("successor parent workspace missing")?;
        let paths = crate::tx_ingress::native_persistence_write_paths_v1(params);
        let ledger = paths
            .iter()
            .find(|(label, _)| *label == "native block ledger")
            .context("successor ledger path missing")?
            .1
            .clone();
        let writes = paths
            .into_iter()
            .filter(|(label, _)| *label != "native block ledger")
            .map(|(_, path)| path)
            .collect::<Vec<_>>();
        super::service_paths::validate_service_paths_v1(&self, &ledger, &writes, &[])?;
        let parent =
            workspace::load_finalized_genesis_parent_v1(self.chain_id, parent_id, pin, params)?;
        if parent.block().header.height != self.height
            || parent.block().header.block_hash != self.block_hash
            || parent.finality_proof().authority != self.authority
        {
            bail!("successor service differs from the live finalized parent or authority");
        }
        if let Some(predecessor) = self.finalized_parent_workspace_id {
            let artifact = workspace::load_block_artifact_v1(self.chain_id, predecessor, params)?
                .context("configured finalized predecessor output missing")?;
            if artifact.block().header.block_hash != parent.block().header.parent_block_hash {
                bail!("successor service predecessor differs from finalized ancestry");
            }
        }
        let height = self
            .height
            .checked_add(1)
            .context("successor height overflow")?;
        let plan = parent.successor_plan(
            novovm_protocol::NovBlockExecutionContextV1 {
                chain_id: self.chain_id,
                block_height: height,
                parent_block_hash: self.block_hash,
                slot,
                timestamp_unix_ms,
            },
            raw_txs,
            params,
        )?;
        // Full-batch authentication above precedes staging, GC and execution.
        // Each existing boundary rechecks live parent authority under its locks.
        let candidate = workspace::create_from_finalized_genesis_v1(&plan, parent_id, pin, params)?;
        workspace::execute_v1(self.chain_id, candidate.workspace_id, params)?;
        let artifact =
            workspace::load_block_artifact_v1(self.chain_id, candidate.workspace_id, params)?
                .context("prepared successor output missing")?;
        let actual = parent.successor_seal_subject(&artifact, expected.map_or(0, |s| s.round))?;
        if expected.is_some_and(|expected| expected != &actual) {
            bail!("received successor output differs from local verified execution");
        }
        workspace::register_finalized_successor_v1(
            self.chain_id,
            parent_id,
            candidate.workspace_id,
            pin,
            params,
        )?;
        self.height = height;
        self.block_hash = artifact.block().header.block_hash;
        self.isolated_workspace_id = Some(candidate.workspace_id);
        self.finalized_parent_workspace_id = Some(parent_id);
        self.validate(self.chain_id)?;
        Ok(self)
    }

    pub fn is_fresh_successor(&self) -> bool {
        self.finalized_parent_workspace_id.is_some()
    }

    /// Explicit startup routing only; live authority still requires open_configured.
    pub fn is_fresh_genesis(&self) -> bool {
        self.fresh_genesis_config_commitment.is_some()
    }

    /// All relative paths are relative to this canonical configuration file,
    /// never to the current working directory or an assumed workspace name.
    pub fn load(path: &Path, expected_chain_id: u64) -> Result<Self> {
        let config_path = fs::canonicalize(path).context("resolve native seal service config")?;
        let config_dir = config_path.parent().context("seal config has no parent")?;
        let raw: ServiceFile = serde_json::from_slice(&read_bounded(
            &config_path,
            MAX_CONFIG_BYTES,
            "seal service config",
        )?)
        .context("invalid native seal service config JSON")?;
        if raw.schema != NOV_NATIVE_SEAL_SERVICE_SCHEMA_V1 || !raw.enabled {
            bail!("native seal config requires the v1 schema and explicit enabled=true");
        }
        if raw.commit_v2_enabled && raw.decision_v3_enabled {
            bail!("V2 and V3 decision modes are mutually exclusive");
        }
        let authority_path = fs::canonicalize(resolve_path(config_dir, &raw.authority_path)?)
            .context("resolve native seal authority file")?;
        let signer_path = fs::canonicalize(resolve_path(config_dir, &raw.signer_key_path)?)
            .context("resolve native seal signer key file")?;
        let seal_store_path =
            native_database_path(resolve_store_path(config_dir, &raw.seal_store_path)?)?;
        let strict_authority: AuthorityFile = serde_json::from_slice(&read_bounded(
            &authority_path,
            MAX_AUTHORITY_BYTES,
            "seal authority",
        )?)
        .context("invalid native seal authority JSON")?;
        let authority = NovNativeSealEpochAuthorityV1::from(strict_authority);
        authority.validate()?;
        // Key parse failures deliberately never include input bytes.
        let mut key_bytes = read_bounded(&signer_path, 64, "seal signer key")?;
        let decoded = decode_hex_32(&key_bytes, "seal signer key");
        key_bytes.fill(0);
        let mut seed = decoded?;
        let signer = SigningKey::from_bytes(&seed);
        seed.fill(0);
        let local_validator_id = authority
            .validator_set
            .validators
            .iter()
            .find(|validator| validator.public_key == signer.verifying_key().to_bytes())
            .map(|validator| validator.validator_id)
            .context("native seal signer key is not a pinned validator")?;
        let config = Self {
            finalized_parent_workspace_id: raw
                .finalized_parent_workspace_id
                .map(|id| decode_hex_32(id.as_bytes(), "finalized parent workspace ID"))
                .transpose()?,
            fresh_genesis_config_commitment: raw
                .fresh_genesis_config_commitment
                .map(|pin| decode_hex_32(pin.as_bytes(), "fresh genesis config commitment"))
                .transpose()?,
            isolated_workspace_id: raw
                .isolated_workspace_id
                .map(|id| decode_hex_32(id.as_bytes(), "isolated workspace ID"))
                .transpose()?,
            chain_id: raw.chain_id,
            height: raw.height,
            block_hash: decode_hex_32(raw.block_hash.as_bytes(), "candidate block hash")?,
            justify_qc_hash: raw
                .justify_qc_hash
                .map(|hash| decode_hex_32(hash.as_bytes(), "justify QC hash"))
                .transpose()?,
            authority,
            signer,
            local_validator_id,
            seal_store_path,
            protected_paths: vec![config_path, authority_path, signer_path],
            commit_v2_enabled: raw.commit_v2_enabled,
            decision_v3_enabled: raw.decision_v3_enabled,
            round_timeout: Duration::from_millis(raw.round_timeout_ms),
            poll_interval: Duration::from_millis(raw.poll_interval_ms),
            ingress_per_source_per_second: raw.ingress_per_source_per_second,
            ingress_per_poll: raw.ingress_per_poll,
        };
        config.validate(expected_chain_id)?;
        Ok(config)
    }

    /// Recheck the configuration at the service boundary, before database opens.
    pub(crate) fn validate(&self, expected_chain_id: u64) -> Result<()> {
        let fresh_authority = self.authority.authority_kind
            == crate::native_block_seal_overlay::NOV_NATIVE_SEAL_OVERLAY_FRESH_GENESIS_AUTHORITY_V1;
        if fresh_authority != self.fresh_genesis_config_commitment.is_some()
            || self
                .fresh_genesis_config_commitment
                .is_some_and(|pin| pin == [0; 32])
            || (fresh_authority
                && (self.isolated_workspace_id.is_none()
                    || !self.decision_v3_enabled
                    || !((self.height == 1 && self.finalized_parent_workspace_id.is_none())
                        || (self.height > 1 && self.finalized_parent_workspace_id.is_some()))
                    || self.justify_qc_hash.is_some()))
            || self.finalized_parent_workspace_id.is_some_and(|id| {
                !fresh_authority || id == [0; 32] || Some(id) == self.isolated_workspace_id
            })
        {
            bail!("fresh service requires pinned V3 candidate, explicit finalized parent for successor heights, and no parent QC override");
        }
        if self.isolated_workspace_id.is_some()
            && (!self.decision_v3_enabled || self.isolated_workspace_id == Some([0; 32]))
        {
            bail!("isolated workspace signing requires V3 and a nonzero workspace ID");
        }
        if self.commit_v2_enabled && self.decision_v3_enabled {
            bail!("V2 and V3 decision modes are mutually exclusive");
        }
        self.authority.validate()?;
        if expected_chain_id == 0
            || self.chain_id != expected_chain_id
            || self.authority.chain_id != self.chain_id
            || self.height < self.authority.activation_height
            || self.block_hash == [0; 32]
            || self.justify_qc_hash == Some([0; 32])
            || self.authority.validator_set.validators.len() < 2
        {
            bail!("native seal service chain, candidate or validator mesh is invalid");
        }
        let validator = self
            .authority
            .validator_set
            .validator(self.local_validator_id)
            .context("native seal service signer is not pinned")?;
        if validator.public_key != self.signer.verifying_key().to_bytes() {
            bail!("native seal service signer does not match its validator identity");
        }
        self.authority.transport_peer_id(self.local_validator_id)?;
        if !(Duration::from_millis(1_000)..=Duration::from_millis(300_000))
            .contains(&self.round_timeout)
            || !(Duration::from_millis(100)..=Duration::from_millis(1_000))
                .contains(&self.poll_interval)
            || self.poll_interval > self.round_timeout / 2
            || !(1..=32).contains(&self.ingress_per_source_per_second)
            || !(1..=64).contains(&self.ingress_per_poll)
        {
            bail!("native seal service timing or ingress budget is outside its bounds");
        }
        if !self.seal_store_path.is_absolute()
            || self.protected_paths.len() != 3
            || self.protected_paths.iter().any(|path| !path.is_absolute())
        {
            bail!("native seal service paths must be resolved before validation");
        }
        // The native spelling is for RocksDB only. Comparisons still use the
        // canonical form so Windows verbatim prefixes cannot hide containment.
        let canonical_store = resolve_store_path(Path::new(""), &self.seal_store_path)?;
        if self
            .protected_paths
            .iter()
            .any(|path| path.starts_with(&canonical_store) || canonical_store.starts_with(path))
        {
            bail!("native seal store must be separate from configuration and signer files");
        }
        Ok(())
    }
}

fn read_bounded(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>> {
    // Check before opening, so an accidentally configured FIFO/device is not
    // opened as a blocking input. Recheck the opened handle for ordinary races.
    let before = fs::metadata(path).with_context(|| format!("stat {label} path"))?;
    if !before.is_file() || before.len() > limit as u64 {
        bail!("{label} must be a regular file of at most {limit} bytes");
    }
    let file = File::open(path).with_context(|| format!("open {label} file"))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("stat {label} file"))?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        bail!("{label} must be a regular file of at most {limit} bytes");
    }
    let mut bytes = Vec::with_capacity((metadata.len() as usize).min(limit));
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {label} file"))?;
    if bytes.len() > limit {
        bytes.fill(0);
        bail!("{label} exceeds its file size limit");
    }
    Ok(bytes)
}

fn decode_hex_32(bytes: &[u8], label: &str) -> Result<[u8; 32]> {
    if bytes.len() != 64 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        bail!("{label} must contain exactly 64 hexadecimal bytes, without prefix or whitespace");
    }
    let mut decoded = [0; 32];
    for (output, pair) in decoded.iter_mut().zip(bytes.chunks_exact(2)) {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => unreachable!("hex checked above"),
        };
        *output = (digit(pair[0]) << 4) | digit(pair[1]);
    }
    Ok(decoded)
}

fn resolve_path(config_dir: &Path, value: &Path) -> Result<PathBuf> {
    if value.as_os_str().is_empty() {
        bail!("native seal paths must not be empty");
    }
    if value.is_absolute() {
        return Ok(value.to_path_buf());
    }
    // Reject Windows drive-relative/root-relative paths; their interpretation
    // depends on process/drive state rather than solely on the config directory.
    if value
        .components()
        .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        bail!("native seal path must be fully absolute or config-relative");
    }
    Ok(config_dir.join(value))
}

fn resolve_store_path(config_dir: &Path, value: &Path) -> Result<PathBuf> {
    let target = resolve_path(config_dir, value)?;
    if target
        .try_exists()
        .context("check native seal store path")?
    {
        let canonical = fs::canonicalize(&target).context("resolve native seal store directory")?;
        if !canonical.is_dir() {
            bail!("native seal store path must be a directory");
        }
        return Ok(canonical);
    }
    let parent = target.parent().context("native seal store has no parent")?;
    let canonical_parent =
        fs::canonicalize(parent).context("native seal store parent must already exist")?;
    if !canonical_parent.is_dir() {
        bail!("native seal store parent must be a directory");
    }
    let name = target
        .file_name()
        .context("native seal store has no final directory name")?;
    Ok(canonical_parent.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_block_seal_overlay::{
        NOV_NATIVE_SEAL_EPOCH_AUTHORITY_SCHEMA_V1, NOV_NATIVE_SEAL_OVERLAY_AUTHORITY_KIND_V1,
        NOV_NATIVE_SEAL_OVERLAY_LEADER_SCHEDULE_V1,
    };
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct Fixture {
        root: PathBuf,
        config: Value,
        authority: Value,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "novovm-seal-service-config-{}-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(root.join("configuration")).unwrap();
            fs::create_dir_all(root.join("keys")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let authority = serde_json::to_value(fixture_authority()).unwrap();
            let config = json!({
                "schema": NOV_NATIVE_SEAL_SERVICE_SCHEMA_V1,
                "enabled": true,
                "chain_id": 22922,
                "height": 1,
                "block_hash": "ab".repeat(32),
                "authority_path": "authority.json",
                "signer_key_path": "../keys/validator.hex",
                "seal_store_path": "../data/seal",
                "round_timeout_ms": 2000,
                "poll_interval_ms": 100,
                "ingress_per_source_per_second": 8,
                "ingress_per_poll": 16
            });
            let fixture = Self {
                root,
                config,
                authority,
            };
            fixture.write();
            fs::write(fixture.root.join("keys/validator.hex"), "01".repeat(32)).unwrap();
            fixture
        }

        fn path(&self) -> PathBuf {
            self.root.join("configuration/service.json")
        }

        fn write(&self) {
            fs::write(self.path(), serde_json::to_vec(&self.config).unwrap()).unwrap();
            fs::write(
                self.root.join("configuration/authority.json"),
                serde_json::to_vec(&self.authority).unwrap(),
            )
            .unwrap();
        }

        fn load(&self) -> Result<NovNativeSealServiceConfigV1> {
            NovNativeSealServiceConfigV1::load(&self.path(), 22922)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    // Hash-only fixture: no ledger/AOEM facts are manufactured by the loader.
    // Actual ledger identity/height checks belong to service startup tests.
    fn fixture_authority() -> NovNativeSealEpochAuthorityV1 {
        let set = NovNativeSealValidatorSetV1::new(
            22922,
            1,
            1,
            (1..=4)
                .map(|seed| {
                    NovNativeSealValidatorV1::new(
                        SigningKey::from_bytes(&[seed; 32])
                            .verifying_key()
                            .to_bytes(),
                        1,
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap();
        let bindings = set
            .validators
            .iter()
            .enumerate()
            .map(
                |(index, validator)| NovNativeSealValidatorTransportBindingV1 {
                    validator_id: validator.validator_id,
                    transport_peer_id: format!("{:064x}", index + 1),
                },
            )
            .collect();
        let mut authority = NovNativeSealEpochAuthorityV1 {
            schema: NOV_NATIVE_SEAL_EPOCH_AUTHORITY_SCHEMA_V1.into(),
            authority_kind: NOV_NATIVE_SEAL_OVERLAY_AUTHORITY_KIND_V1.into(),
            chain_id: 22922,
            genesis_block_hash: [2; 32],
            protocol_config_commitment: [3; 32],
            epoch: 1,
            activation_height: 1,
            validator_set: set,
            transport_bindings: bindings,
            leader_schedule: NOV_NATIVE_SEAL_OVERLAY_LEADER_SCHEDULE_V1.into(),
            authority_commitment: [0; 32],
        };
        fn add_text(hasher: &mut Sha256, value: &str) {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        let mut hasher = Sha256::new();
        hasher.update(b"novovm-native-seal-epoch-authority-v1\0");
        add_text(&mut hasher, &authority.schema);
        add_text(&mut hasher, &authority.authority_kind);
        hasher.update(authority.chain_id.to_be_bytes());
        hasher.update(authority.genesis_block_hash);
        hasher.update(authority.protocol_config_commitment);
        hasher.update(authority.epoch.to_be_bytes());
        hasher.update(authority.activation_height.to_be_bytes());
        hasher.update(authority.validator_set.validator_set_hash);
        hasher.update((authority.transport_bindings.len() as u64).to_be_bytes());
        for binding in &authority.transport_bindings {
            hasher.update(binding.validator_id);
            add_text(&mut hasher, &binding.transport_peer_id);
        }
        add_text(&mut hasher, &authority.leader_schedule);
        authority.authority_commitment = hasher.finalize().into();
        authority.validate().unwrap();
        authority
    }

    #[test]
    fn native_seal_service_config_load_is_read_only_and_config_relative() {
        let fixture = Fixture::new();
        let loaded = fixture.load().unwrap();
        assert_eq!(loaded.block_hash, [0xab; 32]);
        assert_eq!(loaded.justify_qc_hash, None);
        assert_eq!(loaded.round_timeout, Duration::from_secs(2));
        assert_eq!(loaded.poll_interval, Duration::from_millis(100));
        assert_eq!(loaded.ingress_per_source_per_second, 8);
        assert_eq!(loaded.ingress_per_poll, 16);
        assert_eq!(
            loaded.seal_store_path,
            native_database_path(
                fs::canonicalize(fixture.root.join("data"))
                    .unwrap()
                    .join("seal")
            )
            .unwrap()
        );
        assert!(!loaded.seal_store_path.exists());
        assert_eq!(loaded.protected_paths.len(), 3);
        assert!(loaded.protected_paths.iter().all(|path| path.is_absolute()));
        assert!(loaded.protected_paths.iter().all(|path| path.is_file()));
        assert_eq!(loaded.signer.to_bytes(), [1; 32]);
        assert!(!loaded.commit_v2_enabled);
        assert!(loaded.validate(22923).is_err());
    }

    #[test]
    fn native_seal_service_config_isolated_workspace_requires_v3_and_exact_id() {
        let mut fixture = Fixture::new();
        assert!(fixture.load().unwrap().isolated_workspace_id.is_none());
        fixture.config["isolated_workspace_id"] = json!("31".repeat(32));
        fixture.write();
        assert!(fixture.load().is_err());
        fixture.config["decision_v3_enabled"] = json!(true);
        fixture.write();
        assert_eq!(
            fixture.load().unwrap().isolated_workspace_id,
            Some([0x31; 32])
        );
        for invalid in [
            json!("00".repeat(32)),
            json!("31".repeat(31)),
            json!(true),
            json!("workspace/path"),
        ] {
            fixture.config["isolated_workspace_id"] = invalid;
            fixture.write();
            assert!(fixture.load().is_err());
        }
        fixture.config["isolated_workspace_id"] = json!("31".repeat(32));
        fixture.config["commit_v2_enabled"] = json!(true);
        fixture.write();
        assert!(fixture.load().is_err());
    }

    #[test]
    fn native_seal_service_config_fresh_genesis_requires_explicit_matching_mode() {
        use crate::tx_ingress::fresh_genesis::{
            FreshGenesisConfigV1, GenesisValidatorV1, GENESIS_SCHEMA_V1,
        };
        let mut fixture = Fixture::new();
        assert!(!fixture.load().unwrap().is_fresh_genesis());
        let old = fixture_authority();
        let genesis = FreshGenesisConfigV1 {
            schema: GENESIS_SCHEMA_V1.into(),
            chain_id: 22922,
            timestamp_unix_ms: 1,
            protocol_config_commitment: [3; 32],
            allocations: vec![],
            total_initial_nov: "0".into(),
            validators: old
                .validator_set
                .validators
                .iter()
                .map(|v| GenesisValidatorV1 {
                    public_key: v.public_key,
                    weight: v.weight,
                })
                .collect(),
        };
        let pin = genesis.compile().unwrap().config_commitment();
        fixture.authority = serde_json::to_value(
            NovNativeSealEpochAuthorityV1::derive_operator_pinned_fresh_genesis_epoch(
                &genesis,
                pin,
                old.transport_bindings,
            )
            .unwrap(),
        )
        .unwrap();
        fixture.config["height"] = json!(1);
        fixture.config["decision_v3_enabled"] = json!(true);
        fixture.config["isolated_workspace_id"] = json!("31".repeat(32));
        fixture.write();
        assert!(fixture.load().is_err());
        fixture.config["fresh_genesis_config_commitment"] =
            json!(pin.iter().map(|b| format!("{b:02x}")).collect::<String>());
        fixture.write();
        assert_eq!(
            fixture.load().unwrap().fresh_genesis_config_commitment,
            Some(pin)
        );
        assert!(fixture.load().unwrap().is_fresh_genesis());
        assert!(!fixture.load().unwrap().seal_store_path.exists());
        let good = fixture.config.clone();
        fixture.config["height"] = json!(2);
        fixture.config["finalized_parent_workspace_id"] = json!("32".repeat(32));
        fixture.write();
        assert!(fixture.load().unwrap().is_fresh_successor());
        let successor = fixture.config.clone();
        fixture.config["height"] = json!(3);
        fixture.write();
        assert!(fixture.load().unwrap().is_fresh_successor());
        for (field, value) in [
            ("finalized_parent_workspace_id", Value::Null),
            ("finalized_parent_workspace_id", json!("00".repeat(32))),
            ("finalized_parent_workspace_id", json!("31".repeat(32))),
            ("height", json!(1)),
            ("justify_qc_hash", json!("22".repeat(32))),
        ] {
            fixture.config = successor.clone();
            fixture.config[field] = value;
            fixture.write();
            assert!(fixture.load().is_err(), "successor {field}");
        }
        for (field, value) in [
            ("fresh_genesis_config_commitment", json!("00".repeat(32))),
            ("fresh_genesis_config_commitment", json!("bad")),
            ("isolated_workspace_id", Value::Null),
            ("decision_v3_enabled", json!(false)),
            ("height", json!(2)),
            ("justify_qc_hash", json!("22".repeat(32))),
        ] {
            fixture.config = good.clone();
            fixture.config[field] = value;
            fixture.write();
            assert!(fixture.load().is_err(), "{field}");
        }
        fixture.config = good;
        fixture.authority = serde_json::to_value(fixture_authority()).unwrap();
        fixture.write();
        assert!(fixture.load().is_err());
    }

    #[test]
    fn native_seal_service_config_requires_explicit_opt_in_and_known_fields() {
        let mut fixture = Fixture::new();
        let original = fixture.config.clone();
        for (field, value) in [
            ("enabled", json!(false)),
            ("commit_v2_enabled", json!("true")),
            ("schema", json!("legacy")),
            ("silent_unsafe_mode", json!(true)),
            ("chain_id", json!(22923)),
            ("height", json!(0)),
            ("block_hash", json!("00".repeat(32))),
            ("justify_qc_hash", json!("00".repeat(32))),
        ] {
            fixture.config = original.clone();
            fixture.config[field] = value;
            fixture.write();
            assert!(fixture.load().is_err(), "accepted {field}");
        }
        fixture.config = original;
        fixture.config.as_object_mut().unwrap().remove("enabled");
        fixture.write();
        assert!(fixture.load().is_err());
        let mut duplicated = serde_json::to_string(&fixture.config).unwrap();
        duplicated.pop();
        duplicated.push_str(",\"enabled\":true,\"enabled\":true}");
        fs::write(fixture.path(), duplicated).unwrap();
        assert!(fixture.load().is_err());
    }

    #[test]
    fn native_seal_service_config_rejects_unknown_authority_members_at_all_levels() {
        let mut fixture = Fixture::new();
        let original = fixture.authority.clone();
        for pointer in [
            "",
            "/validator_set",
            "/validator_set/validators/0",
            "/transport_bindings/0",
        ] {
            fixture.authority = original.clone();
            fixture.authority.pointer_mut(pointer).unwrap()["ignored"] = json!(true);
            fixture.write();
            assert!(
                fixture.load().is_err(),
                "accepted unknown field at {pointer}"
            );
        }
        fixture.authority = original;
        fixture.authority["authority_commitment"] = serde_json::to_value([0u8; 32]).unwrap();
        fixture.write();
        assert!(fixture.load().is_err());
    }

    #[test]
    fn native_seal_service_config_timing_and_ingress_budgets_are_bounded() {
        let mut fixture = Fixture::new();
        let original = fixture.config.clone();
        for (field, value) in [
            ("round_timeout_ms", 999),
            ("round_timeout_ms", 300_001),
            ("poll_interval_ms", 99),
            ("poll_interval_ms", 1_001),
            ("ingress_per_source_per_second", 0),
            ("ingress_per_source_per_second", 33),
            ("ingress_per_poll", 0),
            ("ingress_per_poll", 65),
        ] {
            fixture.config = original.clone();
            fixture.config[field] = json!(value);
            fixture.write();
            assert!(fixture.load().is_err(), "accepted {field}={value}");
        }
        fixture.config = original;
        fixture.config["round_timeout_ms"] = json!(1000);
        fixture.config["poll_interval_ms"] = json!(501);
        fixture.write();
        assert!(fixture.load().is_err());
        fixture.config["poll_interval_ms"] = json!(500);
        fixture.write();
        fixture.load().unwrap();
    }

    #[test]
    fn native_seal_service_config_keys_are_exact_hex_and_must_be_pinned() {
        let fixture = Fixture::new();
        let path = fixture.root.join("keys/validator.hex");
        for invalid in [
            "".to_owned(),
            "01".repeat(31),
            format!("{}\n", "01".repeat(32)),
            "g1".repeat(32),
            "09".repeat(32),
        ] {
            fs::write(&path, invalid).unwrap();
            assert!(fixture.load().is_err());
        }
        assert!(!decode_hex_32(b"PRIVATE_KEY_SHOULD_NOT_APPEAR", "key")
            .unwrap_err()
            .to_string()
            .contains("PRIVATE_KEY_SHOULD_NOT_APPEAR"));
    }

    #[test]
    fn native_seal_service_config_caps_files_and_refuses_unsafe_store_paths() {
        let mut fixture = Fixture::new();
        fs::write(fixture.path(), vec![b' '; MAX_CONFIG_BYTES + 1]).unwrap();
        assert!(fixture.load().is_err());
        fixture.write();
        fs::write(
            fixture.root.join("configuration/authority.json"),
            vec![b' '; MAX_AUTHORITY_BYTES + 1],
        )
        .unwrap();
        assert!(fixture.load().is_err());
        fixture.write();
        for value in ["", ".", "..", "authority.json", "../missing/seal"] {
            fixture.config["seal_store_path"] = json!(value);
            fixture.write();
            assert!(fixture.load().is_err(), "accepted store path {value:?}");
        }
        fixture.config["seal_store_path"] = json!("../data/seal");
        fixture.config["signer_key_path"] = json!("../data");
        fixture.write();
        assert!(fixture.load().is_err());
    }

    #[cfg(windows)]
    #[test]
    fn native_seal_service_config_rejects_windows_drive_relative_paths() {
        let fixture = Fixture::new();
        for value in [r"D:keys\validator.hex", r"\keys\validator.hex"] {
            assert!(resolve_path(&fixture.root, Path::new(value)).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn native_seal_service_config_database_paths_preserve_disk_and_unc_volumes() {
        for (canonical, native) in [
            (r"\\?\D:\portable\data\seal", r"D:\portable\data\seal"),
            (
                r"\\?\UNC\server\share\data\seal",
                r"\\server\share\data\seal",
            ),
        ] {
            assert_eq!(
                native_database_path(PathBuf::from(canonical)).unwrap(),
                PathBuf::from(native)
            );
        }
        assert!(native_database_path(PathBuf::from(r"\\?\Volume{unknown}\seal")).is_err());
        let mut fixture = Fixture::new();
        fixture.config["seal_store_path"] = json!(".");
        fixture.write();
        assert!(
            fixture.load().is_err(),
            "native-prefix conversion must not bypass config containment"
        );
    }
}
