//! Pure fresh-genesis compilation. No snapshot import, keys, IO or activation.
use super::*;
use crate::native_block_ledger::NovNativeFreshGenesisReservationV1;
use crate::native_block_seal::{NovNativeSealValidatorSetV1, NovNativeSealValidatorV1};
use crate::native_root_codecs::NativeRootCodecProfileV1;
use serde::{Deserialize, Serialize};

#[path = "native_fresh_genesis_publication.rs"]
pub mod publication;

pub const GENESIS_SCHEMA_V1: &str = "novovm-fresh-genesis-config/v1";
pub const GENESIS_SCHEMA_RECORD_V2: &str = "novovm-fresh-genesis-config/v2";
const MAX_CONFIG_BYTES: usize = 1024 * 1024;
const MAX_ALLOCATIONS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisAllocationV1 {
    pub account: [u8; 20],
    /// Integer base units, canonical decimal string (no float or exponent).
    pub nov: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisValidatorV1 {
    pub public_key: [u8; 32],
    pub weight: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreshGenesisConfigV1 {
    pub schema: String,
    pub chain_id: u64,
    pub timestamp_unix_ms: u64,
    pub protocol_config_commitment: [u8; 32],
    pub allocations: Vec<GenesisAllocationV1>,
    pub total_initial_nov: String,
    pub validators: Vec<GenesisValidatorV1>,
}

/// Checked deterministic inputs, not an AOEM execution result or finality proof.
pub struct CompiledFreshGenesisV1 {
    config_commitment: [u8; 32],
    state_root: [u8; 32],
    validator_set: NovNativeSealValidatorSetV1,
    store: NovNativeExecutionStoreV1,
    protocol: [u8; 32],
    root_codec_profile: NativeRootCodecProfileV1,
}

/// One immutable configuration and its checked compilation, scoped to a single
/// validation call. This is not a ledger snapshot or permission to sign/publish.
/// Private construction prevents pairing a config with another config's roots.
pub(crate) struct FreshGenesisValidationV1<'config> {
    config: &'config FreshGenesisConfigV1,
    compiled: CompiledFreshGenesisV1,
}

impl<'config> FreshGenesisValidationV1<'config> {
    pub(crate) fn new(config: &'config FreshGenesisConfigV1) -> Result<Self> {
        Ok(Self {
            config,
            compiled: config.compile()?,
        })
    }

    pub(crate) fn config(&self) -> &FreshGenesisConfigV1 {
        self.config
    }

    pub(crate) fn compiled(&self) -> &CompiledFreshGenesisV1 {
        &self.compiled
    }
}

#[cfg(test)]
thread_local! {
    static GENESIS_COMPILATION_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn count_genesis_compilations_for_test_v1<T>(action: impl FnOnce() -> T) -> (T, usize) {
    let before = GENESIS_COMPILATION_COUNT.with(|count| count.get());
    let result = action();
    let after = GENESIS_COMPILATION_COUNT.with(|count| count.get());
    (result, after - before)
}

/// Chain-wide identity derived only from validated genesis inputs. Neither a
/// transaction block hash nor evidence that this genesis is active locally.
/// Private fields prevent callers from constructing an unchecked identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshGenesisIdentityV1 {
    chain_id: u64,
    config_commitment: [u8; 32],
    anchor: [u8; 32],
}

impl FreshGenesisIdentityV1 {
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
    pub fn config_commitment(&self) -> [u8; 32] {
        self.config_commitment
    }
    pub fn anchor(&self) -> [u8; 32] {
        self.anchor
    }
}

fn amount(value: &str) -> Result<u128> {
    if value.is_empty()
        || value.len() > 39
        || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        bail!("genesis NOV amount must be canonical integer base units");
    }
    value.parse().context("genesis NOV amount exceeds u128")
}

impl FreshGenesisConfigV1 {
    /// The approved fresh-genesis schema selects one indivisible root profile.
    /// Unknown schemas never fall back, and this does not migrate existing state.
    pub fn root_codec_profile(&self) -> Result<NativeRootCodecProfileV1> {
        match self.schema.as_str() {
            GENESIS_SCHEMA_V1 => Ok(NativeRootCodecProfileV1::LegacyWireV1),
            GENESIS_SCHEMA_RECORD_V2 => Ok(NativeRootCodecProfileV1::RecordTreeV1),
            _ => bail!("unsupported fresh genesis configuration schema"),
        }
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        Self::from_json_validated_with(bytes, |_| Ok(()))
    }

