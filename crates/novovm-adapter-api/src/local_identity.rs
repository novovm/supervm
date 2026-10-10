//! Local, unregistered root ownership and narrowly bound key authorization.
//!
//! A signature proves control of a locally trusted root key. It does not prove
//! an on-chain UCA account, balances, transaction rights, device attestation,
//! anonymity, post-quantum security, or a remotely observable revocation state.
//! The protected caller owns root pinning, request consumption, durable clock
//! and epoch monotonicity, revocation, and local-device/user authorization.
//! Verifying the same proof twice does not consume it; no storage lives here.
//!
//! Roots and grants stay local. They must not be sent to contacts, relays or
//! discovery services: their stable bindings can link otherwise separate keys.
//! Only Ed25519 is implemented. There is no algorithm fallback or generic sign
//! API, and neither proof nor verified authorization supports serialization.

use crate::{uca_key_binding::derive_primary_key_ref_from_binding_v1, unified_account::UcaKeyAlgo};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use zeroize::Zeroizing;

pub const MAX_LOCAL_AUTHORIZATION_LIFETIME_MS_V1: u64 = 86_400_000;
pub const MAX_LOCAL_AUTHORIZATION_REQUEST_AGE_MS_V1: u64 = 300_000;
const VERSION: u16 = 1;
const DOMAIN: &[u8] = b"novovm-local-key-authorization-ed25519-v1\0";
// domain + version + root/ref and five request fields + four u64 values.
const SIGNING_BYTES: usize = DOMAIN.len() + 2 + 7 * 32 + 4 * 8;

/// Fixed-size values supplied by the protected caller. `purpose` is a stable
/// application/use-domain commitment, and `scope` binds its complete context
/// and role. They are not interpreted as account IDs or wallet capabilities.
/// The caller must supply its actual owned subject key, not an untrusted key
/// passed through from UI input. Creating this request alone grants nothing.
#[derive(Clone, PartialEq, Eq)]
pub struct LocalKeyAuthorizationRequestV1 {
    pub purpose: [u8; 32],
    pub device_binding: [u8; 32],
    pub scope: [u8; 32],
    pub subject_public_key: [u8; 32],
    pub challenge: [u8; 32],
    pub created_at_ms: u64,
}

/// The seed is consumed and erased; the internal SigningKey is erased on Drop
/// by ed25519-dalek's explicitly enabled zeroize feature. No secret getter,
/// Clone, Debug, serialization, or arbitrary-message signature API is exposed.
pub struct LocalIdentitySignerV1 {
    signing_key: SigningKey,
    primary_key_ref: [u8; 32],
}

impl LocalIdentitySignerV1 {
    pub fn from_seed(seed: Zeroizing<[u8; 32]>) -> Result<Self> {
        ensure!(*seed != [0; 32], "local identity seed unavailable");
        let signing_key = SigningKey::from_bytes(&seed);
        let public_key = signing_key.verifying_key().to_bytes();
        validate_key(&public_key)?;
        Ok(Self {
            signing_key,
            primary_key_ref: primary_ref(&public_key)?,
        })
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub fn primary_key_ref(&self) -> [u8; 32] {
        self.primary_key_ref
    }

    /// Sign only this complete local authorization transcript. A caller must
    /// durably consume the request before exposing a usable authorization; a
    /// returned proof alone is neither activation nor persistence evidence.
    pub fn authorize(
        &self,
        request: &LocalKeyAuthorizationRequestV1,
        epoch: u64,
        now_ms: u64,
        expires_ms: u64,
    ) -> Result<LocalKeyAuthorizationV1> {
        let root_public_key = self.public_key();
        validate_request(request, &root_public_key)?;
        validate_window(request.created_at_ms, epoch, now_ms, expires_ms, now_ms)?;
        let mut proof = LocalKeyAuthorizationV1 {
            version: VERSION,
            root_public_key,
            primary_key_ref: self.primary_key_ref,
            request: request.clone(),
            epoch,
            issued_at_ms: now_ms,
            expires_at_ms: expires_ms,
            signature: [0; 64],
        };
        proof.signature = self.signing_key.sign(&proof.signing_bytes()).to_bytes();
        Ok(proof)
    }
}

/// An in-memory signed proof, not yet authorized by a current trusted owner.
/// No public constructor or mutable fields permit bypassing its fixed bounds.
#[derive(Clone)]
pub struct LocalKeyAuthorizationV1 {
    version: u16,
    root_public_key: [u8; 32],
    primary_key_ref: [u8; 32],
    request: LocalKeyAuthorizationRequestV1,
    epoch: u64,
    issued_at_ms: u64,
    expires_at_ms: u64,
    signature: [u8; 64],
}

impl LocalKeyAuthorizationV1 {
    pub fn request(&self) -> &LocalKeyAuthorizationRequestV1 {
        &self.request
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn issued_at_ms(&self) -> u64 {
        self.issued_at_ms
    }

    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }

