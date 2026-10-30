//! Bounded, explicitly pinned NEW-domain initialization compiler.
//!
//! This does not activate a service, issue live grants, register existing UCA
//! identities or publish state. The independently approved manifest is the
//! initial authority of its new domain. Each identity, account management and
//! configured recovery key approves its subject and whole manifest through AOEM.
//! A supplied manifest/hash or proof
//! of a spending key alone is not authority to alter an existing chain.
use super::{decode_hex, GenesisConfig};
use crate::native_pipeline::business::direct_nov_fee::FeeState;
use crate::native_pipeline::business::nov_transfer_batch::{
    balance_key, fee_record_changes, nonce_key, uca_effect_contract, uca_program_id,
    uca_receipt_codec,
};
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::business::uca_spend_state::{
    account_record_changes, grant_record_changes, identity_association_changes,
    identity_record_changes, AssetAccountAuthorityV2,
};
use crate::native_pipeline::state::tree::{
    InitialStateBuilder, NodeHash, StagedStateUpdate, StateChange,
};
use anyhow::{ensure, Context, Result};
use novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1;
use novovm_adapter_api::uca_spend_policy::SuppliedAssetSpendGrantV2;
use novovm_adapter_api::unified_account::{
    UcaAccount, UcaKeyAlgo, UcaKeyProofType, UcaPrimaryKeyBinding, UcaStatus,
};
use novovm_exec::pq_signature::AoemMldsaParameterSet;
use novovm_exec::AoemExecFacade;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV2, MAX_UCA_ID_BYTES, MLDSA65_PUBLIC_KEY_BYTES,
    MLDSA65_SIGNATURE_BYTES, NATIVE_NOV_ASSET,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

