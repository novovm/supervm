//! Compact projection of the EXISTING signed node record for the original
//! 4KiB discovery RPC. The expected signing key is supplied separately; each
//! signature binds the chain, publication purpose, endpoints and lease.
use crate::{
    product_directory::{
        validate_relay_record_v1, PeerSignedRelayRecordV1, RelayEndpointV1, RelayTransportV1,
        PRODUCT_DIRECTORY_VERSION_V1,
    },
    product_overlay::{
        peer_id_from_mldsa65_public_key_v1, TransportVerifier, MLDSA65_PUBLIC_KEY_BYTES,
        MLDSA65_SIGNATURE_BYTES, MLDSA65_SUITE,
    },
    product_service_lease::{
        split_chain_query_uri, IrohServiceEndpointV1, MAX_SERVICE_LEASE_MS,
        NATIVE_CHAIN_LOCATOR_PREFIX,
    },
};
use anyhow::{ensure, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;

const MAGIC_V1: &[u8; 8] = b"NVLOCR01";
const MAGIC_V2: &[u8; 8] = b"NVLOCR02";
const MAGIC_V3: &[u8; 8] = b"NVLOCR03";
const PUBLIC_READ_V1: &[u8; 8] = b"NVREAD01";
const V1_RELAY_BYTES_PER_MINUTE: u64 = 64 * 1024;
/// Approved native-node allowance. The compact profile must preserve this
/// signed field; changing the implicit V1 value would invalidate old records.
pub const NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE: u64 = 128 * 1024;
const HEADER: usize = 8 + 3 * 8 + 2;
type Pins = (u64, [u8; 32], [u8; 32]);

/// Read the sequence of a locator already validated by the original codec.
pub(super) fn signed_sequence(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(24..32)?.try_into().ok()?))
}

pub fn native_locator_id_v1(pins: Pins) -> String {
    locator_id(b"NOVONET/native-chain-locator/v1\0", pins)
}

/// A transport owner signs this domain when volunteering a discoverable read service.
pub fn public_read_locator_id_v1(pins: Pins) -> String {
    locator_id(b"NOVONET/public-read-locator/v1\0", pins)
}

fn locator_id(purpose: &[u8], pins: Pins) -> String {
    let mut hash = Sha256::new();
    hash.update(purpose);
    hash.update(pins.0.to_be_bytes());
    hash.update(pins.1);
    hash.update(pins.2);
    format!(
        "{NATIVE_CHAIN_LOCATOR_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(hash.finalize())
    )
}

/// Sign the real native query socket alongside the existing discovery carrier.
pub fn native_chain_query_endpoint(
    route: &IrohServiceEndpointV1,
    address: SocketAddr,
) -> Result<RelayEndpointV1> {
    let mut endpoint = route.chain_query_endpoint(address)?;
    endpoint.max_bytes_per_minute = NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE;
    Ok(endpoint)
}

/// Read the native route from a locator whose chain pins and signature were verified.
pub fn native_chain_query_address(record: &PeerSignedRelayRecordV1) -> Result<Option<SocketAddr>> {
    let [endpoint] = record.endpoints.as_slice() else {
        anyhow::bail!("native locator endpoint count");
    };
    ensure!(
        record.record_id.starts_with(NATIVE_CHAIN_LOCATOR_PREFIX)
            && endpoint.transport == RelayTransportV1::IrohRendezvousV1,
        "native query locator kind"
    );
    Ok(split_chain_query_uri(&endpoint.uri)?.1)
}

pub fn encode_node_locator_v1(record: &PeerSignedRelayRecordV1, pins: Pins) -> Result<Vec<u8>> {
    encode_locator(record, pins, LocatorPurpose::Node)
}

pub fn encode_public_read_locator_v1(
    record: &PeerSignedRelayRecordV1,
    pins: Pins,
) -> Result<Vec<u8>> {
    encode_locator(record, pins, LocatorPurpose::PublicRead)
}

#[derive(Clone, Copy)]
enum LocatorPurpose {
    Node,
    PublicRead,
}

impl LocatorPurpose {
    fn max_body(self) -> usize {
        match self {
            Self::Node => super::MAX_BODY,
            Self::PublicRead => super::MAX_PUBLIC_READ_BODY,
        }
    }