    pub(crate) fn from_json_validated_with(
        bytes: &[u8],
        action: impl FnOnce(&FreshGenesisValidationV1<'_>) -> Result<()>,
    ) -> Result<Self> {
        if bytes.len() > MAX_CONFIG_BYTES {
            bail!("genesis configuration exceeds byte limit");
        }
        let config: Self = serde_json::from_slice(bytes).context("decode fresh genesis config")?;
        action(&FreshGenesisValidationV1::new(&config)?)?;
        Ok(config)
    }

    /// Produces a preview for explicit operator pinning. Does not read runtime
    /// environment or choose economic parameters. Both profiles fix epoch and
    /// activation to 1, the current fresh nonce scheme, and all history to empty.
    pub fn compile(&self) -> Result<CompiledFreshGenesisV1> {
        #[cfg(test)]
        GENESIS_COMPILATION_COUNT.with(|count| count.set(count.get() + 1));
        let root_codec_profile = self.root_codec_profile()?;
        if self.chain_id == 0
            || self.timestamp_unix_ms == 0
            || self.protocol_config_commitment == [0; 32]
            || self.allocations.len() > MAX_ALLOCATIONS
            || self.validators.len()
                > crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_MAX_VALIDATORS_V1
        {
            bail!("invalid fresh genesis configuration metadata or bounds");
        }
        let validators = self
            .validators
            .iter()
            .map(|v| NovNativeSealValidatorV1::new(v.public_key, v.weight))
            .collect::<Result<Vec<_>>>()?;
        let validator_set = NovNativeSealValidatorSetV1::new(self.chain_id, 1, 1, validators)?;
        let mut store = NovNativeExecutionStoreV1::default();
        store.module_state.protocol_config_commitment = to_hex(&self.protocol_config_commitment);
        let mut total = 0u128;
        for allocation in &self.allocations {
            let quantity = amount(&allocation.nov)?;
            if allocation.account == [0; 20] || quantity == 0 {
                bail!("genesis allocation requires a nonzero account and amount");
            }
            total = total
                .checked_add(quantity)
                .context("genesis allocation sum overflow")?;
            let account = to_hex_prefixed_v1(&allocation.account);
            let balances = BTreeMap::from([("NOV".to_owned(), quantity)]);
            if store
                .module_state
                .account_asset_balances
                .insert(account, balances)
                .is_some()
            {
                bail!("duplicate genesis allocation account");
            }
        }
        if total != amount(&self.total_initial_nov)? {
            bail!("genesis allocation total does not match declared initial NOV");
        }
        let state_root = match root_codec_profile {
            NativeRootCodecProfileV1::LegacyWireV1 => {
                let root = parse_fixed_hex_32_v1(
                    &native_semantic_ledger_state_digest_v1(&store.module_state),
                    "genesis state root",
                )?;
                // Preserve the exact old full-state witness contract for v1.
                export_native_parent_state_wire_v3(&store.module_state, &root)?;
                root
            }
            NativeRootCodecProfileV1::RecordTreeV1 => {
                // A record root is not the legacy whole-state wire root. Do not
                // manufacture an old nonce witness for this distinct commitment.
                native_record_commitment::consensus_state_root_v1(&store.module_state)?
            }
        };
        let config_commitment = match root_codec_profile {
            NativeRootCodecProfileV1::LegacyWireV1 => sha256_bytes_v1(&[
                b"novovm-fresh-genesis-config-v1\0",
                &self.chain_id.to_be_bytes(),
                &self.timestamp_unix_ms.to_be_bytes(),
                &self.protocol_config_commitment,
                &state_root,
                &validator_set.validator_set_hash,
                &total.to_be_bytes(),
            ]),
            NativeRootCodecProfileV1::RecordTreeV1 => {
                let state_codec = root_codec_profile.state_root_codec().as_bytes();
                let receipt_codec = root_codec_profile.receipt_root_codec().as_bytes();
                sha256_bytes_v1(&[
                    b"novovm-fresh-genesis-config-v2\0",
                    &self.chain_id.to_be_bytes(),
                    &self.timestamp_unix_ms.to_be_bytes(),
                    &self.protocol_config_commitment,
                    &(state_codec.len() as u64).to_be_bytes(),
                    state_codec,
                    &(receipt_codec.len() as u64).to_be_bytes(),
                    receipt_codec,
                    &state_root,
                    &validator_set.validator_set_hash,
                    &total.to_be_bytes(),
                ])
            }
        };
        Ok(CompiledFreshGenesisV1 {
            config_commitment,
            state_root,
            validator_set,
            store,
            protocol: self.protocol_config_commitment,
            root_codec_profile,
        })
    }
}

impl CompiledFreshGenesisV1 {
    /// Excludes local namespace, storage paths, first-block contents and round.
    /// The configuration commitment already binds time, initial state, protocol
    /// and the complete validator set. This distinct domain must not be passed
    /// off as the legacy seal profile's height-one transaction block hash.
    pub fn identity(&self) -> FreshGenesisIdentityV1 {
        FreshGenesisIdentityV1 {
            chain_id: self.validator_set.chain_id,
            config_commitment: self.config_commitment,
            anchor: sha256_bytes_v1(&[
                b"novovm-fresh-genesis-chain-identity-v1\0",
                &self.validator_set.chain_id.to_be_bytes(),
                &self.config_commitment,
            ]),
        }
    }

