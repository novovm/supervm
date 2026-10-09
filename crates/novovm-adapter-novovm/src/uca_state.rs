//! Versioned native account records and pure administrative state reduction.
//! These are candidate business effects, NOT admission, fees, durable commit,
//! anonymous credentials or finality. No registration from a transaction's root.
use crate::uca_delegation::{verify_uca_transaction_v4, UcaDelegationContextV1};
use anyhow::{ensure, Context, Result};
use novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1;
use novovm_adapter_api::unified_account::UcaAccount;
use novovm_adapter_api::UcaKeyAlgo;
use novovm_protocol::uca_delegation::MAX_ACCOUNT_BYTES;
use novovm_protocol::NovTxKindV1;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_AUTHORIZATIONS_V1: usize = 64;
pub const MAX_AUTHORIZATION_LIFETIME_BLOCKS_V1: u64 = 65_536;
pub const MAX_ACCOUNT_RECORD_BYTES_V1: usize = 32 * 1024;
const COMMAND_MAGIC: &[u8; 5] = b"NUCA\x01";

/// Only an opaque, independently randomized credential commitment is stored.
/// Never put peer-visible chat keys, contact identifiers or network certificates
/// here. This record alone provides no zero-knowledge or unlinkability proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaAuthorizationCommitmentV1 {
    pub app_scope: [u8; 32],
    pub issued_at_height: u64,
    pub expires_at_height: u64,
    pub revoked: bool,
}

/// The existing UCA identity type, within the native consensus record owner.
/// The native V4 administrative nonce has its own explicit domain; it is neither
/// a V3 direct-signer nonce nor a wallet spending authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeUcaAccountRecordV1 {
    pub version: u16,
    pub account: UcaAccount,
    pub delegation_epoch: u64,
    pub next_nonce: u64,
    pub authorizations: BTreeMap<String, UcaAuthorizationCommitmentV1>,
}

impl NativeUcaAccountRecordV1 {
    pub fn validate(&self, expected_account: &str) -> Result<()> {
        ensure!(self.version == 1, "unsupported native UCA record version");
        ensure!(
            self.account.uca_id == expected_account
                && !expected_account.is_empty()
                && expected_account.len() <= MAX_ACCOUNT_BYTES
                && expected_account
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
                && self.delegation_epoch > 0
                && self.account.updated_at >= self.account.created_at,
            "invalid native UCA account identity/epoch"
        );
        let binding = self
            .account
            .primary_key_binding
            .as_ref()
            .context("native UCA root binding missing")?;
        // This first record/transition version admits Ed25519 only; other key
        // algorithms require an explicit implementation, never a fallback.
        ensure!(
            binding.key_algo == UcaKeyAlgo::Ed25519 && binding.proof_payload.len() <= 64,
            "unsupported native UCA binding"
        );
        let bytes: [u8; 32] = binding.public_key.as_slice().try_into()?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes)?;
        ensure!(
            !key.is_weak()
                && self.account.primary_key_ref
                    == derive_primary_key_ref_from_binding_v1(
                        binding.key_algo,
                        &binding.public_key
                    ),
            "invalid native UCA root reference"
        );
        ensure!(
            self.authorizations.len() <= MAX_AUTHORIZATIONS_V1,
            "native UCA authorization capacity exceeded"
        );
        for (id, grant) in &self.authorizations {
            ensure!(
                id.len() == 64
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    && id.bytes().any(|b| b != b'0')
                    && grant.app_scope != [0; 32]
                    && grant.issued_at_height > 0
                    && grant.expires_at_height > grant.issued_at_height
                    && grant.expires_at_height - grant.issued_at_height
                        <= MAX_AUTHORIZATION_LIFETIME_BLOCKS_V1,
                "invalid native UCA authorization record"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_ACCOUNT_RECORD_BYTES_V1,
            "native UCA account record too large"
        );
        Ok(())
    }
}

/// Bounded exact command bytes signed inside the existing V4 intent. No JSON,
/// ambiguous duplicate fields, raw device keys or free-form permissions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UcaDeviceCommandV1 {
    Authorize {
        commitment: [u8; 32],
        app_scope: [u8; 32],
        expires_at_height: u64,
    },
    Revoke {
        commitment: [u8; 32],
        app_scope: [u8; 32],
    },
}

impl UcaDeviceCommandV1 {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Authorize { .. } => "authorize_device_v1",
            Self::Revoke { .. } => "revoke_device_v1",
        }
    }
    fn identity(&self) -> ([u8; 32], [u8; 32]) {
        match self {
            Self::Authorize {
                commitment,
                app_scope,
                ..
            }
            | Self::Revoke {
                commitment,
                app_scope,
            } => (*commitment, *app_scope),
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        let (commitment, scope) = self.identity();
        ensure!(
            commitment != [0; 32] && scope != [0; 32],
            "empty UCA commitment/scope"
        );
        let mut out = COMMAND_MAGIC.to_vec();
        out.push(if matches!(self, Self::Authorize { .. }) {
            1
        } else {
            2
        });
        out.extend_from_slice(&commitment);
        out.extend_from_slice(&scope);
        if let Self::Authorize {
            expires_at_height, ..
        } = self
        {
            ensure!(*expires_at_height > 0, "empty UCA expiry");
            out.extend_from_slice(&expires_at_height.to_be_bytes());
        }
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            (bytes.len() == 70 || bytes.len() == 78) && bytes.starts_with(COMMAND_MAGIC),
            "invalid UCA command framing"
        );
        let commitment = bytes[6..38].try_into()?;
        let app_scope = bytes[38..70].try_into()?;
        let result = match (bytes[5], bytes.len()) {
            (1, 78) => Self::Authorize {
                commitment,
                app_scope,
                expires_at_height: u64::from_be_bytes(bytes[70..78].try_into()?),
            },
            (2, 70) => Self::Revoke {
                commitment,
                app_scope,
            },
            _ => anyhow::bail!("unsupported UCA command/length"),
        };
        ensure!(result.encode()? == bytes, "noncanonical UCA command");
        Ok(result)
    }
}