// The Rust data structures remain shared with existing callers. The signed
// contract is explicitly v3: a bounded transcript commits the entire manifest.
pub const INITIALIZATION_CONTEXT_V2: &[u8] = b"NOVOVM/identity-and-asset/genesis-init/v3";
pub const MAX_INITIAL_GRANTS_PER_ACCOUNT: usize = 8;
mod pages;
pub use pages::UcaGenesisManifestPagesV3;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaGenesisGrantV2 {
    pub spend_public_key: Vec<u8>,
    pub grant: SuppliedAssetSpendGrantV2,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaGenesisIdentityV2 {
    pub uca_id: String,
    pub identity_public_key: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaGenesisAccountV2 {
    pub uca_id: String,
    pub asset_account: Vec<u8>,
    pub management_public_key: Vec<u8>,
    pub recovery_public_key: Option<Vec<u8>>,
    pub grants: Vec<UcaGenesisGrantV2>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaGenesisManifestV2 {
    /// Reuses current chain/validator/economic fields. Its old final commitment
    /// MUST be zero; the v2 initializer derives a distinct domain and final pin.
    pub genesis: GenesisConfig,
    pub identities: Vec<UcaGenesisIdentityV2>,
    pub accounts: Vec<UcaGenesisAccountV2>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaGenesisApprovalV2 {
    pub subject: UcaGenesisApprovalSubjectV2,
    // Local JSON metadata, not the signed transcript or native wire. Compact
    // text avoids expanding every ML-DSA approval into a numeric byte array.
    #[serde(with = "approval_signature")]
    pub signature: Vec<u8>,
}

mod approval_signature {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    const PREFIX: &str = "base64url:";
    const MAX_BYTES: usize = super::MLDSA65_SIGNATURE_BYTES;

    pub fn serialize<S: Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(value)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Encoded {
            Text(String),
            Bytes(Vec<u8>),
        }
        let bytes = match Encoded::deserialize(deserializer)? {
            Encoded::Text(text) => {
                let encoded = text
                    .strip_prefix(PREFIX)
                    .ok_or_else(|| serde::de::Error::custom("approval signature encoding"))?;
                if encoded.len() > MAX_BYTES.div_ceil(3) * 4 {
                    return Err(serde::de::Error::custom("approval signature byte bound"));
                }
                URL_SAFE_NO_PAD
                    .decode(encoded)
                    .map_err(serde::de::Error::custom)?
            }
            Encoded::Bytes(bytes) => bytes,
        };
        if bytes.len() > MAX_BYTES {
            return Err(serde::de::Error::custom("approval signature byte bound"));
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UcaGenesisApprovalSubjectV2 {
    Identity(String),
    AssetManagement(Vec<u8>),
    AssetRecovery(Vec<u8>),
}

/// Operator-pinned new-domain initialization material. These three independent
/// pins are mandatory; self-supplied management proofs do not authorize changing
/// an existing chain. Approval artifacts remain in this configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaInitializationConfigV2 {
    #[serde(default)]
    pub identities: Vec<UcaGenesisIdentityV2>,
    #[serde(default)]
    pub accounts: Vec<UcaGenesisAccountV2>,
    #[serde(default)]
    pub approvals: Vec<UcaGenesisApprovalV2>,
    /// Bounded local preparation pages, hydrated before validation. These paths
    /// confer no authority: all records remain bound by the three exact pins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<std::path::PathBuf>,
    pub expected_signing_domain: NodeHash,
    pub expected_initial_root: NodeHash,
    pub expected_final_commitment: NodeHash,
}

impl UcaInitializationConfigV2 {
    pub fn validate_manifest(&self, genesis: &GenesisConfig) -> Result<UcaGenesisManifestV2> {
        ensure!(
            self.expected_signing_domain != [0; 32]
                && self.expected_initial_root != [0; 32]
                && self.expected_final_commitment != [0; 32]
                && genesis.genesis_config_commitment == self.expected_signing_domain,
            "UCA initialization requires explicit domain/root/final pins"
        );
        ensure!(
            self.pages.is_empty()
                && !self.identities.is_empty()
                && self
                    .identities
                    .iter()
                    .all(|identity| identity.uca_id.len() <= MAX_UCA_ID_BYTES
                        && identity.identity_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES)
                && !self.accounts.is_empty()
                && genesis.validators.len() <= 64
                && self
                    .accounts
                    .iter()
                    .all(|a| a.uca_id.len() <= MAX_UCA_ID_BYTES
                        && matches!(a.asset_account.len(), 20 | 32)
                        && a.management_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
                        && a.recovery_public_key
                            .as_ref()
                            .is_none_or(|key| key.len() == MLDSA65_PUBLIC_KEY_BYTES)
                        && a.grants.len() <= MAX_INITIAL_GRANTS_PER_ACCOUNT
                        && a.grants
                            .iter()
                            .all(|g| g.spend_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES))
                && self.approvals.len()
                    == self.identities.len()
                        + self.accounts.len()
                        + self
                            .accounts
                            .iter()
                            .filter(|account| account.recovery_public_key.is_some())
                            .count()
                && self
                    .approvals
                    .iter()
                    .all(|a| a.signature.len() == MLDSA65_SIGNATURE_BYTES),
            "UCA initialization approval bounds"
        );
        let mut genesis = genesis.clone();
        genesis.genesis_config_commitment = [0; 32];
        let manifest = UcaGenesisManifestV2 {
            genesis,
            identities: self.identities.clone(),
            accounts: self.accounts.clone(),
        };
        ensure!(
            manifest.signing_domain()? == self.expected_signing_domain,
            "UCA initialization manifest/domain mismatch"
        );
        Ok(manifest)
    }

    /// Runs before opening any database in either fresh or existing startup.
    pub fn verify(
        &self,
        genesis: &GenesisConfig,
        runtime: &AoemExecFacade,
    ) -> Result<VerifiedUcaGenesisV2> {
        let manifest = self.validate_manifest(genesis)?;
        let verified = manifest.compile(runtime, self.expected_signing_domain, &self.approvals)?;
        ensure!(
            verified.signing_domain() == self.expected_signing_domain
                && verified.state().root() == self.expected_initial_root
                && verified.final_commitment() == self.expected_final_commitment,
            "UCA initialization verified domain/root/final pin mismatch"
        );
        Ok(verified)
    }
}

/// Immutable verified staging output, never deserializable from an App request.
/// Installing it still requires the existing fresh-store/publication owner.
pub struct VerifiedUcaGenesisV2 {
    signing_domain: NodeHash,
    final_commitment: NodeHash,
    state: StagedStateUpdate,
}

impl VerifiedUcaGenesisV2 {
    pub fn signing_domain(&self) -> NodeHash {
        self.signing_domain
    }
    pub fn final_commitment(&self) -> NodeHash {
        self.final_commitment
    }
    pub fn state(&self) -> &StagedStateUpdate {
        &self.state
    }
    pub(crate) fn into_state(self) -> StagedStateUpdate {
        self.state
    }
}

impl UcaGenesisManifestV2 {
    /// Compact v3 canonical commitment signed separately by EVERY identity,
    /// account-management and configured account-recovery key. Identity
    /// ownership does not replace either concrete account's approval.
    /// Proofs, the resulting root and the old claimed final pin are absent.
    /// Input vector ordering is immaterial; all identities are unique/sorted.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.genesis.genesis_config_commitment == [0; 32]
                && !self.identities.is_empty()
                && !self.accounts.is_empty(),
            "UCA initialization manifest bounds/old genesis pin"
        );
        // Original validator, allocation, policy and overflow validation. This
        // pure preparation neither reads nor writes an existing database.
        let (validators, allocations) = self.genesis.validated_allocations()?;
        let mut identities = BTreeMap::new();
        let mut identity_keys = BTreeSet::new();
        for identity in &self.identities {
            ensure!(
                valid_uca_id(&identity.uca_id)
                    && identity.identity_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
                    && identities.insert(&identity.uca_id, identity).is_none()
                    && identity_keys.insert(&identity.identity_public_key),
                "duplicate or invalid initial UCA identity/key"
            );
        }
        let mut accounts = BTreeMap::new();
        let mut management_keys = BTreeSet::new();
        let mut recovery_keys = BTreeSet::new();
        let mut spend_keys = BTreeSet::new();
        let validator_keys: BTreeSet<_> = self
            .genesis
            .validators
            .iter()
            .map(|validator| &validator.public_key)
            .collect();
        for account in &self.accounts {
            let id = &account.uca_id;
            ensure!(
                identities.contains_key(id)
                    && account.management_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
                    && !account.grants.is_empty()
                    && account.grants.len() <= MAX_INITIAL_GRANTS_PER_ACCOUNT,
                "invalid initial UCA identity/key/grant bounds"
            );
            Account::try_from(account.asset_account.as_slice()).map_err(anyhow::Error::msg)?;
            ensure!(
                accounts.insert(&account.asset_account, account).is_none()
                    && management_keys.insert(&account.management_public_key),
                "duplicate initial asset account or management key"
            );
            if let Some(key) = &account.recovery_public_key {
                ensure!(
                    key.len() == MLDSA65_PUBLIC_KEY_BYTES && recovery_keys.insert(key),
                    "duplicate or invalid initial asset recovery key"
                );
            }
            for entry in &account.grants {
                let grant = &entry.grant;
                ensure!(
                    spend_key_ref_v1(&entry.spend_public_key)? == grant.key_ref
                        && grant.account == account.asset_account
                        && grant.asset == NATIVE_NOV_ASSET
                        && grant.authorization_epoch == 1
                        && grant.permits_nov_transfer
                        && !grant.revoked
                        && grant.not_before_height > 0
                        && grant.expires_at_height > grant.not_before_height
                        && grant.max_debit_per_intent > 0
                        && grant.remaining_debit_budget > 0
                        && spend_keys.insert(&entry.spend_public_key),
                    "invalid or duplicate initial UCA spend authorization"
                );
            }
        }
        ensure!(
            management_keys.is_disjoint(&spend_keys)
                && management_keys.is_disjoint(&recovery_keys)
                && recovery_keys.is_disjoint(&spend_keys)
                && identity_keys.is_disjoint(&management_keys)
                && identity_keys.is_disjoint(&recovery_keys)
                && identity_keys.is_disjoint(&spend_keys),
            "identity, asset management, recovery and spending keys must be purpose-separated"
        );
        ensure!(
            validator_keys.is_disjoint(&identity_keys)
                && validator_keys.is_disjoint(&management_keys)
                && validator_keys.is_disjoint(&recovery_keys)
                && validator_keys.is_disjoint(&spend_keys),
            "validator signing keys must not authorize identity or asset operations"
        );
        let mut out = b"NOVOVM/identity-and-asset/initial-manifest/v3\0".to_vec();
        out.extend_from_slice(&self.genesis.chain_id.to_be_bytes());
        out.extend_from_slice(&self.genesis.validator_epoch.to_be_bytes());
        out.extend_from_slice(&self.genesis.timestamp_unix_ms.to_be_bytes());
        out.extend_from_slice(&self.genesis.protocol_commitment);
        out.extend_from_slice(&validators.hash());
        out.extend_from_slice(
            &self
                .genesis
                .transport_bindings(&validators)?
                .canonical_commitment(),
        );
        // Explicit new candidate semantics, not a relabelled old V3 plan.
        out.extend_from_slice(&uca_program_id());
        out.extend_from_slice(&uca_receipt_codec());
        out.extend_from_slice(&uca_effect_contract(&self.genesis.policy)?);
        append(&mut out, &postcard::to_allocvec(&self.genesis.policy)?)?;
        let allocations = allocations
            .into_iter()
            .map(|(account, amount)| (account.as_bytes().to_vec(), amount))
            .collect::<BTreeMap<_, _>>();
        let mut allocation_hash = Sha256::new();
        for (account, amount) in &allocations {
            let mut record = Vec::new();
            append(&mut record, account)?;
            record.extend_from_slice(&amount.to_be_bytes());
            allocation_hash.update(record);
        }
        out.extend_from_slice(&(allocations.len() as u64).to_be_bytes());
        out.extend_from_slice(&allocation_hash.finalize());
        out.extend_from_slice(&(identities.len() as u64).to_be_bytes());
        let mut identity_hash = Sha256::new();
        for identity in identities.values() {
            let mut record = Vec::new();
            append(&mut record, identity.uca_id.as_bytes())?;
            append(&mut record, &identity.identity_public_key)?;
            identity_hash.update(record);
        }
        out.extend_from_slice(&identity_hash.finalize());
        out.extend_from_slice(&(accounts.len() as u64).to_be_bytes());
        let mut account_hash = Sha256::new();
        for account in accounts.values() {
            let mut record = Vec::new();
            append(&mut record, account.uca_id.as_bytes())?;
            append(&mut record, &account.asset_account)?;
            append(&mut record, &account.management_public_key)?;
            match &account.recovery_public_key {
                Some(key) => {
                    record.push(1);
                    append(&mut record, key)?;
                }
                None => record.push(0),
            }
            // Initial spend epoch=1 and stable account nonce=0 are explicit.
            record.extend_from_slice(&1u64.to_be_bytes());
            record.extend_from_slice(&0u64.to_be_bytes());
            let grants = account
                .grants
                .iter()
                .map(|g| (g.grant.key_ref, g))
                .collect::<BTreeMap<_, _>>();
            record.extend_from_slice(&(grants.len() as u32).to_be_bytes());
            for entry in grants.values() {
                append(&mut record, &entry.spend_public_key)?;
                append(&mut record, &postcard::to_allocvec(&entry.grant)?)?;
            }
            account_hash.update((record.len() as u64).to_be_bytes());
            account_hash.update(record);
        }
        out.extend_from_slice(&account_hash.finalize());
        Ok(out)
    }

    pub fn signing_domain(&self) -> Result<NodeHash> {
        Ok(Sha256::digest(self.canonical_bytes()?).into())
    }

    /// expected_domain must come from the independently approved/pinned new
    /// domain manifest, NOT from the same untrusted packet as this object.
    /// Approval cannot mutate an existing chain, grant or identity record.
    /// The caller retains raw approval artifacts. State stores their verified
    /// meaning, not randomized signature bytes: signing the SAME manifest again
    /// must not change its initial state root or final commitment.
    pub fn compile(
        &self,
        runtime: &AoemExecFacade,
        expected_domain: NodeHash,
        approvals: &[UcaGenesisApprovalV2],
    ) -> Result<VerifiedUcaGenesisV2> {
        let plan = self.prepare_approvals()?;
        let domain = plan.domain();
        ensure!(
            domain == expected_domain && domain != [0; 32],
            "UCA initialization pin mismatch"
        );
        let expected = &plan.subjects;
        ensure!(
            approvals.len() == expected.len(),
            "missing/extra identity or asset approval"
        );
        let mut signatures = BTreeMap::new();
        for approval in approvals {
            ensure!(
                expected.contains_key(&approval.subject)
                    && approval.signature.len() == MLDSA65_SIGNATURE_BYTES
                    && signatures
                        .insert(approval.subject.clone(), &approval.signature)
                        .is_none(),
                "duplicate, unknown or malformed identity/asset approval"
            );
        }
        let verifier = runtime.mldsa_verifier_v1(AoemMldsaParameterSet::MlDsa65)?;
        for (subject, public_key) in expected {
            verifier.verify(
                public_key,
                &plan.signing_bytes(subject)?,
                INITIALIZATION_CONTEXT_V2,
                signatures
                    .get(subject)
                    .context("missing identity/asset purpose approval")?,
            )?;
        }
        let mut state = InitialStateBuilder::new();
        state.extend(fee_record_changes(
            &self.genesis.policy,
            &FeeState::default(),
        )?)?;
        let mut funded = BTreeSet::new();
        for allocation in &self.genesis.allocations {
            let address = decode_hex(&allocation.account_hex)?;
            funded.insert(address.clone());
            let account = Account::try_from(address).map_err(anyhow::Error::msg)?;
            state.extend([StateChange::Put {
                key: balance_key(&account),
                value: allocation.amount.parse::<u128>()?.to_le_bytes().to_vec(),
            }])?;
        }
        for identity in &self.identities {
            let record = UcaAccount {
                uca_id: identity.uca_id.clone(),
                primary_key_ref: derive_primary_key_ref_from_binding_v1(
                    UcaKeyAlgo::Mldsa65,
                    &identity.identity_public_key,
                ),
                primary_key_binding: Some(UcaPrimaryKeyBinding {
                    key_algo: UcaKeyAlgo::Mldsa65,
                    public_key: identity.identity_public_key.clone(),
                    proof_type: UcaKeyProofType::GenesisInitializationV1,
                    proof_payload: verified_initialization_commitment(
                        &domain,
                        &identity.identity_public_key,
                    )
                    .to_vec(),
                    verified_at: self.genesis.timestamp_unix_ms,
                }),
                status: UcaStatus::Active,
                created_at: self.genesis.timestamp_unix_ms,
                updated_at: self.genesis.timestamp_unix_ms,
            };
            state.extend(identity_record_changes(
                self.genesis.chain_id,
                &domain,
                &record,
            )?)?;
        }
        for account in &self.accounts {
            let record = AssetAccountAuthorityV2 {
                account: account.asset_account.clone(),
                asset: NATIVE_NOV_ASSET,
                active: true,
                current_spend_epoch: 1,
                management_key_ref: spend_key_ref_v1(&account.management_public_key)?,
                recovery_key_ref: account
                    .recovery_public_key
                    .as_ref()
                    .map(|key| spend_key_ref_v1(key))
                    .transpose()?,
            };
            state.extend(account_record_changes(
                self.genesis.chain_id,
                &domain,
                &record,
            )?)?;
            state.extend(identity_association_changes(
                self.genesis.chain_id,
                &domain,
                &account.uca_id,
                &Account::try_from(account.asset_account.as_slice()).map_err(anyhow::Error::msg)?,
            )?)?;
            if !funded.contains(&account.asset_account) {
                let address = Account::try_from(account.asset_account.as_slice())
                    .map_err(anyhow::Error::msg)?;
                state.extend([StateChange::Put {
                    key: balance_key(&address),
                    value: 0u128.to_le_bytes().to_vec(),
                }])?;
            }
            // One stable nonce per concrete account, never per identity/key.
            let mut first = true;
            for entry in &account.grants {
                let intent = AssetSpendIntentV2 {
                    chain_id: self.genesis.chain_id,
                    genesis: domain,
                    payer: account.asset_account.clone(),
                    authorization_epoch: 1,
                    spend_key_ref: entry.grant.key_ref,
                    recipient: account.asset_account.clone(),
                    asset: NATIVE_NOV_ASSET,
                    amount: 0,
                    max_fee: 0,
                    nonce: 0,
                    not_before_height: 0,
                    expires_at_height: 1,
                    authority_update: None,
                    recipient_creation: None,
                };
                if first {
                    state.extend([StateChange::Put {
                        key: nonce_key(&intent.nonce_identity()?),
                        value: 0u64.to_le_bytes().to_vec(),
                    }])?;
                    first = false;
                }
                state.extend(grant_record_changes(&intent, &entry.grant)?)?;
            }
        }
        let state = state.finish()?;
        let mut final_hash = Sha256::new();
        final_hash.update(b"NOVOVM/identity-and-asset/initial-final-commitment/v3\0");
        final_hash.update(domain);
        final_hash.update(state.root());
        Ok(VerifiedUcaGenesisV2 {
            signing_domain: domain,
            final_commitment: final_hash.finalize().into(),
            state,
        })
    }

    /// The one canonical approval transcript consumed by node, CLI and clients.
    /// The explicit subject prevents reuse across identities/accounts/purposes.
    pub fn approval_signing_bytes(&self, subject: &UcaGenesisApprovalSubjectV2) -> Result<Vec<u8>> {
        self.prepare_approvals()?.signing_bytes(subject)
    }

    pub fn prepare_approvals(&self) -> Result<UcaGenesisApprovalPlanV3> {
        Ok(UcaGenesisApprovalPlanV3 {
            domain: self.signing_domain()?,
            subjects: self.required_approval_subjects().into_iter().collect(),
        })
    }

    /// Deterministically ordered required approvals. The canonical manifest
    /// validates uniqueness/purpose separation before these can authorize state.
    pub fn required_approval_subjects(&self) -> Vec<(UcaGenesisApprovalSubjectV2, Vec<u8>)> {
        let mut keys = BTreeMap::new();
        for identity in &self.identities {
            keys.insert(
                UcaGenesisApprovalSubjectV2::Identity(identity.uca_id.clone()),
                identity.identity_public_key.clone(),
            );
        }
        for account in &self.accounts {
            keys.insert(
                UcaGenesisApprovalSubjectV2::AssetManagement(account.asset_account.clone()),
                account.management_public_key.clone(),
            );
            if let Some(key) = &account.recovery_public_key {
                keys.insert(
                    UcaGenesisApprovalSubjectV2::AssetRecovery(account.asset_account.clone()),
                    key.clone(),
                );
            }
        }
        keys.into_iter().collect()
    }
}

/// Prepared immutable approval transcript. Computing the full manifest once
/// avoids quadratic canonicalization/signing when initializing many accounts.
pub struct UcaGenesisApprovalPlanV3 {
    domain: NodeHash,
    subjects: BTreeMap<UcaGenesisApprovalSubjectV2, Vec<u8>>,
}

impl UcaGenesisApprovalPlanV3 {
    pub fn domain(&self) -> NodeHash {
        self.domain
    }

    pub fn subjects(&self) -> impl Iterator<Item = (&UcaGenesisApprovalSubjectV2, &[u8])> {
        self.subjects
            .iter()
            .map(|(subject, key)| (subject, key.as_slice()))
    }

    pub fn signing_bytes(&self, subject: &UcaGenesisApprovalSubjectV2) -> Result<Vec<u8>> {
        ensure!(
            self.subjects.contains_key(subject),
            "unknown identity/asset approval subject"
        );
        let mut bytes = b"NOVOVM/identity-and-asset/approval/v3\0".to_vec();
        bytes.extend_from_slice(&self.domain);
        append(&mut bytes, &postcard::to_allocvec(subject)?)?;
        Ok(bytes)
    }
}

fn valid_uca_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_UCA_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

/// NOT signature verification. Called only after all original ML-DSA approvals
/// pass; the separate proof type prevents treating this digest as SignatureV1.
fn verified_initialization_commitment(manifest: &[u8], identity_key: &[u8]) -> NodeHash {
    let mut h = Sha256::new();
    h.update(b"NOVOVM/identity-and-asset/verified-initialization-commitment/v3\0");
    h.update((manifest.len() as u64).to_be_bytes());
    h.update(manifest);
    h.update((identity_key.len() as u32).to_be_bytes());
    h.update(identity_key);
    h.finalize().into()
}

fn append(out: &mut Vec<u8>, field: &[u8]) -> Result<()> {
    ensure!(
        u32::try_from(field.len()).is_ok()
            && out
                .len()
                .checked_add(field.len())
                .and_then(|n| n.checked_add(4))
                .is_some(),
        "UCA initialization field size overflow"
    );
    out.extend_from_slice(&(field.len() as u32).to_be_bytes());
    out.extend_from_slice(field);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_json_is_compact_exact_and_bounded_without_changing_signed_bytes() {
        let approval = UcaGenesisApprovalV2 {
            subject: UcaGenesisApprovalSubjectV2::AssetManagement(vec![3; 20]),
            signature: (0..MLDSA65_SIGNATURE_BYTES).map(|n| n as u8).collect(),
        };
        let encoded = serde_json::to_vec_pretty(&approval).unwrap();
        assert!(encoded.len() < 5_000);
        let decoded: UcaGenesisApprovalV2 = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.subject, approval.subject);
        assert_eq!(decoded.signature, approval.signature);
        let bytes_document = serde_json::json!({
            "subject": approval.subject, "signature": approval.signature,
        });
        let decoded: UcaGenesisApprovalV2 = serde_json::from_value(bytes_document).unwrap();
        assert_eq!(decoded.signature, approval.signature);
        for text in ["base64url:!", "unversioned-signature"] {
            let mut invalid = serde_json::to_value(&approval).unwrap();
            invalid["signature"] = serde_json::json!(text);
            assert!(serde_json::from_value::<UcaGenesisApprovalV2>(invalid).is_err());
        }
        let oversized = serde_json::json!({
            "subject":approval.subject,"signature":vec![0; MLDSA65_SIGNATURE_BYTES + 1],
        });
        assert!(serde_json::from_value::<UcaGenesisApprovalV2>(oversized).is_err());
    }
    use crate::native_pipeline::service::config::{
        protocol_commitment, GenesisAllocation, GenesisValidator,
    };

    fn manifest() -> UcaGenesisManifestV2 {
        let accounts = (1u8..=2)
            .map(|i| {
                let spend_public_key = vec![i + 20; MLDSA65_PUBLIC_KEY_BYTES];
                UcaGenesisAccountV2 {
                    uca_id: "uca:owner".into(),
                    asset_account: vec![i; 20],
                    management_public_key: vec![i + 10; MLDSA65_PUBLIC_KEY_BYTES],
                    recovery_public_key: Some(vec![i + 40; MLDSA65_PUBLIC_KEY_BYTES]),
                    grants: vec![UcaGenesisGrantV2 {
                        grant: SuppliedAssetSpendGrantV2 {
                            account: vec![i; 20],
                            asset: NATIVE_NOV_ASSET,
                            key_ref: spend_key_ref_v1(&spend_public_key).unwrap(),
                            authorization_epoch: 1,
                            permits_nov_transfer: true,
                            revoked: false,
                            not_before_height: 1,
                            expires_at_height: 100,
                            max_debit_per_intent: 100,
                            remaining_debit_budget: 1000,
                        },
                        spend_public_key,
                    }],
                }
            })
            .collect();
        UcaGenesisManifestV2 {
            genesis: GenesisConfig {
                chain_id: 71,
                validator_epoch: 1,
                protocol_commitment: protocol_commitment(),
                genesis_config_commitment: [0; 32],
                timestamp_unix_ms: 123,
                validators: (1..=4)
                    .map(|seed| GenesisValidator {
                        transport_public_key: ed25519_dalek::SigningKey::from_bytes(
                            &[100 + seed; 32],
                        )
                        .verifying_key()
                        .to_bytes(),
                        // Canonical-manifest fixture, not a signing backend.
                        public_key: vec![seed; MLDSA65_PUBLIC_KEY_BYTES],
                        weight: 1,
                    })
                    .collect(),
                allocations: vec![GenesisAllocation {
                    account_hex: "01".repeat(20),
                    amount:
                        crate::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS
                            .to_string(),
                }],
                policy: serde_json::from_value(serde_json::json!({
                    "quote_ttl_ms": 15000, "policy_version": 1,
                    "policy_source": "test-only", "resolution_source": "runtime_state",
                    "reserve_share_bps": 7000, "fee_share_bps": 2000,
                    "risk_buffer_share_bps": 1000, "min_reserve_bucket_nov": 0,
                    "min_fee_bucket_nov": 0, "min_risk_buffer_nov": 1,
                    "settlement_paused": false, "redeem_paused": false,
                    "clearing_enabled": true, "clearing_daily_nov_hard_limit": 1000000,
                    "clearing_require_healthy_risk_buffer": false,
                    "clearing_constrained_max_slippage_bps": 500,
                    "clearing_constrained_daily_usage_bps": 8000,
                    "clearing_constrained_strategy": "daily_volume_only",
                    "mapped_asset_auto_heal_rollback_enabled": false,
                    "mapped_asset_reorg_response_policy": "report_only"
                }))
                .unwrap(),
            },
            identities: vec![
                UcaGenesisIdentityV2 {
                    uca_id: "uca:owner".into(),
                    identity_public_key: vec![50; MLDSA65_PUBLIC_KEY_BYTES],
                },
                UcaGenesisIdentityV2 {
                    uca_id: "uca:observer".into(),
                    identity_public_key: vec![51; MLDSA65_PUBLIC_KEY_BYTES],
                },
            ],
            accounts,
        }
    }

    #[test]
    fn uca_manifest_canonical_order_and_every_authority_field_are_bound() {
        let base = manifest();
        let domain = base.signing_domain().unwrap();
        let mut reordered = base.clone();
        reordered.identities.reverse();
        reordered.accounts.reverse();
        reordered.genesis.validators.reverse();
        assert_eq!(domain, reordered.signing_domain().unwrap());
        for field in 0..14 {
            let mut changed = base.clone();
            match field {
                0 => changed.genesis.chain_id += 1,
                1 => changed.genesis.timestamp_unix_ms += 1,
                2 => changed.genesis.validators[0].weight += 1,
                3 => {
                    changed.genesis.allocations[0].amount = (crate::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS - 1).to_string();
                    changed.genesis.allocations.push(GenesisAllocation {
                        account_hex: "02".repeat(20),
                        amount: "1".into(),
                    });
                }
                4 => changed.accounts[0].grants[0].grant.remaining_debit_budget += 1,
                5 => changed.accounts[0].grants[0].grant.max_debit_per_intent += 1,
                6 => {
                    changed.accounts[0].asset_account[0] = 9;
                    changed.accounts[0].grants[0].grant.account[0] = 9;
                }
                7 => changed.genesis.policy.quote_ttl_ms += 1,
                8 => {
                    let first = changed.genesis.validators[0].transport_public_key;
                    changed.genesis.validators[0].transport_public_key =
                        changed.genesis.validators[1].transport_public_key;
                    changed.genesis.validators[1].transport_public_key = first;
                }
                9 => changed.identities[0].identity_public_key[0] ^= 1,
                10 => changed.accounts[0].management_public_key[0] ^= 1,
                11 => changed.accounts[0].recovery_public_key.as_mut().unwrap()[0] ^= 1,
                12 => changed.accounts[0].uca_id = "uca:observer".into(),
                13 => changed.accounts[0].recovery_public_key = None,
                _ => unreachable!(),
            }
            assert_ne!(domain, changed.signing_domain().unwrap(), "field {field}");
        }
    }

    #[test]
    fn uca_manifest_rejects_alias_owners_wrong_keys_and_grant_shapes() {
        for field in 0..17 {
            let mut bad = manifest();
            match field {
                0 => bad.genesis.genesis_config_commitment = [7; 32],
                1 => bad.accounts[1].asset_account = bad.accounts[0].asset_account.clone(),
                2 => bad.accounts[1].uca_id = "uca:unregistered".into(),
                3 => {
                    let entry = bad.accounts[0].grants[0].clone();
                    bad.accounts[0].grants.push(entry);
                }
                4 => bad.accounts[0].grants[0].grant.authorization_epoch = 2,
                5 => bad.accounts[0].grants[0].grant.permits_nov_transfer = false,
                6 => bad.accounts[0].grants[0].grant.key_ref = [0; 32],
                7 => {
                    bad.accounts[0].management_public_key =
                        bad.accounts[0].grants[0].spend_public_key.clone()
                }
                8 => {
                    bad.accounts[0].management_public_key.truncate(1);
                }
                9 => bad.accounts = vec![bad.accounts[0].clone(); 33],
                10 => bad.identities.push(bad.identities[0].clone()),
                11 => {
                    bad.accounts[0].grants[0].grant.account = bad.accounts[1].asset_account.clone()
                }
                12 => bad.accounts[0].grants[0].grant.asset = 2,
                13 => {
                    bad.accounts[0].recovery_public_key =
                        Some(bad.accounts[0].management_public_key.clone())
                }
                14 => {
                    bad.accounts[0].management_public_key =
                        bad.identities[0].identity_public_key.clone()
                }
                15 => {
                    bad.identities[1].identity_public_key =
                        bad.identities[0].identity_public_key.clone()
                }
                16 => {
                    bad.accounts[1].recovery_public_key =
                        bad.accounts[0].recovery_public_key.clone()
                }
                _ => unreachable!(),
            }
            assert!(bad.canonical_bytes().is_err(), "field {field}");
        }
    }

    #[test]
    fn one_identity_has_independent_asset_accounts_and_subject_bound_approvals() {
        let manifest = manifest();
        assert_eq!(manifest.accounts[0].uca_id, manifest.accounts[1].uca_id);
        assert!(manifest.canonical_bytes().is_ok());
        let approvals = manifest.required_approval_subjects();
        assert_eq!(approvals.len(), 6);
        let mut transcripts = BTreeSet::new();
        for (subject, _) in &approvals {
            assert!(transcripts.insert(manifest.approval_signing_bytes(subject).unwrap()));
        }
        let mut reordered = manifest.clone();
        reordered.accounts.reverse();
        reordered.identities.reverse();
        assert_eq!(approvals, reordered.required_approval_subjects());
        for (subject, _) in approvals {
            assert_eq!(
                manifest.approval_signing_bytes(&subject).unwrap(),
                reordered.approval_signing_bytes(&subject).unwrap()
            );
        }
        assert!(manifest
            .approval_signing_bytes(&UcaGenesisApprovalSubjectV2::AssetManagement(vec![9; 20]))
            .is_err());
        assert!(manifest
            .approval_signing_bytes(&UcaGenesisApprovalSubjectV2::Identity("uca:unknown".into()))
            .is_err());
    }

    #[test]
    fn obsolete_genesis_approval_shape_is_not_reinterpreted_as_asset_authority() {
        let old = serde_json::json!({"uca_id": "uca:owner", "signature": [0, 1]});
        assert!(serde_json::from_value::<UcaGenesisApprovalV2>(old).is_err());
        let mut old = serde_json::to_value(manifest()).unwrap();
        old.as_object_mut().unwrap().remove("identities");
        assert!(serde_json::from_value::<UcaGenesisManifestV2>(old).is_err());
    }

    #[test]
    fn uca_manifest_new_tags_do_not_relabel_legacy_key_or_proof() {
        assert_eq!(postcard::to_allocvec(&UcaKeyAlgo::Mldsa87).unwrap(), [2]);
        assert_eq!(postcard::to_allocvec(&UcaKeyAlgo::Mldsa65).unwrap(), [3]);
        assert_eq!(
            postcard::to_allocvec(&UcaKeyProofType::SignatureV1).unwrap(),
            [0]
        );
        assert_eq!(
            postcard::to_allocvec(&UcaKeyProofType::GenesisInitializationV1).unwrap(),
            [1]
        );
        assert_ne!(
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &[7; 32]),
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa87, &[7; 32]),
        );
    }

    #[test]
    fn validator_key_cannot_also_manage_or_spend_genesis_assets() {
        let mut management = manifest();
        management.accounts[0].management_public_key =
            management.genesis.validators[0].public_key.clone();
        assert!(management
            .canonical_bytes()
            .unwrap_err()
            .to_string()
            .contains("validator signing keys"));
        let mut spending = manifest();
        let key = spending.genesis.validators[0].public_key.clone();
        spending.accounts[0].grants[0].grant.key_ref = spend_key_ref_v1(&key).unwrap();
        spending.accounts[0].grants[0].spend_public_key = key;
        assert!(spending
            .canonical_bytes()
            .unwrap_err()
            .to_string()
            .contains("validator signing keys"));
    }

    fn many_accounts(count: usize) -> UcaGenesisManifestV2 {
        let mut manifest = manifest();
        let template = manifest.accounts[0].clone();
        manifest.accounts = (0..count)
            .map(|index| {
                let mut account = template.clone();
                account.asset_account[12..].copy_from_slice(&(index as u64).to_be_bytes());
                account.management_public_key = vec![70; MLDSA65_PUBLIC_KEY_BYTES];
                account.management_public_key[..8].copy_from_slice(&(index as u64).to_be_bytes());
                account.recovery_public_key = None;
                let spend = &mut account.grants[0];
                spend.spend_public_key = vec![90; MLDSA65_PUBLIC_KEY_BYTES];
                spend.spend_public_key[..8].copy_from_slice(&(index as u64).to_be_bytes());
                spend.grant.account = account.asset_account.clone();
                spend.grant.key_ref = spend_key_ref_v1(&spend.spend_public_key).unwrap();
                account
            })
            .collect();
        manifest
    }

    #[test]
    fn initialization_has_no_32_account_ceiling_and_approval_work_is_linear() {
        for count in [33, 1025] {
            let manifest = many_accounts(count);
            let plan = manifest.prepare_approvals().unwrap();
            assert_eq!(plan.subjects().count(), count + manifest.identities.len());
            assert!(manifest.canonical_bytes().unwrap().len() < 4096);
            let subject = UcaGenesisApprovalSubjectV2::AssetManagement(
                manifest.accounts[count - 1].asset_account.clone(),
            );
            let bytes = plan.signing_bytes(&subject).unwrap();
            assert!(bytes.starts_with(b"NOVOVM/identity-and-asset/approval/v3\0"));
            assert!(bytes.len() < 256);
            let mut changed = manifest.clone();
            changed.accounts[0].grants[0].grant.remaining_debit_budget += 1;
            assert_ne!(plan.domain(), changed.signing_domain().unwrap());
            let mut reversed = manifest;
            reversed.accounts.reverse();
            assert_eq!(plan.domain(), reversed.signing_domain().unwrap());
        }
    }

    #[test]
    fn paged_initialization_preserves_pins_and_rejects_mixed_or_tampered_material() -> Result<()> {
        let manifest = many_accounts(65);
        let material = UcaInitializationConfigV2 {
            pages: vec![],
            identities: manifest.identities.clone(),
            accounts: manifest.accounts.clone(),
            approvals: vec![],
            expected_signing_domain: manifest.signing_domain()?,
            expected_initial_root: [7; 32],
            expected_final_commitment: [8; 32],
        };
        let directory = std::env::temp_dir().join(format!(
            "novovm-genesis-page-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let header = material.write_pages(&directory, 32)?;
        assert_eq!(header.pages.len(), 4);
        assert!(header.accounts.is_empty());
        let mut mixed = header.clone();
        mixed.accounts.push(manifest.accounts[0].clone());
        assert!(mixed
            .hydrate_pages_with_genesis(&directory, &mut manifest.genesis.clone())
            .is_err());
        let mut decoded = header;
        decoded.hydrate_pages_with_genesis(&directory, &mut manifest.genesis.clone())?;
        assert!(decoded.pages.is_empty());
        assert_eq!(decoded.accounts.len(), 65);
        assert_eq!(
            decoded.expected_signing_domain,
            material.expected_signing_domain
        );
        let reconstructed = UcaGenesisManifestV2 {
            genesis: manifest.genesis,
            identities: decoded.identities,
            accounts: decoded.accounts,
        };
        assert_eq!(
            reconstructed.signing_domain()?,
            material.expected_signing_domain
        );
        let mut altered = reconstructed;
        altered.accounts[0].grants[0].grant.max_debit_per_intent += 1;
        assert_ne!(altered.signing_domain()?, material.expected_signing_domain);
        Ok(())
    }

    #[test]
    fn allocated_accounts_page_out_of_header_and_round_trip_without_changing_domain() -> Result<()>
    {
        let mut manifest = many_accounts(65);
        let supply = crate::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS;
        manifest.genesis.allocations = manifest
            .accounts
            .iter()
            .enumerate()
            .map(|(index, account)| super::super::GenesisAllocation {
                account_hex: account
                    .asset_account
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
                amount: if index == 64 {
                    (supply - 64).to_string()
                } else {
                    "1".into()
                },
            })
            .collect();
        let domain = manifest.signing_domain()?;
        let directory = std::env::temp_dir().join(format!(
            "novovm-allocation-page-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let header = manifest.write_pages(&directory.join("manifest"), 32)?;
        assert!(header.genesis.allocations.is_empty());
        let header_file = directory.join("public-manifest.json");
        std::fs::write(&header_file, serde_json::to_vec(&header)?)?;
        let loaded = UcaGenesisManifestV2::read_file(&header_file)?;
        assert_eq!(loaded.genesis.allocations.len(), 65);
        assert_eq!(loaded.signing_domain()?, domain);
        let material = UcaInitializationConfigV2 {
            identities: loaded.identities.clone(),
            accounts: loaded.accounts.clone(),
            approvals: vec![],
            pages: vec![],
            expected_signing_domain: domain,
            expected_initial_root: [7; 32],
            expected_final_commitment: [8; 32],
        };
        let mut genesis = loaded.genesis.clone();
        let mut material_header =
            material.write_pages_with_genesis(&mut genesis, &directory.join("startup"), 32)?;
        assert!(genesis.allocations.is_empty());
        let mut mixed = material_header.clone();
        assert!(mixed
            .hydrate_pages_with_genesis(&directory, &mut loaded.genesis.clone())
            .is_err());
        material_header.hydrate_pages_with_genesis(&directory, &mut genesis)?;
        assert_eq!(genesis.allocations.len(), 65);
        assert_eq!(material_header.expected_signing_domain, domain);
        let mut changed = UcaGenesisManifestV2 {
            genesis,
            identities: material_header.identities,
            accounts: material_header.accounts,
        };
        assert_eq!(changed.signing_domain()?, domain);
        changed.genesis.allocations[0].amount = "2".into();
        changed.genesis.allocations[64].amount = (supply - 65).to_string();
        assert_ne!(changed.signing_domain()?, domain);
        Ok(())
    }

    #[test]
    #[ignore = "requires trusted NOVOVM_AOEM_TEST_LIBRARY; genuine independent ML-DSA approvals above 32 accounts, not deployment"]
    fn actual_65_account_initialization_requires_independent_approvals() -> Result<()> {
        let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM library required")?;
        let runtime = AoemExecFacade::open(
            std::path::Path::new(&library),
            novovm_exec::AoemExecOpenOptions::default(),
        )?;
        let keygen = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
        let mut manifest = many_accounts(65);
        let mut keys = Vec::new();
        for identity in &mut manifest.identities {
            let key = keygen.generate_keypair()?;
            identity.identity_public_key = key.public_key.clone();
            keys.push((key.public_key, key.expanded_secret_key));
        }
        for account in &mut manifest.accounts {
            let management = keygen.generate_keypair()?;
            account.management_public_key = management.public_key.clone();
            keys.push((management.public_key, management.expanded_secret_key));
            let spending = keygen.generate_keypair()?;
            account.grants[0].grant.key_ref = spend_key_ref_v1(&spending.public_key)?;
            account.grants[0].spend_public_key = spending.public_key;
        }
        let references = keys
            .iter()
            .map(|(public, secret)| (public.as_slice(), secret.as_slice()))
            .collect::<Vec<_>>();
        let approvals =
            crate::native_pipeline::service::config::test_asset_genesis::approve_manifest(
                &runtime,
                &manifest,
                &references,
            )?;
        let domain = manifest.signing_domain()?;
        let verified = manifest.compile(&runtime, domain, &approvals)?;
        assert_ne!(
            verified.state().root(),
            crate::native_pipeline::state::tree::empty_root()
        );
        assert!(verified.state().nodes().len() > 65);
        let mut forged = approvals.clone();
        forged[0].signature[0] ^= 1;
        assert!(manifest.compile(&runtime, domain, &forged).is_err());
        let mut changed = manifest;
        changed.accounts[0].grants[0].grant.remaining_debit_budget += 1;
        assert!(changed.compile(&runtime, domain, &approvals).is_err());
        Ok(())
    }
}