    pub fn config_commitment(&self) -> [u8; 32] {
        self.config_commitment
    }
    pub fn state_root(&self) -> [u8; 32] {
        self.state_root
    }
    pub fn root_codec_profile(&self) -> NativeRootCodecProfileV1 {
        self.root_codec_profile
    }
    pub fn validator_set(&self) -> &NovNativeSealValidatorSetV1 {
        &self.validator_set
    }
    pub fn initial_store(&self) -> &NovNativeExecutionStoreV1 {
        &self.store
    }

    /// Build ledger reservation inputs only after matching an out-of-band pin.
    /// This does NOT reserve storage or certify the local namespace as unused.
    pub fn reservation(
        &self,
        expected_config: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<NovNativeFreshGenesisReservationV1> {
        if expected_config != self.config_commitment || namespace == [0; 32] {
            bail!("fresh genesis approved configuration or local namespace mismatch");
        }
        Ok(NovNativeFreshGenesisReservationV1 {
            chain_id: self.validator_set.chain_id,
            genesis_config_commitment: self.config_commitment,
            initial_state_root: self.state_root,
            validator_set_hash: self.validator_set.validator_set_hash,
            protocol_config_commitment: self.protocol,
            namespace_digest: namespace,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> FreshGenesisConfigV1 {
        FreshGenesisConfigV1 {
            schema: GENESIS_SCHEMA_V1.to_owned(),
            chain_id: 998713,
            timestamp_unix_ms: 1900000000000,
            protocol_config_commitment: [7; 32],
            allocations: vec![
                GenesisAllocationV1 {
                    account: [1; 20],
                    nov: "100".into(),
                },
                GenesisAllocationV1 {
                    account: [2; 20],
                    nov: "200".into(),
                },
            ],
            total_initial_nov: "300".into(),
            validators: (1..=4)
                .map(|seed| GenesisValidatorV1 {
                    public_key: ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                        .verifying_key()
                        .to_bytes(),
                    weight: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn fresh_genesis_validation_reuses_only_one_call_without_changing_checked_values() {
        for schema in [GENESIS_SCHEMA_V1, GENESIS_SCHEMA_RECORD_V2] {
            let mut config = config();
            config.schema = schema.into();
            let expected = config.compile().unwrap();
            let bytes = serde_json::to_vec(&config).unwrap();
            for _ in 0..2 {
                let (result, count) = count_genesis_compilations_for_test_v1(|| {
                    FreshGenesisConfigV1::from_json_validated_with(&bytes, |checked| {
                        assert_eq!(checked.config().chain_id, config.chain_id);
                        for _ in 0..3 {
                            let actual = checked.compiled();
                            assert_eq!(actual.identity(), expected.identity());
                            assert_eq!(actual.state_root(), expected.state_root());
                            assert_eq!(actual.root_codec_profile(), expected.root_codec_profile());
                            assert_eq!(actual.validator_set(), expected.validator_set());
                            assert_eq!(actual.initial_store(), expected.initial_store());
                            assert_eq!(
                                actual.reservation(expected.config_commitment(), [2; 32])?,
                                expected.reservation(expected.config_commitment(), [2; 32])?
                            );
                        }
                        Ok(())
                    })
                });
                assert_eq!(serde_json::to_vec(&result.unwrap()).unwrap(), bytes);
                assert_eq!(count, 1, "each separate validation compiles exactly once");
            }
        }
    }

    #[test]
    fn fresh_genesis_validation_authority_matches_cold_path_and_rechecks_pins() {
        use crate::native_block_seal_overlay::{
            NovNativeSealEpochAuthorityV1 as Authority,
            NovNativeSealValidatorTransportBindingV1 as Binding,
        };
        for schema in [GENESIS_SCHEMA_V1, GENESIS_SCHEMA_RECORD_V2] {
            let mut config = config();
            config.schema = schema.into();
            let compiled = config.compile().unwrap();
            let bindings: Vec<_> = compiled
                .validator_set()
                .validators
                .iter()
                .rev()
                .map(|v| Binding {
                    validator_id: v.validator_id,
                    transport_peer_id: novovm_network::peer_id_from_ed25519_public_key_v1(
                        &v.public_key,
                    ),
                })
                .collect();
            let expected = Authority::derive_operator_pinned_fresh_genesis_epoch(
                &config,
                compiled.config_commitment(),
                bindings.clone(),
            )
            .unwrap();
            let ((), count) = count_genesis_compilations_for_test_v1(|| {
                let validation = FreshGenesisValidationV1::new(&config).unwrap();
                let actual = Authority::derive_operator_pinned_validated_fresh_genesis_epoch(
                    &validation,
                    compiled.config_commitment(),
                    bindings.clone(),
                )
                .unwrap();
                assert_eq!(actual, expected);
                assert!(
                    Authority::derive_operator_pinned_validated_fresh_genesis_epoch(
                        &validation,
                        [99; 32],
                        bindings.clone(),
                    )
                    .is_err()
                );
                let mut bad = bindings.clone();
                bad[0].transport_peer_id = bad[1].transport_peer_id.clone();
                assert!(
                    Authority::derive_operator_pinned_validated_fresh_genesis_epoch(
                        &validation,
                        compiled.config_commitment(),
                        bad,
                    )
                    .is_err()
                );
            });
            assert_eq!(count, 1);
            config.protocol_config_commitment = [8; 32];
            let changed = FreshGenesisValidationV1::new(&config).unwrap();
            assert!(
                Authority::derive_operator_pinned_validated_fresh_genesis_epoch(
                    &changed,
                    compiled.config_commitment(),
                    bindings,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn fresh_genesis_validation_rejects_invalid_input_before_callback_and_propagates_errors() {
        let mut config = config();
        config.total_initial_nov = "301".into();
        let invalid = serde_json::to_vec(&config).unwrap();
        let unknown = br#"{"unknown":true}"#.to_vec();
        let too_large = vec![b' '; MAX_CONFIG_BYTES + 1];
        for bytes in [&invalid, &unknown, &too_large] {
            assert!(FreshGenesisConfigV1::from_json_validated_with(bytes, |_| {
                panic!("invalid genesis reached callback")
            })
            .is_err());
        }
        config.total_initial_nov = "300".into();
        let bytes = serde_json::to_vec(&config).unwrap();
        let (result, count) = count_genesis_compilations_for_test_v1(|| {
            FreshGenesisConfigV1::from_json_validated_with(&bytes, |_| {
                bail!("test ledger evidence rejected")
            })
        });
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("test ledger evidence rejected"));
        assert_eq!(count, 1);
    }

    #[test]
    fn root_codec_genesis_legacy_commitment_and_witness_bytes_are_unchanged() {
        let config = config();
        let compiled = config.compile().unwrap();
        assert_eq!(
            config.root_codec_profile().unwrap(),
            NativeRootCodecProfileV1::LegacyWireV1
        );
        assert_eq!(
            compiled.root_codec_profile(),
            NativeRootCodecProfileV1::LegacyWireV1
        );
        let old_root = parse_fixed_hex_32_v1(
            &native_semantic_ledger_state_digest_v1(&compiled.store.module_state),
            "legacy root",
        )
        .unwrap();
        assert_eq!(compiled.state_root(), old_root);
        let wire =
            export_native_parent_state_wire_v3(&compiled.store.module_state, &old_root).unwrap();
        assert_eq!(
            novovm_protocol::native_parent_nonce::native_state_wire_root_v3(&wire),
            old_root
        );
        let old_commitment = sha256_bytes_v1(&[
            b"novovm-fresh-genesis-config-v1\0",
            &config.chain_id.to_be_bytes(),
            &config.timestamp_unix_ms.to_be_bytes(),
            &config.protocol_config_commitment,
            &old_root,
            &compiled.validator_set.validator_set_hash,
            &300u128.to_be_bytes(),
        ]);
        assert_eq!(compiled.config_commitment(), old_commitment);
    }

    #[test]
    fn root_codec_genesis_record_schema_is_explicit_pinned_and_order_independent() {
        let mut config = config();
        let legacy = config.compile().unwrap();
        config.schema = GENESIS_SCHEMA_RECORD_V2.into();
        let record = config.compile().unwrap();
        assert_eq!(
            config.root_codec_profile().unwrap(),
            NativeRootCodecProfileV1::RecordTreeV1
        );
        assert_eq!(
            record.root_codec_profile(),
            NativeRootCodecProfileV1::RecordTreeV1
        );
        assert_ne!(record.state_root(), legacy.state_root());
        assert_ne!(record.config_commitment(), legacy.config_commitment());
        assert_ne!(record.identity(), legacy.identity());
        assert_eq!(record.initial_store(), legacy.initial_store());
        assert!(record
            .reservation(legacy.config_commitment(), [1; 32])
            .is_err());
        assert!(legacy
            .reservation(record.config_commitment(), [1; 32])
            .is_err());
        assert!(export_native_parent_state_wire_v3(
            &record.store.module_state,
            &record.state_root()
        )
        .is_err());

        let state_codec = record.root_codec_profile().state_root_codec().as_bytes();
        let receipt_codec = record.root_codec_profile().receipt_root_codec().as_bytes();
        assert_eq!(
            record.config_commitment(),
            sha256_bytes_v1(&[
                b"novovm-fresh-genesis-config-v2\0",
                &config.chain_id.to_be_bytes(),
                &config.timestamp_unix_ms.to_be_bytes(),
                &config.protocol_config_commitment,
                &(state_codec.len() as u64).to_be_bytes(),
                state_codec,
                &(receipt_codec.len() as u64).to_be_bytes(),
                receipt_codec,
                &record.state_root(),
                &record.validator_set.validator_set_hash,
                &300u128.to_be_bytes(),
            ])
        );
        config.allocations.reverse();
        config.validators.reverse();
        assert_eq!(
            record.config_commitment(),
            config.compile().unwrap().config_commitment()
        );
        let roundtrip =
            FreshGenesisConfigV1::from_json(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(roundtrip.compile().unwrap().identity(), record.identity());
        for schema in [
            "",
            "novovm-fresh-genesis-config/v3",
            "novovm-fresh-genesis-config/v2 ",
        ] {
            config.schema = schema.into();
            assert!(config.root_codec_profile().is_err());
            assert!(config.compile().is_err());
        }
    }

    #[test]
    fn root_codec_genesis_record_amounts_retain_full_u128_precision() {
        let mut config = config();
        config.schema = GENESIS_SCHEMA_RECORD_V2.into();
        config.allocations.truncate(1);
        config.allocations[0].nov = u128::MAX.to_string();
        config.total_initial_nov = u128::MAX.to_string();
        let compiled = config.compile().unwrap();
        assert_eq!(
            compiled.store.module_state.account_asset_balances[&to_hex_prefixed_v1(&[1; 20])]
                ["NOV"],
            u128::MAX
        );
        assert_eq!(
            compiled.state_root(),
            native_record_commitment::consensus_state_root_v1(&compiled.store.module_state)
                .unwrap()
        );
        config.allocations[0].nov = (u128::MAX - 1).to_string();
        config.total_initial_nov = (u128::MAX - 1).to_string();
        let changed = config.compile().unwrap();
        assert_ne!(compiled.state_root(), changed.state_root());
        assert_ne!(compiled.config_commitment(), changed.config_commitment());
    }

    #[test]
    fn fresh_genesis_is_order_independent_and_has_no_imported_history() {
        let mut config = config();
        let first = config.compile().unwrap();
        config.allocations.reverse();
        config.validators.reverse();
        let other = config.compile().unwrap();
        assert_eq!(first.config_commitment(), other.config_commitment());
        assert_eq!(first.identity(), other.identity());
        assert_eq!(first.identity().chain_id(), config.chain_id);
        assert_eq!(
            first.identity().config_commitment(),
            first.config_commitment()
        );
        assert_ne!(first.identity().anchor(), first.config_commitment());
        assert_ne!(first.identity().anchor(), [0; 32]);
        assert_eq!(first.initial_store(), other.initial_store());
        assert_eq!(first.validator_set().quorum_weight, 3);
        let mut expected = NovNativeExecutionStoreV1::default();
        expected.module_state.protocol_config_commitment = to_hex(&[7; 32]);
        expected.module_state.account_asset_balances =
            first.store.module_state.account_asset_balances.clone();
        assert_eq!(first.store, expected); // all non-allocation state stays fresh
        assert_eq!(
            first
                .reservation(first.config_commitment(), [1; 32])
                .unwrap()
                .initial_state_root,
            first.state_root()
        );
        assert!(first.reservation([9; 32], [1; 32]).is_err());
        assert!(first
            .reservation(first.config_commitment(), [0; 32])
            .is_err());
        assert_ne!(
            first
                .reservation(first.config_commitment(), [1; 32])
                .unwrap(),
            first
                .reservation(first.config_commitment(), [2; 32])
                .unwrap()
        );
    }

    #[test]
    fn fresh_genesis_rejects_ambiguous_amounts_duplicates_and_overflow() {
        for invalid in [
            "01",
            "-1",
            "1.0",
            "1e2",
            " 1",
            "",
            "0",
            "340282366920938463463374607431768211456",
        ] {
            let mut value = config();
            value.allocations[0].nov = invalid.into();
            assert!(value.compile().is_err(), "{invalid}");
        }
        let mut value = config();
        value.allocations.push(value.allocations[0].clone());
        assert!(value.compile().is_err());
        let mut value = config();
        value.allocations[0].nov = u128::MAX.to_string();
        assert!(value.compile().is_err());
        let mut value = config();
        value.validators.push(value.validators[0].clone());
        assert!(value.compile().is_err());
    }

    #[test]
    fn fresh_genesis_commitment_binds_chain_time_protocol_allocations_and_authority() {
        let baseline = config().compile().unwrap().config_commitment();
        let identity = config().compile().unwrap().identity();
        for field in 0..5 {
            let mut value = config();
            match field {
                0 => value.chain_id += 1,
                1 => value.timestamp_unix_ms += 1,
                2 => value.protocol_config_commitment = [8; 32],
                3 => value.allocations[0].account = [3; 20],
                _ => value.validators[0].weight += 1,
            }
            assert_ne!(value.compile().unwrap().config_commitment(), baseline);
            assert_ne!(value.compile().unwrap().identity(), identity);
            assert_ne!(
                value.compile().unwrap().identity().anchor(),
                identity.anchor()
            );
        }
    }

    #[test]
    fn fresh_genesis_json_rejects_test_history_and_unknown_nested_fields() {
        let bytes = serde_json::to_vec(&config()).unwrap();
        assert!(FreshGenesisConfigV1::from_json(&bytes).is_ok());
        for field in [
            "receipts",
            "native_auth_next_nonces",
            "ledger",
            "initial_store",
        ] {
            let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            json[field] = serde_json::json!({});
            assert!(FreshGenesisConfigV1::from_json(&serde_json::to_vec(&json).unwrap()).is_err());
        }
        let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["allocations"][0]["nonce"] = serde_json::json!(1);
        assert!(FreshGenesisConfigV1::from_json(&serde_json::to_vec(&json).unwrap()).is_err());
        assert!(FreshGenesisConfigV1::from_json(&vec![b' '; MAX_CONFIG_BYTES + 1]).is_err());
    }
}
