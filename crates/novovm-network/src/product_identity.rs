use crate::product_overlay::{peer_id_from_ed25519_public_key_v1, AuthenticatedPeerV1};
use ed25519_dalek::{
    Signature as Ed25519Signature, Signer as Ed25519Signer, SigningKey,
    Verifier as Ed25519Verifier, VerifyingKey as Ed25519VerifyingKey,
};
use k256::ecdsa::{
    Signature as Secp256k1Signature, SigningKey as Secp256k1SigningKey,
    VerifyingKey as Secp256k1VerifyingKey,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use thiserror::Error;

pub const PRODUCT_NETWORK_IDENTITY_VERSION_V1: u16 = 1;
pub const PRODUCT_NETWORK_DEVICE_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"novovm-product-network-device-authorization-v1";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProductIdentityErrorV1 {
    #[error("unsupported network identity version: {0}")]
    UnsupportedVersion(u16),
    #[error("network device authorization is outside its validity window")]
    AuthorizationExpired,
    #[error("network device authorization does not match the device peer id")]
    DeviceIdentityMismatch,
    #[error("network device authorization signature is invalid")]
    InvalidSignature,
    #[error("network authority key material is invalid")]
    InvalidAuthorityKey,
    #[error("network device authorization is revoked")]
    AuthorizationRevoked,
    #[error("network device authorization status is unavailable")]
    AuthorizationStatusUnavailable,
    #[error("network device authorization lacks required capability: {0}")]
    MissingCapability(String),
}

/// The UCA primary key is deliberately not a network identity key. A UCA implementation
/// creates a network-only authority subkey and uses it to authorize individual device keys.
/// The corresponding UCA account identifier is never included in this record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkAuthorityKeyAlgorithmV1 {
    Ed25519,
    Secp256k1,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkDeviceCapabilityV1 {
    RelayOperator,
    BootstrapPublisher,
    DirectoryProvider,
    MeteringReporter,
}

/// A privacy-preserving authorization from a UCA-derived network authority to a device key.
/// `authority_subject_commitment` is an opaque commitment chosen by the account layer; it must
/// not be a raw UCA ID or a primary-account public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkDeviceKeyAuthorizationV1 {
    pub version: u16,
    pub authorization_id: String,
    pub authority_subject_commitment: [u8; 32],
    pub authority_key_algorithm: NetworkAuthorityKeyAlgorithmV1,
    pub authority_public_key: Vec<u8>,
    pub device_peer_id: String,
    pub device_public_key: [u8; 32],
    pub capabilities: BTreeSet<NetworkDeviceCapabilityV1>,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub delegation_epoch: u64,
    pub signature: Vec<u8>,
}

