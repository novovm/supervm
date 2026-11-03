//! Constructor-validated mapping from independently approved validator keys to
//! authenticated transport identities. This is routing authority, not a vote
//! signature or a replacement validator set. The genesis compiler commits the
//! canonical mapping; endpoints and local secret paths are deliberately absent.
use crate::native_pipeline::consensus::wire::{Hash, ValidatorSet};
use anyhow::{ensure, Context, Result};
use novovm_network::product_overlay::{
    peer_id_from_mldsa65_public_key_v1, MLDSA65_PUBLIC_KEY_BYTES,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct ValidatorTransportBindings {
    validator_set_hash: Hash,
    transport_by_validator: BTreeMap<Vec<u8>, Vec<u8>>,
    validator_by_peer: BTreeMap<String, Hash>,
}

impl ValidatorTransportBindings {
    pub fn new(
        validators: &ValidatorSet,
        bindings: impl IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
    ) -> Result<Self> {
        let signing_keys = validators
            .members()
            .iter()
            .map(|v| (v.public_key().to_vec(), v.id()))
            .collect::<BTreeMap<_, _>>();
        let mut transport_by_validator = BTreeMap::new();
        let mut validator_by_peer = BTreeMap::new();
        let mut transport_keys = BTreeSet::new();
        for (validator, transport) in bindings {
            let id = signing_keys
                .get(&validator)
                .context("transport binding names an unknown validator")?;
            ensure!(
                transport.len() == MLDSA65_PUBLIC_KEY_BYTES && transport.iter().any(|b| *b != 0),
                "validator transport public key must be ML-DSA-65"
            );
            ensure!(
                !signing_keys.contains_key(&transport),
                "validator signing and transport keys must be independent"
            );
            let peer = peer_id_from_mldsa65_public_key_v1(&transport);
            ensure!(
                transport_by_validator
                    .insert(validator, transport.clone())
                    .is_none()
                    && transport_keys.insert(transport)
                    && validator_by_peer.insert(peer, *id).is_none(),
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
        })
    }

    pub fn transport_public_key(&self, validator_public_key: &[u8]) -> Result<Vec<u8>> {
        self.transport_by_validator
            .get(validator_public_key)
            .cloned()
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
        hash.update(b"NOVOVM/validator-transport-bindings/v3-mldsa65-mutual\0");
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
}