/// Must originate from the node's verified immutable parent, not peer JSON.
pub struct UcaParentDomainV1 {
    pub chain_id: u64,
    pub genesis: [u8; 32],
    pub parent_height: u64,
    pub app_scope: [u8; 32],
}

/// Only a proposed account effect. Caller must join this with fees, canonical
/// receipt and exact parent checks in the SAME candidate before publication.
pub struct PreparedUcaAccountUpdateV1 {
    expected: NativeUcaAccountRecordV1,
    next: NativeUcaAccountRecordV1,
    transaction_id: [u8; 32],
}
impl PreparedUcaAccountUpdateV1 {
    pub fn next_record(&self) -> &NativeUcaAccountRecordV1 {
        &self.next
    }
    pub fn transaction_id(&self) -> &[u8; 32] {
        &self.transaction_id
    }
    /// Compare the whole record; a nonce-only CAS would miss root, epoch,
    /// status and grant changes. Does not mutate or publish anything.
    pub fn require_unchanged_parent(&self, current: &NativeUcaAccountRecordV1) -> Result<()> {
        ensure!(current == &self.expected, "native UCA parent changed");
        Ok(())
    }
}

pub fn prepare_uca_account_update_v1(
    parent: &NativeUcaAccountRecordV1,
    wire: &[u8],
    domain: &UcaParentDomainV1,
) -> Result<PreparedUcaAccountUpdateV1> {
    parent.validate(&parent.account.uca_id)?;
    let context = UcaDelegationContextV1 {
        account: &parent.account,
        delegation_epoch: parent.delegation_epoch,
        chain_id: domain.chain_id,
        genesis: domain.genesis,
        parent_height: domain.parent_height,
        next_nonce: parent.next_nonce,
        app_scope: domain.app_scope,
    };
    let verified = verify_uca_transaction_v4(wire, &context)?;
    let NovTxKindV1::Execute(execute) = &verified.transaction().kind else {
        anyhow::bail!("UCA execute required");
    };
    // No private/PQ execution claim is silently satisfied by this public,
    // Ed25519-only administrative reducer.
    ensure!(
        execute.execution_mode == novovm_protocol::NovExecutionModeV1::Standard
            && execute.privacy_mode == novovm_protocol::NovPrivacyModeV1::Public
            && execute.verification_mode == novovm_protocol::NovVerificationModeV1::Standard,
        "unsupported UCA execution/privacy/verification mode"
    );
    let command = UcaDeviceCommandV1::decode(&execute.args)?;
    let (commitment, scope) = command.identity();
    ensure!(
        execute.method == command.method() && scope == domain.app_scope,
        "UCA action/scope mismatch"
    );
    let height = domain
        .parent_height
        .checked_add(1)
        .context("UCA height exhausted")?;
    let id: String = commitment.iter().map(|b| format!("{b:02x}")).collect();
    let mut next = parent.clone();
    // Tombstones remain until expiry so old credentials cannot be resurrected
    // by a revoke/re-authorize pair using the same commitment.
    next.authorizations
        .retain(|_, grant| grant.expires_at_height > height);
    match command {
        UcaDeviceCommandV1::Authorize {
            expires_at_height, ..
        } => {
            ensure!(
                expires_at_height > height
                    && expires_at_height - height <= MAX_AUTHORIZATION_LIFETIME_BLOCKS_V1,
                "UCA grant lifetime out of bounds"
            );
            ensure!(
                !next.authorizations.contains_key(&id),
                "UCA commitment already used within its lease"
            );
            ensure!(
                next.authorizations.len() < MAX_AUTHORIZATIONS_V1,
                "native UCA authorization capacity exceeded"
            );
            next.authorizations.insert(
                id,
                UcaAuthorizationCommitmentV1 {
                    app_scope: scope,
                    issued_at_height: height,
                    expires_at_height,
                    revoked: false,
                },
            );
        }
        UcaDeviceCommandV1::Revoke { .. } => {
            let grant = next
                .authorizations
                .get_mut(&id)
                .context("UCA grant unavailable")?;
            ensure!(
                grant.app_scope == scope && !grant.revoked,
                "UCA grant revoked or outside application scope"
            );
            grant.revoked = true;
        }
    }
    next.next_nonce = parent
        .next_nonce
        .checked_add(1)
        .context("UCA nonce exhausted")?;
    next.validate(&parent.account.uca_id)?;
    Ok(PreparedUcaAccountUpdateV1 {
        expected: parent.clone(),
        next,
        transaction_id: *verified.transaction_id(),
    })
}

#[cfg(test)]
#[path = "uca_state_tests.rs"]
mod tests;
