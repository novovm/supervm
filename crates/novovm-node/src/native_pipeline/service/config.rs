//! Explicit experimental profile; no fixture accounts, signer, chain or AOEM path.
use crate::native_pipeline::business::direct_nov_fee::{DirectNovFeePolicy, FeeState};
use crate::native_pipeline::business::nov_transfer_batch::{
    balance_key, fee_record_changes, program_id, receipt_codec, SEMANTIC_VERSION,
};
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::consensus::wire::{Hash, Validator, ValidatorSet};
use crate::native_pipeline::state::tree::{
    empty_root, stage_state_update, StagedStateUpdate, StateChange, StateNodeReader,
};
use anyhow::{ensure, Context, Result};
use novovm_network::duplex::product_relay_client::{
    ProductRelayClientConfigV1, ProductRelayTlsTrustV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const PROFILE: &str = "native-resident-v1";

/// Local deployment settings do not enter the genesis commitment.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentConfig {
    pub profile: String,
    pub library: PathBuf,
    pub database: PathBuf,
    pub rpc_addr: SocketAddr,
    pub signing_key_file: PathBuf,
    pub relay: ProductRelayClientConfigV1,
    pub genesis: GenesisConfig,
    pub workers: u32,
    pub batch_size: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisConfig {
    pub chain_id: u64,
    pub validator_epoch: u64,
    pub protocol_commitment: Hash,
    pub genesis_config_commitment: Hash,
    pub timestamp_unix_ms: u64,
    pub validators: Vec<GenesisValidator>,
    pub allocations: Vec<GenesisAllocation>,
    pub policy: DirectNovFeePolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisValidator {
    pub public_key: Hash,
    pub weight: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisAllocation {
    pub account_hex: String,
    /// Canonical base-10 NOV base units, avoiding JSON floating-point loss.
    pub amount: String,
}

/// Changes to this selected protocol require an explicit version/pin, never a
/// re-labelled old V3 certificate or an arbitrary supplied commitment.
pub fn protocol_commitment() -> Hash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/native-resident-v1/round-bft-v1/candidate-store-v1\0");
    hash.update(program_id());
    hash.update(SEMANTIC_VERSION.to_be_bytes());
    hash.update(receipt_codec());
    hash.finalize().into()
}

impl ResidentConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let path = path
            .canonicalize()
            .context("resolve resident configuration")?;
        ensure!(
            std::fs::metadata(&path)?.len() <= 1024 * 1024,
            "resident configuration exceeds 1 MiB"
        );
        let mut config: Self = serde_json::from_slice(&std::fs::read(&path)?)
            .context("decode resident configuration")?;
        let base = path.parent().context("configuration has no directory")?;
        for target in [
            &mut config.library,
            &mut config.database,
            &mut config.signing_key_file,
        ] {
            resolve_relative(base, target);
        }
        if let ProductRelayTlsTrustV1::ExplicitCa { certificate_path } = &mut config.relay.tls_trust
        {
            resolve_relative(base, certificate_path);
        }
        config.normalize_native_paths();
        config.validate()?;
        Ok(config)
    }

    pub(super) fn normalize_native_paths(&mut self) {
        for target in [
            &mut self.library,
            &mut self.database,
            &mut self.signing_key_file,
        ] {
            *target = native_path(target);
        }
        if let ProductRelayTlsTrustV1::ExplicitCa { certificate_path } = &mut self.relay.tls_trust {
            *certificate_path = native_path(certificate_path);
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.profile == PROFILE,
            "only explicit native-resident-v1 experimental profile is supported"
        );
        ensure!(
            self.rpc_addr.ip().is_loopback(),
            "resident RPC must bind loopback; public authentication is not configured"
        );
        ensure!(
            (1..=256).contains(&self.workers) && (1..=1024).contains(&self.batch_size),
            "resident worker/batch limits out of range"
        );
        ensure!(
            self.library.is_file() && self.signing_key_file.is_file(),
            "explicit AOEM library or signing-key file missing"
        );
        ensure!(
            self.database.is_absolute()
                && self.library.is_absolute()
                && self.signing_key_file.is_absolute(),
            "load resident paths relative to their configuration file first"
        );
        ensure!(
            self.relay.endpoint.starts_with("wss://")
                && !self.relay.expected_relay_peer_id.is_empty(),
            "resident relay requires explicit WSS endpoint and peer identity"
        );
        ensure!(
            !matches!(
                self.relay.tls_trust,
                ProductRelayTlsTrustV1::NodeKeyBoundEncrypted
            ),
            "insecure test relay trust is not a resident profile option"
        );
        if let ProductRelayTlsTrustV1::ExplicitCa { certificate_path } = &self.relay.tls_trust {
            ensure!(
                certificate_path.is_absolute() && certificate_path.is_file(),
                "explicit relay CA missing or unresolved"
            );
        }
        self.genesis.validate()?;
        Ok(())
    }
}