    /// Trust comes from the protected caller's pinned root, exact expected
    /// request, and CURRENT active epoch. Never obtain these trust inputs from
    /// this proof itself. The owner must reject a revoked/inactive identity
    /// before calling and continuously recheck its own current state afterward.
    pub fn verify(
        &self,
        trusted_root: [u8; 32],
        expected: &LocalKeyAuthorizationRequestV1,
        trusted_epoch: u64,
        now_ms: u64,
    ) -> Result<VerifiedLocalKeyAuthorizationV1> {
        ensure!(
            self.version == VERSION,
            "unsupported local authorization version"
        );
        let root = validate_key(&trusted_root)?;
        validate_request(expected, &trusted_root)?;
        ensure!(
            self.root_public_key == trusted_root
                && self.primary_key_ref == primary_ref(&trusted_root)?,
            "local authorization root mismatch"
        );
        ensure!(
            self.request == *expected,
            "local authorization binding mismatch"
        );
        ensure!(
            trusted_epoch != 0 && self.epoch == trusted_epoch,
            "local authorization epoch mismatch"
        );
        validate_window(
            self.request.created_at_ms,
            self.epoch,
            self.issued_at_ms,
            self.expires_at_ms,
            now_ms,
        )?;
        root.verify_strict(
            &self.signing_bytes(),
            &Signature::from_bytes(&self.signature),
        )
        .context("invalid local authorization signature")?;
        Ok(VerifiedLocalKeyAuthorizationV1 {
            primary_key_ref: self.primary_key_ref,
            request: self.request.clone(),
            epoch: self.epoch,
            issued_at_ms: self.issued_at_ms,
            expires_at_ms: self.expires_at_ms,
        })
    }

    fn signing_bytes(&self) -> [u8; SIGNING_BYTES] {
        let mut bytes = [0; SIGNING_BYTES];
        let mut position = 0;
        let mut append = |value: &[u8]| {
            bytes[position..position + value.len()].copy_from_slice(value);
            position += value.len();
        };
        append(DOMAIN);
        append(&self.version.to_be_bytes());
        append(&self.root_public_key);
        append(&self.primary_key_ref);
        append(&self.request.purpose);
        append(&self.request.device_binding);
        append(&self.request.scope);
        append(&self.request.subject_public_key);
        append(&self.request.challenge);
        append(&self.request.created_at_ms.to_be_bytes());
        append(&self.epoch.to_be_bytes());
        append(&self.issued_at_ms.to_be_bytes());
        append(&self.expires_at_ms.to_be_bytes());
        debug_assert_eq!(position, SIGNING_BYTES);
        bytes
    }
}

/// Immutable verification result. It is not a network credential or a durable
/// revocation lease: the protected owner retains and rechecks the trust source.
#[derive(Clone)]
pub struct VerifiedLocalKeyAuthorizationV1 {
    primary_key_ref: [u8; 32],
    request: LocalKeyAuthorizationRequestV1,
    epoch: u64,
    issued_at_ms: u64,
    expires_at_ms: u64,
}

impl VerifiedLocalKeyAuthorizationV1 {
    pub fn request(&self) -> &LocalKeyAuthorizationRequestV1 {
        &self.request
    }

    pub fn subject_public_key(&self) -> [u8; 32] {
        self.request.subject_public_key
    }

    pub fn primary_key_ref(&self) -> [u8; 32] {
        self.primary_key_ref
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn issued_at_ms(&self) -> u64 {
        self.issued_at_ms
    }

    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }
}

fn primary_ref(public_key: &[u8; 32]) -> Result<[u8; 32]> {
    derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Ed25519, public_key)
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid local root reference length"))
}

fn validate_key(bytes: &[u8; 32]) -> Result<VerifyingKey> {
    ensure!(*bytes != [0; 32], "empty local authorization public key");
    let key = VerifyingKey::from_bytes(bytes).context("invalid local authorization public key")?;
    ensure!(!key.is_weak(), "weak local authorization public key");
    let point = key.to_edwards();
    ensure!(
        point.compress().to_bytes() == *bytes && point.is_torsion_free(),
        "noncanonical or non-prime-order local authorization public key"
    );
    Ok(key)
}

fn validate_request(request: &LocalKeyAuthorizationRequestV1, root: &[u8; 32]) -> Result<()> {
    ensure!(
        request.purpose != [0; 32]
            && request.device_binding != [0; 32]
            && request.scope != [0; 32]
            && request.challenge != [0; 32]
            && request.created_at_ms != 0,
        "empty local authorization binding"
    );
    validate_key(&request.subject_public_key)?;
    ensure!(
        request.subject_public_key != *root,
        "local root/subject key reuse"
    );
    Ok(())
}

fn validate_window(created: u64, epoch: u64, issued: u64, expires: u64, now: u64) -> Result<()> {
    ensure!(epoch != 0, "empty local authorization epoch");
    ensure!(
        created != 0 && created <= issued && issued <= now,
        "local authorization time rollback or future issuance"
    );
    ensure!(
        issued - created <= MAX_LOCAL_AUTHORIZATION_REQUEST_AGE_MS_V1,
        "stale local authorization request at issuance"
    );
    ensure!(
        expires > now && expires - issued <= MAX_LOCAL_AUTHORIZATION_LIFETIME_MS_V1,
        "local authorization expired or lifetime exceeded"
    );
    Ok(())
}

#[cfg(test)]
#[path = "local_identity_tests.rs"]
mod tests;
