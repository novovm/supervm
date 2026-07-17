use crate::product_overlay::peer_id_from_ed25519_public_key_v1;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    io,
    net::{SocketAddr, UdpSocket},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub const PRODUCT_NAT_PROTOCOL_VERSION_V1: u16 = 1;
const PRODUCT_NAT_OBSERVED_DOMAIN_V1: &[u8] = b"novovm-product-nat-observed-v1";
const PRODUCT_NAT_PUNCH_DOMAIN_V1: &[u8] = b"novovm-product-nat-punch-v1";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProductNatErrorV1 {
    #[error("unsupported NAT protocol version: {0}")]
    UnsupportedVersion(u16),
    #[error("NAT probe is outside its validity window")]
    Expired,
    #[error("NAT peer identity does not match its public key")]
    IdentityMismatch,
    #[error("NAT signature is invalid")]
    InvalidSignature,
    #[error("NAT nonce does not match its request")]
    NonceMismatch,
    #[error("NAT target peer id does not match")]
    TargetMismatch,
    #[error("NAT packet is not a valid expected message")]
    UnexpectedPacket,
    #[error("NAT datagram I/O failed: {0}")]
    Io(String),
    #[error("NAT datagram encoding failed: {0}")]
    Encoding(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedObservedEndpointProbeV1 {
    pub version: u16,
    pub probe_nonce: [u8; 16],
    pub requester_peer_id: String,
    pub requester_public_key: [u8; 32],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedObservedEndpointAckV1 {
    pub version: u16,
    pub probe_nonce: [u8; 16],
    pub requester_peer_id: String,
    pub observer_peer_id: String,
    pub observer_public_key: [u8; 32],
    pub observed_endpoint: String,
    pub observed_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedNatPunchRequestV1 {
    pub version: u16,
    pub punch_nonce: [u8; 16],
    pub source_peer_id: String,
    pub source_public_key: [u8; 32],
    pub target_peer_id: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedNatPunchAckV1 {
    pub version: u16,
    pub punch_nonce: [u8; 16],
    pub source_peer_id: String,
    pub target_peer_id: String,
    pub target_public_key: [u8; 32],
    pub observed_source_endpoint: String,
    pub observed_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", rename_all = "snake_case")]
pub enum NatDatagramV1 {
    ObservedProbe(SignedObservedEndpointProbeV1),
    ObservedAck(SignedObservedEndpointAckV1),
    PunchRequest(SignedNatPunchRequestV1),
    PunchAck(SignedNatPunchAckV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NatDiagnosisV1 {
    PunchedDirect,
    UdpReachabilityBlockedOrAckReturnFailed,
    InvalidAckRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NatSelectedPathV1 {
    PunchedDirect,
    RelayNovoRudp,
    QueueFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NatPunchAttemptV1 {
    pub diagnosis: NatDiagnosisV1,
    pub selected_path_after_punch: NatSelectedPathV1,
    pub relay_fallback_selected: bool,
    pub fallback_reason: Option<String>,
    pub ack_valid: bool,
}

pub fn build_observed_endpoint_probe_v1(
    identity: &SigningKey,
    now_ms: u64,
    ttl_ms: u64,
) -> SignedObservedEndpointProbeV1 {
    let requester_public_key = identity.verifying_key().to_bytes();
    let mut probe_nonce = [0u8; 16];
    OsRng.fill_bytes(&mut probe_nonce);
    let mut probe = SignedObservedEndpointProbeV1 {
        version: PRODUCT_NAT_PROTOCOL_VERSION_V1,
        probe_nonce,
        requester_peer_id: peer_id_from_ed25519_public_key_v1(&requester_public_key),
        requester_public_key,
        issued_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(ttl_ms.max(1)),
        signature: Vec::new(),
    };
    probe.signature = identity
        .sign(&observed_probe_bytes_v1(&probe))
        .to_bytes()
        .to_vec();
    probe
}

pub fn handle_observed_endpoint_probe_v1(
    observer_identity: &SigningKey,
    probe: &SignedObservedEndpointProbeV1,
    observed_source: SocketAddr,
    now_ms: u64,
    ttl_ms: u64,
) -> Result<SignedObservedEndpointAckV1, ProductNatErrorV1> {
    validate_observed_probe_v1(probe, now_ms)?;
    let observer_public_key = observer_identity.verifying_key().to_bytes();
    let mut ack = SignedObservedEndpointAckV1 {
        version: PRODUCT_NAT_PROTOCOL_VERSION_V1,
        probe_nonce: probe.probe_nonce,
        requester_peer_id: probe.requester_peer_id.clone(),
        observer_peer_id: peer_id_from_ed25519_public_key_v1(&observer_public_key),
        observer_public_key,
        observed_endpoint: observed_source.to_string(),
        observed_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(ttl_ms.max(1)),
        signature: Vec::new(),
    };
    ack.signature = observer_identity
        .sign(&observed_ack_bytes_v1(&ack))
        .to_bytes()
        .to_vec();
    Ok(ack)
}

pub fn request_observed_endpoint_v1(
    socket: &UdpSocket,
    observer_addr: SocketAddr,
    identity: &SigningKey,
    expected_observer_peer_id: &str,
    timeout: Duration,
) -> Result<SignedObservedEndpointAckV1, ProductNatErrorV1> {
    let now_ms = now_ms_v1();
    let probe =
        build_observed_endpoint_probe_v1(identity, now_ms, timeout.as_millis() as u64 + 5_000);
    send_datagram_v1(
        socket,
        observer_addr,
        &NatDatagramV1::ObservedProbe(probe.clone()),
    )?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(io_error_v1)?;
    let (packet, _) = receive_datagram_v1(socket)?;
    let NatDatagramV1::ObservedAck(ack) = packet else {
        return Err(ProductNatErrorV1::UnexpectedPacket);
    };
    validate_observed_endpoint_ack_v1(&ack, &probe, expected_observer_peer_id, now_ms_v1())?;
    Ok(ack)
}

pub fn serve_observed_endpoint_once_v1(
    socket: &UdpSocket,
    observer_identity: &SigningKey,
    response_ttl_ms: u64,
) -> Result<(), ProductNatErrorV1> {
    let (packet, source) = receive_datagram_v1(socket)?;
    let NatDatagramV1::ObservedProbe(probe) = packet else {
        return Err(ProductNatErrorV1::UnexpectedPacket);
    };
    let ack = handle_observed_endpoint_probe_v1(
        observer_identity,
        &probe,
        source,
        now_ms_v1(),
        response_ttl_ms,
    )?;
    send_datagram_v1(socket, source, &NatDatagramV1::ObservedAck(ack))
}

pub fn build_nat_punch_request_v1(
    identity: &SigningKey,
    target_peer_id: impl Into<String>,
    now_ms: u64,
    ttl_ms: u64,
) -> SignedNatPunchRequestV1 {
    let source_public_key = identity.verifying_key().to_bytes();
    let mut punch_nonce = [0u8; 16];
    OsRng.fill_bytes(&mut punch_nonce);
    let mut request = SignedNatPunchRequestV1 {
        version: PRODUCT_NAT_PROTOCOL_VERSION_V1,
        punch_nonce,
        source_peer_id: peer_id_from_ed25519_public_key_v1(&source_public_key),
        source_public_key,
        target_peer_id: target_peer_id.into(),
        issued_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(ttl_ms.max(1)),
        signature: Vec::new(),
    };
    request.signature = identity
        .sign(&punch_request_bytes_v1(&request))
        .to_bytes()
        .to_vec();
    request
}

pub fn handle_nat_punch_request_v1(
    target_identity: &SigningKey,
    request: &SignedNatPunchRequestV1,
    observed_source: SocketAddr,
    now_ms: u64,
    ttl_ms: u64,
) -> Result<SignedNatPunchAckV1, ProductNatErrorV1> {
    validate_punch_request_v1(request, now_ms)?;
    let target_public_key = target_identity.verifying_key().to_bytes();
    let target_peer_id = peer_id_from_ed25519_public_key_v1(&target_public_key);
    if request.target_peer_id != target_peer_id {
        return Err(ProductNatErrorV1::TargetMismatch);
    }
    let mut ack = SignedNatPunchAckV1 {
        version: PRODUCT_NAT_PROTOCOL_VERSION_V1,
        punch_nonce: request.punch_nonce,
        source_peer_id: request.source_peer_id.clone(),
        target_peer_id,
        target_public_key,
        observed_source_endpoint: observed_source.to_string(),
        observed_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(ttl_ms.max(1)),
        signature: Vec::new(),
    };
    ack.signature = target_identity
        .sign(&punch_ack_bytes_v1(&ack))
        .to_bytes()
        .to_vec();
    Ok(ack)
}

pub fn serve_nat_punch_once_v1(
    socket: &UdpSocket,
    target_identity: &SigningKey,
    response_ttl_ms: u64,
) -> Result<(), ProductNatErrorV1> {
    let (packet, source) = receive_datagram_v1(socket)?;
    let NatDatagramV1::PunchRequest(request) = packet else {
        return Err(ProductNatErrorV1::UnexpectedPacket);
    };
    let ack = handle_nat_punch_request_v1(
        target_identity,
        &request,
        source,
        now_ms_v1(),
        response_ttl_ms,
    )?;
    send_datagram_v1(socket, source, &NatDatagramV1::PunchAck(ack))
}

pub fn attempt_signed_nat_punch_v1(
    socket: &UdpSocket,
    target_addr: SocketAddr,
    source_identity: &SigningKey,
    expected_target_peer_id: &str,
    timeout: Duration,
    relay_candidate_available: bool,
) -> NatPunchAttemptV1 {
    let request = build_nat_punch_request_v1(
        source_identity,
        expected_target_peer_id,
        now_ms_v1(),
        timeout.as_millis() as u64 + 5_000,
    );
    let result = (|| {
        send_datagram_v1(
            socket,
            target_addr,
            &NatDatagramV1::PunchRequest(request.clone()),
        )?;
        socket
            .set_read_timeout(Some(timeout))
            .map_err(io_error_v1)?;
        let (packet, _) = receive_datagram_v1(socket)?;
        let NatDatagramV1::PunchAck(ack) = packet else {
            return Err(ProductNatErrorV1::UnexpectedPacket);
        };
        validate_nat_punch_ack_v1(&ack, &request, expected_target_peer_id, now_ms_v1())?;
        Ok(ack)
    })();
    match result {
        Ok(_) => NatPunchAttemptV1 {
            diagnosis: NatDiagnosisV1::PunchedDirect,
            selected_path_after_punch: NatSelectedPathV1::PunchedDirect,
            relay_fallback_selected: false,
            fallback_reason: None,
            ack_valid: true,
        },
        Err(
            ProductNatErrorV1::InvalidSignature
            | ProductNatErrorV1::IdentityMismatch
            | ProductNatErrorV1::NonceMismatch
            | ProductNatErrorV1::TargetMismatch,
        ) => fallback_after_nat_failure_v1(
            NatDiagnosisV1::InvalidAckRejected,
            relay_candidate_available,
        ),
        Err(_) => fallback_after_nat_failure_v1(
            NatDiagnosisV1::UdpReachabilityBlockedOrAckReturnFailed,
            relay_candidate_available,
        ),
    }
}

pub fn fallback_after_nat_failure_v1(
    diagnosis: NatDiagnosisV1,
    relay_candidate_available: bool,
) -> NatPunchAttemptV1 {
    NatPunchAttemptV1 {
        diagnosis,
        selected_path_after_punch: if relay_candidate_available {
            NatSelectedPathV1::RelayNovoRudp
        } else {
            NatSelectedPathV1::QueueFallback
        },
        relay_fallback_selected: relay_candidate_available,
        fallback_reason: Some(if relay_candidate_available {
            "NatPunchFailed".into()
        } else {
            "NoReachableRelayCandidate".into()
        }),
        ack_valid: false,
    }
}

pub fn validate_observed_endpoint_ack_v1(
    ack: &SignedObservedEndpointAckV1,
    probe: &SignedObservedEndpointProbeV1,
    expected_observer_peer_id: &str,
    now_ms: u64,
) -> Result<(), ProductNatErrorV1> {
    if ack.version != PRODUCT_NAT_PROTOCOL_VERSION_V1 {
        return Err(ProductNatErrorV1::UnsupportedVersion(ack.version));
    }
    if ack.expires_at_ms <= now_ms || ack.observed_at_ms > now_ms {
        return Err(ProductNatErrorV1::Expired);
    }
    if ack.probe_nonce != probe.probe_nonce {
        return Err(ProductNatErrorV1::NonceMismatch);
    }
    if ack.requester_peer_id != probe.requester_peer_id
        || ack.observer_peer_id != expected_observer_peer_id
    {
        return Err(ProductNatErrorV1::TargetMismatch);
    }
    verify_identity_signature_v1(
        &ack.observer_peer_id,
        &ack.observer_public_key,
        &observed_ack_bytes_v1(ack),
        &ack.signature,
    )
}

pub fn validate_nat_punch_ack_v1(
    ack: &SignedNatPunchAckV1,
    request: &SignedNatPunchRequestV1,
    expected_target_peer_id: &str,
    now_ms: u64,
) -> Result<(), ProductNatErrorV1> {
    if ack.version != PRODUCT_NAT_PROTOCOL_VERSION_V1 {
        return Err(ProductNatErrorV1::UnsupportedVersion(ack.version));
    }
    if ack.expires_at_ms <= now_ms || ack.observed_at_ms > now_ms {
        return Err(ProductNatErrorV1::Expired);
    }
    if ack.punch_nonce != request.punch_nonce {
        return Err(ProductNatErrorV1::NonceMismatch);
    }
    if ack.source_peer_id != request.source_peer_id || ack.target_peer_id != expected_target_peer_id
    {
        return Err(ProductNatErrorV1::TargetMismatch);
    }
    verify_identity_signature_v1(
        &ack.target_peer_id,
        &ack.target_public_key,
        &punch_ack_bytes_v1(ack),
        &ack.signature,
    )
}

fn validate_observed_probe_v1(
    probe: &SignedObservedEndpointProbeV1,
    now_ms: u64,
) -> Result<(), ProductNatErrorV1> {
    if probe.version != PRODUCT_NAT_PROTOCOL_VERSION_V1 {
        return Err(ProductNatErrorV1::UnsupportedVersion(probe.version));
    }
    if probe.expires_at_ms <= now_ms || probe.issued_at_ms > now_ms {
        return Err(ProductNatErrorV1::Expired);
    }
    verify_identity_signature_v1(
        &probe.requester_peer_id,
        &probe.requester_public_key,
        &observed_probe_bytes_v1(probe),
        &probe.signature,
    )
}

fn validate_punch_request_v1(
    request: &SignedNatPunchRequestV1,
    now_ms: u64,
) -> Result<(), ProductNatErrorV1> {
    if request.version != PRODUCT_NAT_PROTOCOL_VERSION_V1 {
        return Err(ProductNatErrorV1::UnsupportedVersion(request.version));
    }
    if request.expires_at_ms <= now_ms || request.issued_at_ms > now_ms {
        return Err(ProductNatErrorV1::Expired);
    }
    verify_identity_signature_v1(
        &request.source_peer_id,
        &request.source_public_key,
        &punch_request_bytes_v1(request),
        &request.signature,
    )
}

fn verify_identity_signature_v1(
    peer_id: &str,
    public_key: &[u8; 32],
    bytes: &[u8],
    signature: &[u8],
) -> Result<(), ProductNatErrorV1> {
    if peer_id != peer_id_from_ed25519_public_key_v1(public_key) {
        return Err(ProductNatErrorV1::IdentityMismatch);
    }
    let verifying_key =
        VerifyingKey::from_bytes(public_key).map_err(|_| ProductNatErrorV1::IdentityMismatch)?;
    let signature =
        Signature::from_slice(signature).map_err(|_| ProductNatErrorV1::InvalidSignature)?;
    verifying_key
        .verify(bytes, &signature)
        .map_err(|_| ProductNatErrorV1::InvalidSignature)
}

fn send_datagram_v1(
    socket: &UdpSocket,
    target: SocketAddr,
    message: &NatDatagramV1,
) -> Result<(), ProductNatErrorV1> {
    let bytes = serde_json::to_vec(message)
        .map_err(|error| ProductNatErrorV1::Encoding(error.to_string()))?;
    socket.send_to(&bytes, target).map_err(io_error_v1)?;
    Ok(())
}

fn receive_datagram_v1(
    socket: &UdpSocket,
) -> Result<(NatDatagramV1, SocketAddr), ProductNatErrorV1> {
    let mut bytes = vec![0u8; 16 * 1024];
    let (length, source) = socket.recv_from(&mut bytes).map_err(io_error_v1)?;
    let message = serde_json::from_slice(&bytes[..length])
        .map_err(|error| ProductNatErrorV1::Encoding(error.to_string()))?;
    Ok((message, source))
}

fn observed_probe_bytes_v1(probe: &SignedObservedEndpointProbeV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_field_v1(&mut bytes, PRODUCT_NAT_OBSERVED_DOMAIN_V1);
    append_field_v1(&mut bytes, &probe.version.to_be_bytes());
    append_field_v1(&mut bytes, &probe.probe_nonce);
    append_field_v1(&mut bytes, probe.requester_peer_id.as_bytes());
    append_field_v1(&mut bytes, &probe.requester_public_key);
    append_field_v1(&mut bytes, &probe.issued_at_ms.to_be_bytes());
    append_field_v1(&mut bytes, &probe.expires_at_ms.to_be_bytes());
    bytes
}

fn observed_ack_bytes_v1(ack: &SignedObservedEndpointAckV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_field_v1(&mut bytes, PRODUCT_NAT_OBSERVED_DOMAIN_V1);
    append_field_v1(&mut bytes, &ack.version.to_be_bytes());
    append_field_v1(&mut bytes, &ack.probe_nonce);
    append_field_v1(&mut bytes, ack.requester_peer_id.as_bytes());
    append_field_v1(&mut bytes, ack.observer_peer_id.as_bytes());
    append_field_v1(&mut bytes, &ack.observer_public_key);
    append_field_v1(&mut bytes, ack.observed_endpoint.as_bytes());
    append_field_v1(&mut bytes, &ack.observed_at_ms.to_be_bytes());
    append_field_v1(&mut bytes, &ack.expires_at_ms.to_be_bytes());
    bytes
}

fn punch_request_bytes_v1(request: &SignedNatPunchRequestV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_field_v1(&mut bytes, PRODUCT_NAT_PUNCH_DOMAIN_V1);
    append_field_v1(&mut bytes, &request.version.to_be_bytes());
    append_field_v1(&mut bytes, &request.punch_nonce);
    append_field_v1(&mut bytes, request.source_peer_id.as_bytes());
    append_field_v1(&mut bytes, &request.source_public_key);
    append_field_v1(&mut bytes, request.target_peer_id.as_bytes());
    append_field_v1(&mut bytes, &request.issued_at_ms.to_be_bytes());
    append_field_v1(&mut bytes, &request.expires_at_ms.to_be_bytes());
    bytes
}

fn punch_ack_bytes_v1(ack: &SignedNatPunchAckV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_field_v1(&mut bytes, PRODUCT_NAT_PUNCH_DOMAIN_V1);
    append_field_v1(&mut bytes, &ack.version.to_be_bytes());
    append_field_v1(&mut bytes, &ack.punch_nonce);
    append_field_v1(&mut bytes, ack.source_peer_id.as_bytes());
    append_field_v1(&mut bytes, ack.target_peer_id.as_bytes());
    append_field_v1(&mut bytes, &ack.target_public_key);
    append_field_v1(&mut bytes, ack.observed_source_endpoint.as_bytes());
    append_field_v1(&mut bytes, &ack.observed_at_ms.to_be_bytes());
    append_field_v1(&mut bytes, &ack.expires_at_ms.to_be_bytes());
    bytes
}

fn append_field_v1(destination: &mut Vec<u8>, value: &[u8]) {
    destination.extend_from_slice(&(value.len() as u32).to_be_bytes());
    destination.extend_from_slice(value);
}

fn io_error_v1(error: io::Error) -> ProductNatErrorV1 {
    ProductNatErrorV1::Io(error.to_string())
}

fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn signed_observed_endpoint_and_cooperative_punch_use_real_udp_sockets() {
        let observer_identity = SigningKey::from_bytes(&[121; 32]);
        let requester_identity = SigningKey::from_bytes(&[122; 32]);
        let observer = UdpSocket::bind("127.0.0.1:0").unwrap();
        let observer_addr = observer.local_addr().unwrap();
        let observer_thread = thread::spawn(move || {
            serve_observed_endpoint_once_v1(&observer, &observer_identity, 5_000)
        });
        let requester = UdpSocket::bind("127.0.0.1:0").unwrap();
        let observer_peer_id = peer_id_from_ed25519_public_key_v1(
            &SigningKey::from_bytes(&[121; 32])
                .verifying_key()
                .to_bytes(),
        );
        let observed = request_observed_endpoint_v1(
            &requester,
            observer_addr,
            &requester_identity,
            &observer_peer_id,
            Duration::from_secs(1),
        )
        .unwrap();
        observer_thread.join().unwrap().unwrap();
        assert_eq!(
            observed.requester_peer_id,
            peer_id_from_ed25519_public_key_v1(&requester_identity.verifying_key().to_bytes())
        );
        assert!(observed.observed_endpoint.starts_with("127.0.0.1:"));

        let target_identity = SigningKey::from_bytes(&[123; 32]);
        let target = UdpSocket::bind("127.0.0.1:0").unwrap();
        let target_addr = target.local_addr().unwrap();
        let target_thread =
            thread::spawn(move || serve_nat_punch_once_v1(&target, &target_identity, 5_000));
        let target_peer_id = peer_id_from_ed25519_public_key_v1(
            &SigningKey::from_bytes(&[123; 32])
                .verifying_key()
                .to_bytes(),
        );
        let punch = attempt_signed_nat_punch_v1(
            &requester,
            target_addr,
            &requester_identity,
            &target_peer_id,
            Duration::from_secs(1),
            true,
        );
        target_thread.join().unwrap().unwrap();
        assert_eq!(punch.diagnosis, NatDiagnosisV1::PunchedDirect);
        assert_eq!(
            punch.selected_path_after_punch,
            NatSelectedPathV1::PunchedDirect
        );
    }

    #[test]
    fn invalid_ack_and_timeout_never_promote_direct_and_fallback_is_deterministic() {
        let source = SigningKey::from_bytes(&[124; 32]);
        let target = SigningKey::from_bytes(&[125; 32]);
        let expected_target =
            peer_id_from_ed25519_public_key_v1(&target.verifying_key().to_bytes());
        let request = build_nat_punch_request_v1(&source, &expected_target, 1_000, 5_000);
        let mut ack = handle_nat_punch_request_v1(
            &target,
            &request,
            "127.0.0.1:1000".parse().unwrap(),
            1_001,
            5_000,
        )
        .unwrap();
        ack.punch_nonce = [0; 16];
        assert_eq!(
            validate_nat_punch_ack_v1(&ack, &request, &expected_target, 1_002),
            Err(ProductNatErrorV1::NonceMismatch)
        );
        let relay = fallback_after_nat_failure_v1(
            NatDiagnosisV1::UdpReachabilityBlockedOrAckReturnFailed,
            true,
        );
        assert_eq!(
            relay.selected_path_after_punch,
            NatSelectedPathV1::RelayNovoRudp
        );
        let queue = fallback_after_nat_failure_v1(
            NatDiagnosisV1::UdpReachabilityBlockedOrAckReturnFailed,
            false,
        );
        assert_eq!(
            queue.selected_path_after_punch,
            NatSelectedPathV1::QueueFallback
        );
    }
}
