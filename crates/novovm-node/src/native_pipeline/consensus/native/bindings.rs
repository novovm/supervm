//! Constructor-validated mapping from independently approved validator keys to
//! authenticated transport identities. This is routing authority, not a vote
//! signature or a replacement validator set. The genesis compiler commits the
//! canonical mapping; endpoints and local secret paths are deliberately absent.
use crate::native_pipeline::consensus::wire::{Hash, ValidatorSet};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::VerifyingKey;
use novovm_network::product_overlay::peer_id_from_ed25519_public_key_v1;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct ValidatorTransportBindings {
    validator_set_hash: Hash,
    transport_by_validator: BTreeMap<Hash, Hash>,
    validator_by_peer: BTreeMap<String, Hash>,
    #[cfg(test)]
    historical_identity_fixture: bool,
}

impl ValidatorTransportBindings {
    pub fn new(
        validators: &ValidatorSet,
        bindings: impl IntoIterator<Item = (Hash, Hash)>,
    ) -> Result<Self> {
        let signing_keys = validators
            .members()
            .iter()
            .map(|v| (*v.public_key(), v.id()))
            .collect::<BTreeMap<_, _>>();
        let mut transport_by_validator = BTreeMap::new();
        let mut validator_by_peer = BTreeMap::new();
        let mut transport_keys = BTreeSet::new();
        for (validator, transport) in bindings {
            let id = signing_keys
                .get(&validator)
                .context("transport binding names an unknown validator")?;
            let key = VerifyingKey::from_bytes(&transport)
                .context("invalid validator transport public key")?;
            ensure!(!key.is_weak(), "weak validator transport public key");
            ensure!(
                !signing_keys.contains_key(&transport),
                "validator signing and transport keys must be purpose-separated"
            );
            ensure!(
                transport_by_validator
                    .insert(validator, transport)
                    .is_none()
                    && transport_keys.insert(transport)
                    && validator_by_peer
                        .insert(peer_id_from_ed25519_public_key_v1(&transport), *id)
                        .is_none(),
                "duplicate validator or transport identity binding"
            );
        }
        ensure!(
            transport_by_validator.len() == signing_keys.len(),
            "transport bindings must cover the complete validator set"
        );
        Ok(Self {
            validator_set_hash: validators.hash(),
            transport_by_validator,
            validator_by_peer,
            #[cfg(test)]
            historical_identity_fixture: false,
        })
    }

    pub fn transport_public_key(&self, validator_public_key: &Hash) -> Result<Hash> {
        self.transport_by_validator
            .get(validator_public_key)
            .copied()
            .context("validator has no approved transport identity")
    }

    pub fn validator_for_peer(&self, transport_peer_id: &str) -> Result<Hash> {
        self.validator_by_peer
            .get(transport_peer_id)
            .copied()
            .context("transport identity has no approved validator binding")
    }

    /// Stable independently of input order. Validator weights and chain/epoch
    /// remain committed by the original validator set and genesis envelope.
    pub fn canonical_commitment(&self) -> Hash {
        let mut hash = Sha256::new();
        hash.update(b"NOVOVM/validator-transport-bindings/v1\0");
        hash.update((self.transport_by_validator.len() as u64).to_be_bytes());
        for (validator, transport) in &self.transport_by_validator {
            hash.update(validator);
            hash.update(transport);
        }
        hash.finalize().into()
    }

    pub(super) fn validate_for(&self, validators: &ValidatorSet) -> Result<()> {
        ensure!(
            self.validator_set_hash == validators.hash(),
            "transport bindings belong to another validator set"
        );
        Ok(())
    }

    pub(super) fn require_production_binding(&self) -> Result<()> {
        #[cfg(test)]
        ensure!(
            !self.historical_identity_fixture,
            "historical identity fixture cannot enter original transport owner"
        );
        Ok(())
    }

    /// Old in-process regression fixtures alone retain their original key
    /// layout. This is not deserializable or available to production callers.
    #[cfg(test)]
    pub(super) fn historical_identity_fixture(validators: &ValidatorSet) -> Self {
        Self {
            validator_set_hash: validators.hash(),
            transport_by_validator: validators
                .members()
                .iter()
                .map(|v| (*v.public_key(), *v.public_key()))
                .collect(),
            validator_by_peer: validators
                .members()
                .iter()
                .map(|v| (peer_id_from_ed25519_public_key_v1(v.public_key()), v.id()))
                .collect(),
            historical_identity_fixture: true,
        }
    }
}