    fn record_id(self, pins: Pins) -> String {
        match self {
            Self::Node => native_locator_id_v1(pins),
            Self::PublicRead => public_read_locator_id_v1(pins),
        }
    }
}

fn encode_locator(
    record: &PeerSignedRelayRecordV1,
    pins: Pins,
    purpose: LocatorPurpose,
) -> Result<Vec<u8>> {
    ensure!(
        record.version == PRODUCT_DIRECTORY_VERSION_V1
            && record.suite == MLDSA65_SUITE
            && record.relay_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
            && record.signature.len() == MLDSA65_SIGNATURE_BYTES
            && record.relay_peer_id == peer_id_from_mldsa65_public_key_v1(&record.relay_public_key)
            && record.record_id == purpose.record_id(pins),
        "native locator profile or fixed domain differs"
    );
    let [signed_endpoint] = record.endpoints.as_slice() else {
        anyhow::bail!("native locator requires exactly one endpoint");
    };
    let address = native_chain_query_address(record)?;
    let magic = match (purpose, signed_endpoint.max_bytes_per_minute, address) {
        (LocatorPurpose::Node, V1_RELAY_BYTES_PER_MINUTE, None) => MAGIC_V1,
        (LocatorPurpose::Node, NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE, None) => MAGIC_V2,
        (LocatorPurpose::Node, NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE, Some(_)) => MAGIC_V3,
        (LocatorPurpose::PublicRead, NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE, None) => {
            ensure!(
                record.sequence != 0
                    && record
                        .expires_at_ms
                        .checked_sub(record.issued_at_ms)
                        .is_some_and(|lease| (1..=MAX_SERVICE_LEASE_MS).contains(&lease)),
                "public read locator requires a bounded signed lease and sequence"
            );
            PUBLIC_READ_V1
        }
        _ => anyhow::bail!("native locator allowance has no compact profile"),
    };
    let carrier = IrohServiceEndpointV1::from_record(record)?;
    if let LocatorPurpose::PublicRead = purpose {
        ensure!(
            carrier.address.direct.is_empty() && !carrier.address.relays.is_empty(),
            "public read locator requires a relay-only route"
        );
    }
    let mut endpoint = if let Some(address) = address {
        native_chain_query_endpoint(&carrier, address)?
    } else {
        carrier.endpoint()?
    };
    endpoint.max_bytes_per_minute = signed_endpoint.max_bytes_per_minute;
    ensure!(
        record.endpoints == [endpoint.clone()],
        "native locator projection would change signed fields"
    );
    let size = HEADER + endpoint.uri.len() + MLDSA65_SIGNATURE_BYTES;
    ensure!(
        size <= purpose.max_body(),
        "native locator exceeds original discovery budget"
    );
    let mut wire = Vec::with_capacity(size);
    wire.extend_from_slice(magic);
    for value in [record.issued_at_ms, record.expires_at_ms, record.sequence] {
        wire.extend_from_slice(&value.to_be_bytes());
    }
    wire.extend_from_slice(&u16::try_from(endpoint.uri.len())?.to_be_bytes());
    wire.extend_from_slice(endpoint.uri.as_bytes());
    wire.extend_from_slice(&record.signature);
    Ok(wire)
}

pub fn decode_node_locator_v1(
    wire: &[u8],
    expected_key: &[u8],
    pins: Pins,
    now: u64,
    verifier: &dyn TransportVerifier,
) -> Result<PeerSignedRelayRecordV1> {
    decode_locator(
        wire,
        expected_key,
        pins,
        now,
        verifier,
        LocatorPurpose::Node,
    )
}

pub fn decode_public_read_locator_v1(
    wire: &[u8],
    expected_key: &[u8],
    pins: Pins,
    now: u64,
    verifier: &dyn TransportVerifier,
) -> Result<PeerSignedRelayRecordV1> {
    decode_locator(
        wire,
        expected_key,
        pins,
        now,
        verifier,
        LocatorPurpose::PublicRead,
    )
}