fn resolve_relative(base: &Path, path: &mut PathBuf) {
    if path.is_relative() {
        *path = base.join(&*path);
    }
}

// The packaged RocksDB provider joins an ordinary '/LOG' suffix, which must
// not be mixed with Windows canonicalization's verbatim prefix. Preserve the
// exact disk/UNC target with ordinary Win32 syntax; never choose another DB.
fn native_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let mut parts = path.components();
        let prefix = match parts.next() {
            Some(Component::Prefix(prefix)) => prefix.kind(),
            _ => return path.to_owned(),
        };
        let mut result = match prefix {
            Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
            Prefix::VerbatimUNC(server, share) => {
                let mut unc = PathBuf::from(r"\\");
                unc.push(server);
                unc.push(share);
                unc
            }
            _ => return path.to_owned(),
        };
        for part in parts {
            if part != Component::RootDir {
                result.push(part.as_os_str());
            }
        }
        result
    }
    #[cfg(not(windows))]
    {
        path.to_owned()
    }
}

struct Empty;
impl StateNodeReader for Empty {
    fn read_node(&self, _: &Hash) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

impl GenesisConfig {
    /// Call when authoring the genesis file, then independently distribute/pin
    /// this value on every node. The claimed genesis field is excluded from its
    /// own derivation; validator weights, policy and every allocation are bound.
    pub fn derive_commitment(&self) -> Result<Hash> {
        let (set, state) = self.prepare()?;
        Ok(self.commitment(&set, &state))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.genesis_config_commitment == self.derive_commitment()?,
            "resident genesis commitment differs from configured allocations/policy/validators"
        );
        Ok(())
    }

    fn commitment(&self, set: &ValidatorSet, state: &StagedStateUpdate) -> Hash {
        let mut hash = Sha256::new();
        hash.update(b"novovm/native-resident-v1/genesis\0");
        hash.update(self.chain_id.to_be_bytes());
        hash.update(self.validator_epoch.to_be_bytes());
        hash.update(self.protocol_commitment);
        hash.update(self.timestamp_unix_ms.to_be_bytes());
        hash.update(set.hash());
        hash.update(state.root());
        hash.finalize().into()
    }

    pub(super) fn prepare(&self) -> Result<(Arc<ValidatorSet>, StagedStateUpdate)> {
        ensure!(self.protocol_commitment == protocol_commitment(), "resident protocol pin mismatch; old V3 and arbitrary protocol domains are not accepted");
        ensure!(
            self.chain_id != 0 && self.validator_epoch != 0 && self.timestamp_unix_ms > 0,
            "invalid resident genesis domain/time"
        );
        ensure!(
            (4..=64).contains(&self.validators.len()),
            "resident profile requires 4..64 explicitly configured validators"
        );
        ensure!(
            !self.allocations.is_empty() && self.allocations.len() <= 8192,
            "resident genesis allocation count out of range"
        );
        self.policy.validate()?;
        let set = Arc::new(ValidatorSet::new(
            self.chain_id,
            self.validator_epoch,
            1,
            self.validators
                .iter()
                .map(|v| Validator::new(v.public_key, v.weight))
                .collect::<Result<Vec<_>>>()?,
        )?);
        let mut changes = fee_record_changes(&self.policy, &FeeState::default())?;
        let mut accounts = BTreeSet::new();
        let mut supply = 0u128;
        for allocation in &self.allocations {
            let account = Account::try_from(decode_hex(&allocation.account_hex)?)
                .map_err(anyhow::Error::msg)?;
            ensure!(
                accounts.insert(account.clone()),
                "duplicate genesis allocation account"
            );
            let amount: u128 = allocation
                .amount
                .parse()
                .context("genesis amount must be base-10 u128")?;
            ensure!(
                amount > 0 && amount.to_string() == allocation.amount,
                "genesis amount must be positive canonical base-10 units"
            );
            supply = supply
                .checked_add(amount)
                .context("genesis allocation supply overflow")?;
            changes.push(StateChange::Put {
                key: balance_key(&account),
                value: amount.to_le_bytes().to_vec(),
            });
        }
        let state = stage_state_update(&Empty, empty_root(), &changes)?;
        Ok((set, state))
    }
}

