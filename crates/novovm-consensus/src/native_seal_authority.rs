//! Native proof-seal validator identity and authority rules.
//!
//! Moved from `novovm-node::native_block_seal` without changing the V1
//! serialization, hash domains, validation, or quorum threshold. These rules
//! do not perform signing, storage, candidate admission, or finality promotion.

use anyhow::{bail, Context, Result};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NOV_NATIVE_BLOCK_SEAL_VALIDATOR_SET_SCHEMA_V1: &str =
    "novovm-native-block-seal-validator-set/v1";
pub const NOV_NATIVE_BLOCK_SEAL_MAX_VALIDATORS_V1: usize = 1_024;

const VALIDATOR_ID_DOMAIN_V1: &[u8] = b"novovm-native-seal-validator-id-v1\0";
const VALIDATOR_SET_HASH_DOMAIN_V1: &[u8] = b"novovm-native-seal-validator-set-v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealValidatorV1 {
    pub validator_id: [u8; 32],
    pub public_key: [u8; 32],
    pub weight: u64,
}

impl NovNativeSealValidatorV1 {
    pub fn new(public_key: [u8; 32], weight: u64) -> Result<Self> {
        if weight == 0 {
            bail!("NOV native seal validator weight must be non-zero");
        }
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .context("NOV native seal validator public key is invalid")?;
        if verifying_key.is_weak() {
            bail!("NOV native seal validator public key is weak");
        }
        Ok(Self {
            validator_id: validator_id_v1(&public_key),
            public_key,
            weight,
        })
    }

    fn validate(&self) -> Result<()> {
        if self.weight == 0 || self.validator_id != validator_id_v1(&self.public_key) {
            bail!("NOV native seal validator identity or weight is invalid");
        }
        let verifying_key = VerifyingKey::from_bytes(&self.public_key)
            .context("NOV native seal validator public key is invalid")?;
        if verifying_key.is_weak() {
            bail!("NOV native seal validator public key is weak");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovNativeSealValidatorSetV1 {
    pub schema: String,
    pub chain_id: u64,
    pub epoch: u64,
    pub activation_height: u64,
    pub validators: Vec<NovNativeSealValidatorV1>,
    pub total_weight: u64,
    pub quorum_weight: u64,
    pub validator_set_hash: [u8; 32],
}

impl NovNativeSealValidatorSetV1 {
    pub fn new(
        chain_id: u64,
        epoch: u64,
        activation_height: u64,
        mut validators: Vec<NovNativeSealValidatorV1>,
    ) -> Result<Self> {
        if chain_id == 0 || epoch == 0 || activation_height == 0 {
            bail!("NOV native seal validator set chain, epoch, and activation height must be non-zero");
        }
        if validators.is_empty() || validators.len() > NOV_NATIVE_BLOCK_SEAL_MAX_VALIDATORS_V1 {
            bail!("NOV native seal validator set size is invalid");
        }
        for validator in &validators {
            validator.validate()?;
        }
        validators.sort_by_key(|validator| validator.validator_id);
        if validators
            .windows(2)
            .any(|pair| pair[0].validator_id == pair[1].validator_id)
        {
            bail!("NOV native seal validator set contains a duplicate validator");
        }
        let total_weight = validators.iter().try_fold(0u64, |total, validator| {
            total
                .checked_add(validator.weight)
                .context("NOV native seal validator weight overflow")
        })?;
        let quorum_weight = (((total_weight as u128) * 2) / 3 + 1) as u64;
        let validator_set_hash = validator_set_hash_v1(
            chain_id,
            epoch,
            activation_height,
            validators.as_slice(),
            total_weight,
            quorum_weight,
        );
        let set = Self {
            schema: NOV_NATIVE_BLOCK_SEAL_VALIDATOR_SET_SCHEMA_V1.to_string(),
            chain_id,
            epoch,
            activation_height,
            validators,
            total_weight,
            quorum_weight,
            validator_set_hash,
        };
        set.validate()?;
        Ok(set)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != NOV_NATIVE_BLOCK_SEAL_VALIDATOR_SET_SCHEMA_V1
            || self.chain_id == 0
            || self.epoch == 0
            || self.activation_height == 0
            || self.validators.is_empty()
            || self.validators.len() > NOV_NATIVE_BLOCK_SEAL_MAX_VALIDATORS_V1
        {
            bail!("NOV native seal validator set metadata is invalid");
        }
        let mut total_weight = 0u64;
        let mut previous = None;
        for validator in &self.validators {
            validator.validate()?;
            if previous.is_some_and(|id| id >= validator.validator_id) {
                bail!("NOV native seal validators are not strictly sorted and unique");
            }
            previous = Some(validator.validator_id);
            total_weight = total_weight
                .checked_add(validator.weight)
                .context("NOV native seal validator weight overflow")?;
        }
        let quorum_weight = (((total_weight as u128) * 2) / 3 + 1) as u64;
        let expected_hash = validator_set_hash_v1(
            self.chain_id,
            self.epoch,
            self.activation_height,
            self.validators.as_slice(),
            total_weight,
            quorum_weight,
        );
        if self.total_weight != total_weight
            || self.quorum_weight != quorum_weight
            || self.validator_set_hash != expected_hash
        {
            bail!("NOV native seal validator set commitment is invalid");
        }
        Ok(())
    }

    pub fn validator(&self, validator_id: [u8; 32]) -> Option<&NovNativeSealValidatorV1> {
        self.validators
            .binary_search_by_key(&validator_id, |validator| validator.validator_id)
            .ok()
            .map(|index| &self.validators[index])
    }
}

pub fn validator_id_v1(public_key: &[u8; 32]) -> [u8; 32] {
    hash_parts_v1(VALIDATOR_ID_DOMAIN_V1, &[public_key.as_slice()])
}

fn validator_set_hash_v1(
    chain_id: u64,
    epoch: u64,
    activation_height: u64,
    validators: &[NovNativeSealValidatorV1],
    total_weight: u64,
    quorum_weight: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(VALIDATOR_SET_HASH_DOMAIN_V1);
    hasher.update(chain_id.to_be_bytes());
    hasher.update(epoch.to_be_bytes());
    hasher.update(activation_height.to_be_bytes());
    hasher.update((validators.len() as u64).to_be_bytes());
    for validator in validators {
        hasher.update(validator.validator_id);
        hasher.update(validator.public_key);
        hasher.update(validator.weight.to_be_bytes());
    }
    hasher.update(total_weight.to_be_bytes());
    hasher.update(quorum_weight.to_be_bytes());
    hasher.finalize().into()
}

fn hash_parts_v1(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        update_len_prefixed_v1(&mut hasher, part);
    }
    hasher.finalize().into()
}

fn update_len_prefixed_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
#[path = "native_seal_authority_tests.rs"]
mod tests;