fn decode_locator(
    wire: &[u8],
    expected_key: &[u8],
    pins: Pins,
    now: u64,
    verifier: &dyn TransportVerifier,
    purpose: LocatorPurpose,
) -> Result<PeerSignedRelayRecordV1> {
    ensure!(
        wire.len() >= HEADER + MLDSA65_SIGNATURE_BYTES && wire.len() <= purpose.max_body(),
        "native locator wire size"
    );
    ensure!(
        expected_key.len() == MLDSA65_PUBLIC_KEY_BYTES,
        "native locator independently selected key"
    );
    // The original signature authenticates the reconstructed allowance too.
    // A changed profile marker cannot promote or downgrade a signed record.
    let max_bytes_per_minute = match (purpose, &wire[..8]) {
        (LocatorPurpose::Node, magic) if magic == MAGIC_V1 => V1_RELAY_BYTES_PER_MINUTE,
        (LocatorPurpose::Node, magic) if magic == MAGIC_V2 || magic == MAGIC_V3 => {
            NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE
        }
        (LocatorPurpose::PublicRead, magic) if magic == PUBLIC_READ_V1 => {
            NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE
        }
        _ => anyhow::bail!("native locator version"),
    };
    let length = u16::from_be_bytes(wire[32..34].try_into()?) as usize;
    ensure!(
        wire.len() == HEADER + length + MLDSA65_SIGNATURE_BYTES,
        "noncanonical native locator length"
    );
    let record = PeerSignedRelayRecordV1 {
        version: PRODUCT_DIRECTORY_VERSION_V1,
        suite: MLDSA65_SUITE,
        record_id: purpose.record_id(pins),
        relay_peer_id: peer_id_from_mldsa65_public_key_v1(expected_key),
        relay_public_key: expected_key.to_vec(),
        endpoints: vec![RelayEndpointV1 {
            transport: RelayTransportV1::IrohRendezvousV1,
            uri: std::str::from_utf8(&wire[HEADER..HEADER + length])?.to_owned(),
            priority: 0,
            max_sessions: 4,
            max_bytes_per_minute,
        }],
        issued_at_ms: u64::from_be_bytes(wire[8..16].try_into()?),
        expires_at_ms: u64::from_be_bytes(wire[16..24].try_into()?),
        sequence: u64::from_be_bytes(wire[24..32].try_into()?),
        signature: wire[HEADER + length..].to_vec(),
    };
    validate_relay_record_v1(&record, now, verifier)?;
    // Preserve the exact signed projection and reject version/suffix substitution.
    ensure!(
        encode_locator(&record, pins, purpose)? == wire,
        "noncanonical native locator or version"
    );
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        iroh_transport::IrohAddressV1, product_directory::sign_relay_record_v1,
        product_overlay::TransportIdentity,
    };

    #[test]
    fn public_read_offer_binds_purpose_identity_chain_and_bounded_lease() -> Result<()> {
        let provider = TransportIdentity::from_bytes(&[101; 32]);
        let other = TransportIdentity::from_bytes(&[102; 32]);
        let key = provider.public_key();
        let pins = (19, [9; 32], [10; 32]);
        let route = IrohServiceEndpointV1 {
            instance: [11; 32],
            address: IrohAddressV1 {
                endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[12; 32])
                    .verifying_key()
                    .to_bytes(),
                direct: Vec::new(),
                relays: vec!["https://relay.example.org/".into()],
            },
        };
        let mut endpoint = route.endpoint()?;
        endpoint.max_bytes_per_minute = NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE;
        let offer = sign_relay_record_v1(
            &provider,
            public_read_locator_id_v1(pins),
            vec![endpoint.clone()],
            1000,
            2000,
            7,
        )?;
        let ordinary = sign_relay_record_v1(
            &provider,
            native_locator_id_v1(pins),
            vec![endpoint.clone()],
            1000,
            2000,
            7,
        )?;
        let mut wire = encode_public_read_locator_v1(&offer, pins)?;
        assert_eq!(
            decode_public_read_locator_v1(&wire, key, pins, 1000, &provider)?,
            offer
        );
        assert!(
            decode_public_read_locator_v1(&wire, other.public_key(), pins, 1000, &provider)
                .is_err()
        );
        assert!(
            decode_public_read_locator_v1(&wire, key, (20, pins.1, pins.2), 1000, &provider)
                .is_err()
        );
        assert!(decode_public_read_locator_v1(&wire, key, pins, 999, &provider).is_err());
        assert!(decode_public_read_locator_v1(&wire, key, pins, 2000, &provider).is_err());
        assert!(encode_node_locator_v1(&offer, pins).is_err());
        assert!(encode_public_read_locator_v1(&ordinary, pins).is_err());
        wire[..8].copy_from_slice(MAGIC_V2);
        assert!(decode_node_locator_v1(&wire, key, pins, 1000, &provider).is_err());
        let mut wire = encode_node_locator_v1(&ordinary, pins)?;
        wire[..8].copy_from_slice(PUBLIC_READ_V1);
        assert!(decode_public_read_locator_v1(&wire, key, pins, 1000, &provider).is_err());
        let mut exposed_route = route.clone();
        exposed_route.address.direct.push("127.0.0.1:9000".parse()?);
        let mut exposed_endpoint = exposed_route.endpoint()?;
        exposed_endpoint.max_bytes_per_minute = NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE;
        let exposed = sign_relay_record_v1(
            &provider,
            public_read_locator_id_v1(pins),
            vec![exposed_endpoint],
            1000,
            2000,
            7,
        )?;
        assert!(encode_public_read_locator_v1(&exposed, pins).is_err());
        let oversized = sign_relay_record_v1(
            &provider,
            public_read_locator_id_v1(pins),
            vec![endpoint],
            1000,
            1001 + MAX_SERVICE_LEASE_MS,
            7,
        )?;
        assert!(encode_public_read_locator_v1(&oversized, pins).is_err());
        Ok(())
    }

    #[test]
    fn signed_native_query_address_is_bound_to_the_original_locator() -> Result<()> {
        let key = TransportIdentity::from_bytes(&[104; 32]);
        let pins = (7, [8; 32], [9; 32]);
        let route = IrohServiceEndpointV1 {
            instance: [10; 32],
            address: IrohAddressV1 {
                endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[11; 32])
                    .verifying_key()
                    .to_bytes(),
                direct: vec!["127.0.0.1:9000".parse()?],
                relays: vec![],
            },
        };
        for ip in ["127.0.0.1:51011", "[::1]:51012"] {
            let address = ip.parse()?;
            let record = sign_relay_record_v1(
                &key,
                native_locator_id_v1(pins),
                vec![native_chain_query_endpoint(&route, address)?],
                1000,
                2000,
                4,
            )?;
            let wire = encode_node_locator_v1(&record, pins)?;
            assert_eq!(&wire[..8], MAGIC_V3);
            let decoded = decode_node_locator_v1(&wire, key.public_key(), pins, 1000, &key)?;
            assert_eq!(decoded, record);
            assert_eq!(native_chain_query_address(&decoded)?, Some(address));
            let decoded_route = IrohServiceEndpointV1::from_record(&decoded)?;
            assert_eq!(decoded_route.instance, route.instance);
            assert_eq!(
                decoded_route.address.endpoint_key,
                route.address.endpoint_key
            );
            assert_eq!(decoded_route.address.direct, route.address.direct);
            assert_eq!(decoded_route.address.relays, route.address.relays);
            let mut bad_version = wire.clone();
            bad_version[..8].copy_from_slice(MAGIC_V2);
            assert!(
                decode_node_locator_v1(&bad_version, key.public_key(), pins, 1000, &key).is_err()
            );
            let mut bad_address = wire;
            let last_address_digit = bad_address.len() - MLDSA65_SIGNATURE_BYTES - 1;
            bad_address[last_address_digit] ^= 1;
            assert!(
                decode_node_locator_v1(&bad_address, key.public_key(), pins, 1000, &key).is_err()
            );
        }
        assert!(native_chain_query_endpoint(&route, "0.0.0.0:51011".parse()?).is_err());
        Ok(())
    }
    #[test]
    fn compact_profiles_preserve_signed_allowances_and_reject_profile_substitution() -> Result<()> {
        let key = TransportIdentity::from_bytes(&[103; 32]);
        let pins = (7, [8; 32], [9; 32]);
        let route = IrohServiceEndpointV1 {
            instance: [10; 32],
            address: IrohAddressV1 {
                endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[11; 32])
                    .verifying_key()
                    .to_bytes(),
                direct: vec!["127.0.0.1:9000".parse()?],
                relays: vec![],
            },
        };
        for (allowance, magic, other) in [
            (V1_RELAY_BYTES_PER_MINUTE, MAGIC_V1, MAGIC_V2),
            (NATIVE_LOCATOR_V2_RELAY_BYTES_PER_MINUTE, MAGIC_V2, MAGIC_V1),
        ] {
            let mut endpoint = route.endpoint()?;
            endpoint.max_bytes_per_minute = allowance;
            let record = sign_relay_record_v1(
                &key,
                native_locator_id_v1(pins),
                vec![endpoint],
                1000,
                2000,
                1,
            )?;
            let mut wire = encode_node_locator_v1(&record, pins)?;
            assert_eq!(&wire[..8], magic);
            assert_eq!(
                wire.len(),
                HEADER + record.endpoints[0].uri.len() + MLDSA65_SIGNATURE_BYTES
            );
            assert!(wire.len() <= super::super::MAX_BODY);
            assert_eq!(
                decode_node_locator_v1(&wire, key.public_key(), pins, 1000, &key)?,
                record
            );
            wire[..8].copy_from_slice(other);
            assert!(decode_node_locator_v1(&wire, key.public_key(), pins, 1000, &key).is_err());
            wire[..8].copy_from_slice(b"NVLOCR03");
            assert!(decode_node_locator_v1(&wire, key.public_key(), pins, 1000, &key).is_err());
            let mut unsupported = record.clone();
            unsupported.endpoints[0].max_bytes_per_minute = allowance + 1;
            assert!(encode_node_locator_v1(&unsupported, pins).is_err());
            let mut changed_sessions = record.clone();
            changed_sessions.endpoints[0].max_sessions += 1;
            assert!(encode_node_locator_v1(&changed_sessions, pins).is_err());
            let mut changed_priority = record;
            changed_priority.endpoints[0].priority += 1;
            assert!(encode_node_locator_v1(&changed_priority, pins).is_err());
        }
        Ok(())
    }
    #[test]
    fn binary_locator_preserves_original_signature_and_rejects_remote_domain_key_tamper_and_expiry(
    ) -> Result<()> {
        let key = TransportIdentity::from_bytes(&[101; 32]);
        let pins = (7, [8; 32], [9; 32]);
        let route = IrohServiceEndpointV1 {
            instance: [10; 32],
            address: IrohAddressV1 {
                endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[11; 32])
                    .verifying_key()
                    .to_bytes(),
                direct: vec!["127.0.0.1:9000".parse()?],
                relays: vec![],
            },
        };
        let record = sign_relay_record_v1(
            &key,
            native_locator_id_v1(pins),
            vec![route.endpoint()?],
            1000,
            2000,
            1,
        )?;
        let wire = encode_node_locator_v1(&record, pins)?;
        assert!(wire.len() <= 4096);
        assert_eq!(
            decode_node_locator_v1(&wire, key.public_key(), pins, 1000, &key)?,
            record
        );
        assert!(
            decode_node_locator_v1(&wire, key.public_key(), (8, pins.1, pins.2), 1000, &key)
                .is_err()
        );
        assert!(decode_node_locator_v1(
            &wire,
            TransportIdentity::from_bytes(&[102; 32]).public_key(),
            pins,
            1000,
            &key
        )
        .is_err());
        assert!(decode_node_locator_v1(&wire, key.public_key(), pins, 2000, &key).is_err());
        for index in [8, 24, HEADER, wire.len() - 1] {
            let mut bad = wire.clone();
            bad[index] ^= 1;
            assert!(decode_node_locator_v1(&bad, key.public_key(), pins, 1000, &key).is_err());
        }
        let mut longer = wire.clone();
        longer.push(0);
        assert!(decode_node_locator_v1(&longer, key.public_key(), pins, 1000, &key).is_err());
        let mut modified = record;
        modified.endpoints[0].max_sessions += 1;
        assert!(encode_node_locator_v1(&modified, pins).is_err());
        Ok(())
    }
}