/// The account layer owns revocation and recovery. The network only asks this narrow interface
/// when a policy requires a UCA-backed capability; anonymous transport never calls it.
pub trait NetworkDeviceAuthorizationStatusV1: Send + Sync {
    fn is_authorization_active(
        &self,
        authority_subject_commitment: &[u8; 32],
        authorization_id: &str,
        delegation_epoch: u64,
        now_ms: u64,
    ) -> Result<bool, ProductIdentityErrorV1>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedNetworkPeerV1 {
    peer_id: String,
    capabilities: BTreeSet<NetworkDeviceCapabilityV1>,
    authorization_id: String,
    authorization_expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkDeviceAuthorizationRequestV1 {
    pub authorization_id: String,
    pub authority_subject_commitment: [u8; 32],
    pub device_public_key: [u8; 32],
    pub capabilities: BTreeSet<NetworkDeviceCapabilityV1>,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub delegation_epoch: u64,
}

pub fn sign_network_device_authorization_ed25519_v1(
    authority_signing_key: &SigningKey,
    request: NetworkDeviceAuthorizationRequestV1,
) -> NetworkDeviceKeyAuthorizationV1 {
    let mut authorization = unsigned_authorization_v1(
        request,
        NetworkAuthorityKeyAlgorithmV1::Ed25519,
        authority_signing_key.verifying_key().to_bytes().to_vec(),
    );
    authorization.signature = authority_signing_key
        .sign(&network_device_authorization_signing_bytes_v1(
            &authorization,
        ))
        .to_bytes()
        .to_vec();
    authorization
}

pub fn sign_network_device_authorization_secp256k1_v1(
    authority_signing_key: &Secp256k1SigningKey,
    request: NetworkDeviceAuthorizationRequestV1,
) -> NetworkDeviceKeyAuthorizationV1 {
    let mut authorization = unsigned_authorization_v1(
        request,
        NetworkAuthorityKeyAlgorithmV1::Secp256k1,
        authority_signing_key
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec(),
    );
    let signature: Secp256k1Signature = authority_signing_key.sign(
        &network_device_authorization_signing_bytes_v1(&authorization),
    );
    authorization.signature = signature.to_bytes().to_vec();
    authorization
}

pub fn validate_network_device_authorization_v1(
    authorization: &NetworkDeviceKeyAuthorizationV1,
    now_ms: u64,
    status: Option<&dyn NetworkDeviceAuthorizationStatusV1>,
) -> Result<(), ProductIdentityErrorV1> {
    if authorization.version != PRODUCT_NETWORK_IDENTITY_VERSION_V1 {
        return Err(ProductIdentityErrorV1::UnsupportedVersion(
            authorization.version,
        ));
    }
    if authorization.authorization_id.is_empty()
        || authorization.expires_at_ms <= now_ms
        || authorization.issued_at_ms > now_ms
    {
        return Err(ProductIdentityErrorV1::AuthorizationExpired);
    }
    if authorization.device_peer_id
        != peer_id_from_ed25519_public_key_v1(&authorization.device_public_key)
    {
        return Err(ProductIdentityErrorV1::DeviceIdentityMismatch);
    }

    let message = network_device_authorization_signing_bytes_v1(authorization);
    match authorization.authority_key_algorithm {
        NetworkAuthorityKeyAlgorithmV1::Ed25519 => {
            let public_key: [u8; 32] = authorization
                .authority_public_key
                .as_slice()
                .try_into()
                .map_err(|_| ProductIdentityErrorV1::InvalidAuthorityKey)?;
            let verifying_key = Ed25519VerifyingKey::from_bytes(&public_key)
                .map_err(|_| ProductIdentityErrorV1::InvalidAuthorityKey)?;
            let signature = Ed25519Signature::from_slice(&authorization.signature)
                .map_err(|_| ProductIdentityErrorV1::InvalidSignature)?;
            verifying_key
                .verify(&message, &signature)
                .map_err(|_| ProductIdentityErrorV1::InvalidSignature)?;
        }
        NetworkAuthorityKeyAlgorithmV1::Secp256k1 => {
            let verifying_key =
                Secp256k1VerifyingKey::from_sec1_bytes(&authorization.authority_public_key)
                    .map_err(|_| ProductIdentityErrorV1::InvalidAuthorityKey)?;
            let signature = Secp256k1Signature::from_slice(&authorization.signature)
                .map_err(|_| ProductIdentityErrorV1::InvalidSignature)?;
            verifying_key
                .verify(&message, &signature)
                .map_err(|_| ProductIdentityErrorV1::InvalidSignature)?;
        }
    }

    let status = status.ok_or(ProductIdentityErrorV1::AuthorizationStatusUnavailable)?;
    if !status.is_authorization_active(
        &authorization.authority_subject_commitment,
        &authorization.authorization_id,
        authorization.delegation_epoch,
        now_ms,
    )? {
        return Err(ProductIdentityErrorV1::AuthorizationRevoked);
    }
    Ok(())
}

pub fn authorize_authenticated_peer_v1(
    peer: &AuthenticatedPeerV1,
    authorization: &NetworkDeviceKeyAuthorizationV1,
    required_capability: NetworkDeviceCapabilityV1,
    now_ms: u64,
    status: &dyn NetworkDeviceAuthorizationStatusV1,
) -> Result<AuthorizedNetworkPeerV1, ProductIdentityErrorV1> {
    validate_network_device_authorization_v1(authorization, now_ms, Some(status))?;
    if peer.peer_id() != authorization.device_peer_id
        || peer.identity_public_key() != authorization.device_public_key
    {
        return Err(ProductIdentityErrorV1::DeviceIdentityMismatch);
    }
    if !authorization.capabilities.contains(&required_capability) {
        return Err(ProductIdentityErrorV1::MissingCapability(format!(
            "{required_capability:?}"
        )));
    }
    Ok(AuthorizedNetworkPeerV1 {
        peer_id: peer.peer_id().to_string(),
        capabilities: authorization.capabilities.clone(),
        authorization_id: authorization.authorization_id.clone(),
        authorization_expires_at_ms: authorization.expires_at_ms,
    })
}

impl AuthorizedNetworkPeerV1 {
    #[must_use]
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    #[must_use]
    pub fn has_capability(&self, capability: &NetworkDeviceCapabilityV1) -> bool {
        self.capabilities.contains(capability)
    }

    #[must_use]
    pub fn authorization_id(&self) -> &str {
        &self.authorization_id
    }

    #[must_use]
    pub fn authorization_expires_at_ms(&self) -> u64 {
        self.authorization_expires_at_ms
    }
}

fn unsigned_authorization_v1(
    request: NetworkDeviceAuthorizationRequestV1,
    authority_key_algorithm: NetworkAuthorityKeyAlgorithmV1,
    authority_public_key: Vec<u8>,
) -> NetworkDeviceKeyAuthorizationV1 {
    NetworkDeviceKeyAuthorizationV1 {
        version: PRODUCT_NETWORK_IDENTITY_VERSION_V1,
        authorization_id: request.authorization_id,
        authority_subject_commitment: request.authority_subject_commitment,
        authority_key_algorithm,
        authority_public_key,
        device_peer_id: peer_id_from_ed25519_public_key_v1(&request.device_public_key),
        device_public_key: request.device_public_key,
        capabilities: request.capabilities,
        issued_at_ms: request.issued_at_ms,
        expires_at_ms: request.expires_at_ms,
        delegation_epoch: request.delegation_epoch,
        signature: Vec::new(),
    }
}

fn network_device_authorization_signing_bytes_v1(
    authorization: &NetworkDeviceKeyAuthorizationV1,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_field_v1(&mut bytes, PRODUCT_NETWORK_DEVICE_AUTHORIZATION_DOMAIN_V1);
    append_field_v1(&mut bytes, &authorization.version.to_be_bytes());
    append_field_v1(&mut bytes, authorization.authorization_id.as_bytes());
    append_field_v1(&mut bytes, &authorization.authority_subject_commitment);
    append_field_v1(
        &mut bytes,
        match authorization.authority_key_algorithm {
            NetworkAuthorityKeyAlgorithmV1::Ed25519 => b"ed25519",
            NetworkAuthorityKeyAlgorithmV1::Secp256k1 => b"secp256k1",
        },
    );
    append_field_v1(&mut bytes, &authorization.authority_public_key);
    append_field_v1(&mut bytes, authorization.device_peer_id.as_bytes());
    append_field_v1(&mut bytes, &authorization.device_public_key);
    for capability in &authorization.capabilities {
        append_field_v1(&mut bytes, format!("{capability:?}").as_bytes());
    }
    append_field_v1(&mut bytes, &authorization.issued_at_ms.to_be_bytes());
    append_field_v1(&mut bytes, &authorization.expires_at_ms.to_be_bytes());
    append_field_v1(&mut bytes, &authorization.delegation_epoch.to_be_bytes());
    Sha256::digest(bytes).to_vec()
}

fn append_field_v1(destination: &mut Vec<u8>, field: &[u8]) {
    destination.extend_from_slice(&(field.len() as u32).to_be_bytes());
    destination.extend_from_slice(field);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_overlay::NodeHandshakeInitiatorV1;
    use ed25519_dalek::SigningKey;
    use k256::ecdsa::SigningKey as Secp256k1SigningKey;

    #[derive(Default)]
    struct Status {
        active: bool,
    }

    impl NetworkDeviceAuthorizationStatusV1 for Status {
        fn is_authorization_active(
            &self,
            _: &[u8; 32],
            _: &str,
            _: u64,
            _: u64,
        ) -> Result<bool, ProductIdentityErrorV1> {
            Ok(self.active)
        }
    }

    fn capabilities() -> BTreeSet<NetworkDeviceCapabilityV1> {
        BTreeSet::from([NetworkDeviceCapabilityV1::RelayOperator])
    }

    fn request(
        authorization_id: &str,
        authority_subject_commitment: [u8; 32],
        device_public_key: [u8; 32],
        issued_at_ms: u64,
        expires_at_ms: u64,
        delegation_epoch: u64,
    ) -> NetworkDeviceAuthorizationRequestV1 {
        NetworkDeviceAuthorizationRequestV1 {
            authorization_id: authorization_id.into(),
            authority_subject_commitment,
            device_public_key,
            capabilities: capabilities(),
            issued_at_ms,
            expires_at_ms,
            delegation_epoch,
        }
    }

    #[test]
    fn ed25519_uca_derived_authority_authorizes_device_without_account_disclosure() {
        let authority = SigningKey::from_bytes(&[7; 32]);
        let device = SigningKey::from_bytes(&[8; 32]);
        let authorization = sign_network_device_authorization_ed25519_v1(
            &authority,
            request(
                "device-auth-1",
                [42; 32],
                device.verifying_key().to_bytes(),
                1_000,
                2_000,
                3,
            ),
        );
        assert_eq!(authorization.authority_subject_commitment, [42; 32]);
        assert!(
            !network_device_authorization_signing_bytes_v1(&authorization)
                .windows(b"uca".len())
                .any(|window| window == b"uca")
        );
        assert!(validate_network_device_authorization_v1(
            &authorization,
            1_500,
            Some(&Status { active: true })
        )
        .is_ok());
        assert_eq!(
            validate_network_device_authorization_v1(
                &authorization,
                1_500,
                Some(&Status::default())
            ),
            Err(ProductIdentityErrorV1::AuthorizationRevoked)
        );
    }

    #[test]
    fn secp256k1_uca_derived_authority_is_verified() {
        let authority = Secp256k1SigningKey::from_bytes((&[3u8; 32]).into()).unwrap();
        let device = SigningKey::from_bytes(&[8; 32]);
        let authorization = sign_network_device_authorization_secp256k1_v1(
            &authority,
            request(
                "device-auth-secp",
                [43; 32],
                device.verifying_key().to_bytes(),
                1_000,
                2_000,
                4,
            ),
        );
        assert!(validate_network_device_authorization_v1(
            &authorization,
            1_500,
            Some(&Status { active: true })
        )
        .is_ok());
    }

    #[test]
    fn authorization_binds_to_authenticated_peer_and_capability() {
        let authority = SigningKey::from_bytes(&[7; 32]);
        let initiator = SigningKey::from_bytes(&[8; 32]);
        let responder = SigningKey::from_bytes(&[9; 32]);
        let responder_peer_id =
            peer_id_from_ed25519_public_key_v1(&responder.verifying_key().to_bytes());
        let offer = NodeHandshakeInitiatorV1::start(&initiator, responder_peer_id, 1_000, 2_000)
            .unwrap()
            .offer()
            .clone();
        let authorization = sign_network_device_authorization_ed25519_v1(
            &authority,
            request(
                "device-auth-2",
                [44; 32],
                initiator.verifying_key().to_bytes(),
                1_000,
                2_000,
                1,
            ),
        );
        let mut replay = crate::product_overlay::HandshakeReplayCacheV1::default();
        let response = crate::product_overlay::NodeHandshakeResponderV1::respond(
            &offer,
            &responder,
            1_500,
            1_000,
            &mut replay,
        )
        .unwrap();
        let authenticated = response.authenticated_remote().clone();
        let authorized = authorize_authenticated_peer_v1(
            &authenticated,
            &authorization,
            NetworkDeviceCapabilityV1::RelayOperator,
            1_500,
            &Status { active: true },
        )
        .unwrap();
        assert_eq!(authorized.peer_id(), authenticated.peer_id());
        assert!(authorized.has_capability(&NetworkDeviceCapabilityV1::RelayOperator));
    }

    #[test]
    fn tampering_expiry_and_missing_status_are_rejected() {
        let authority = SigningKey::from_bytes(&[7; 32]);
        let device = SigningKey::from_bytes(&[8; 32]);
        let mut authorization = sign_network_device_authorization_ed25519_v1(
            &authority,
            request(
                "device-auth-3",
                [45; 32],
                device.verifying_key().to_bytes(),
                1_000,
                2_000,
                1,
            ),
        );
        authorization.expires_at_ms = 3_000;
        assert_eq!(
            validate_network_device_authorization_v1(
                &authorization,
                1_500,
                Some(&Status { active: true })
            ),
            Err(ProductIdentityErrorV1::InvalidSignature)
        );
        let authorization = sign_network_device_authorization_ed25519_v1(
            &authority,
            request(
                "device-auth-4",
                [46; 32],
                device.verifying_key().to_bytes(),
                1_000,
                1_500,
                1,
            ),
        );
        assert_eq!(
            validate_network_device_authorization_v1(
                &authorization,
                1_500,
                Some(&Status { active: true })
            ),
            Err(ProductIdentityErrorV1::AuthorizationExpired)
        );
        assert_eq!(
            validate_network_device_authorization_v1(&authorization, 1_200, None),
            Err(ProductIdentityErrorV1::AuthorizationStatusUnavailable)
        );
    }
}