pub(super) fn decode_hex(text: &str) -> Result<Vec<u8>> {
    let text = text.strip_prefix("0x").unwrap_or(text);
    ensure!(
        !text.is_empty() && text.len() <= 64 && text.len().is_multiple_of(2) && text.is_ascii(),
        "expected bounded hexadecimal account/key"
    );
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |b: u8| {
                (b as char)
                    .to_digit(16)
                    .context("invalid hexadecimal account/key")
            };
            Ok(((digit(pair[0])? << 4) | digit(pair[1])?) as u8)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genesis() -> GenesisConfig {
        let policy = DirectNovFeePolicy {
            quote_ttl_ms: 15_000,
            policy_version: 1,
            policy_source: "default".into(),
            resolution_source: "runtime_state".into(),
            reserve_share_bps: 7000,
            fee_share_bps: 2000,
            risk_buffer_share_bps: 1000,
            min_reserve_bucket_nov: 0,
            min_fee_bucket_nov: 0,
            min_risk_buffer_nov: 1,
            settlement_paused: false,
            redeem_paused: false,
            clearing_enabled: true,
            clearing_daily_nov_hard_limit: 1_000_000,
            clearing_require_healthy_risk_buffer: false,
            clearing_constrained_max_slippage_bps: 500,
            clearing_constrained_daily_usage_bps: 8000,
            clearing_constrained_strategy: "daily_volume_only".into(),
            mapped_asset_auto_heal_rollback_enabled: false,
            mapped_asset_reorg_response_policy: "report_only".into(),
        };
        GenesisConfig {
            chain_id: 71,
            validator_epoch: 1,
            protocol_commitment: protocol_commitment(),
            genesis_config_commitment: [0; 32],
            timestamp_unix_ms: 123,
            policy,
            validators: (1..=4)
                .map(|seed| GenesisValidator {
                    public_key: ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                        .verifying_key()
                        .to_bytes(),
                    weight: 1,
                })
                .collect(),
            allocations: vec![
                GenesisAllocation {
                    account_hex: "01".repeat(20),
                    amount: "1000".into(),
                },
                GenesisAllocation {
                    account_hex: "02".repeat(20),
                    amount: "2000".into(),
                },
            ],
        }
    }

    #[test]
    fn retired_direct_risc0_proof_config_is_never_silently_accepted() {
        let base = serde_json::json!({
            "profile": PROFILE,
            "library": "not-opened-library",
            "database": "not-opened-database",
            "rpc_addr": "127.0.0.1:0",
            "signing_key_file": "not-read-key",
            "relay": {
                "endpoint": "wss://127.0.0.1:9/not-opened",
                "expected_relay_peer_id": "not-connected-test-peer",
                "tls_trust": "native_web_pki"
            },
            "genesis": genesis(),
            "workers": 1,
            "batch_size": 2
        });
        // Decode only: no key, library, network or database is opened here.
        let config: ResidentConfig = serde_json::from_value(base.clone()).unwrap();
        assert_eq!(config.profile, PROFILE);
        assert_eq!(config.batch_size, 2);
        for retired in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({
                "library": "retired-proof-library",
                "guest": "retired-guest.bin",
                "image_id": [1, 2, 3, 4, 5, 6, 7, 8]
            }),
        ] {
            let mut value = base.clone();
            value["proof"] = retired;
            let error = serde_json::from_value::<ResidentConfig>(value).unwrap_err();
            assert!(
                error.to_string().contains("unknown field `proof`"),
                "retired direct-proof configuration must fail, even when null: {error}"
            );
        }
    }

    #[test]
    fn genesis_derivation_binds_economics_validator_weights_and_protocol() {
        let mut base = genesis();
        base.genesis_config_commitment = base.derive_commitment().unwrap();
        base.validate().unwrap();
        let mut reordered = base.clone();
        reordered.validators.reverse();
        reordered.allocations.reverse();
        assert_eq!(
            reordered.derive_commitment().unwrap(),
            base.genesis_config_commitment
        );
        for mutate in [
            |c: &mut GenesisConfig| c.chain_id += 1,
            |c: &mut GenesisConfig| c.validator_epoch += 1,
            |c: &mut GenesisConfig| c.timestamp_unix_ms += 1,
            |c: &mut GenesisConfig| c.validators[0].weight += 1,
            |c: &mut GenesisConfig| c.allocations[0].amount = "1001".into(),
            |c: &mut GenesisConfig| c.policy.quote_ttl_ms += 1,
        ] {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert!(changed.validate().is_err());
        }
        let mut old_protocol = base;
        old_protocol.protocol_commitment = [7; 32];
        assert!(old_protocol.derive_commitment().is_err());
    }

    #[test]
    fn duplicate_allocations_bad_numbers_and_missing_quorum_profile_are_rejected() {
        let mut config = genesis();
        config.allocations.push(config.allocations[0].clone());
        assert!(config.derive_commitment().is_err());
        for amount in [
            "0",
            "01",
            "-1",
            "1.0",
            "340282366920938463463374607431768211456",
        ] {
            let mut config = genesis();
            config.allocations[0].amount = amount.into();
            assert!(config.derive_commitment().is_err());
        }
        let mut config = genesis();
        config.validators.pop();
        assert!(config.derive_commitment().is_err());
        let mut config = genesis();
        config.validators[1] = config.validators[0].clone();
        assert!(config.derive_commitment().is_err());
    }
    #[test]
    fn relative_paths_follow_configuration_directory() {
        let base = Path::new("task-config-root");
        let mut path = PathBuf::from("keys/signer.key");
        resolve_relative(base, &mut path);
        assert_eq!(path, base.join("keys/signer.key"));
    }
    #[cfg(windows)]
    #[test]
    fn windows_native_paths_preserve_disk_and_unc_targets() {
        assert_eq!(
            native_path(Path::new(r"\\?\D:\work\state.rocksdb")),
            PathBuf::from(r"D:\work\state.rocksdb")
        );
        assert_eq!(
            native_path(Path::new(r"\\?\UNC\server\share\state.rocksdb")),
            PathBuf::from(r"\\server\share\state.rocksdb")
        );
        assert_eq!(
            native_path(Path::new("relative/state")),
            PathBuf::from("relative/state")
        );
    }
    #[test]
    fn hex_is_bounded_and_not_lossily_decoded() {
        assert_eq!(decode_hex("0x00aF").unwrap(), vec![0, 175]);
        for text in ["", "x0", "000", "🦀"] {
            assert!(decode_hex(text).is_err());
        }
        assert!(decode_hex(&"ab".repeat(33)).is_err());
    }

    #[test]
    #[ignore = "requires explicit trusted NOVOVM_AOEM_TEST_LIBRARY; opens a real fresh AOEM database"]
    fn real_resident_startup_reopen_preserves_genesis_and_refuses_recreate() -> Result<()> {
        use super::super::{ResidentNode, StartMode};
        let library = PathBuf::from(
            std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
                .context("explicit trusted NOVOVM_AOEM_TEST_LIBRARY required")?,
        )
        .canonicalize()?;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../artifacts/audit/resident-startup")
            .join(format!("{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let key_file = root.join("signer.key");
        std::fs::write(&key_file, [1_u8; 32])?; // Test-only signer, never a profile default.
        let mut genesis = genesis();
        genesis.genesis_config_commitment = genesis.derive_commitment()?;
        let config = ResidentConfig {
            profile: PROFILE.into(),
            library,
            database: root.join("state.rocksdb"),
            rpc_addr: "127.0.0.1:0".parse()?,
            signing_key_file: key_file,
            relay: ProductRelayClientConfigV1 {
                endpoint: "wss://127.0.0.1:9/resident-startup-only".into(),
                expected_relay_peer_id: novovm_network::duplex::peer_id_from_ed25519_public_key_v1(
                    &ed25519_dalek::SigningKey::from_bytes(&[9; 32])
                        .verifying_key()
                        .to_bytes(),
                ),
                connect_timeout_ms: 100,
                read_timeout_ms: 10,
                tls_trust: ProductRelayTlsTrustV1::NativeWebPki,
            },
            genesis,
            workers: 2,
            batch_size: 2,
        };
        let first = ResidentNode::start(config.clone(), StartMode::CreateNew)?;
        let parent = first.controller.parent();
        assert!(first.controller.head().is_none());
        assert_eq!(first.controller.context().height, 1);
        first.shutdown()?;
        assert!(ResidentNode::start(config.clone(), StartMode::CreateNew).is_err());
        let reopened = ResidentNode::start(config.clone(), StartMode::Existing)?;
        assert_eq!(reopened.controller.parent(), parent);
        assert!(reopened.controller.head().is_none());
        reopened.shutdown()?;
        let mut changed = config;
        changed.genesis.allocations[0].amount = "1001".into();
        assert!(ResidentNode::start(changed, StartMode::Existing).is_err());
        // Retain the DB and test key as explicitly test-only evidence. No blocks
        // were voted/finalized, and no real-network success is asserted here.
        Ok(())
    }
}
