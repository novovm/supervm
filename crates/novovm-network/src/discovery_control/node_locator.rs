//! Compact projection of the EXISTING signed node record for the original
//! 4KiB discovery RPC. The expected native key and chain pins come from the
//! consumer, never from an unsigned hint in this wire. No new signing engine.
use crate::{
    product_directory::{
        validate_relay_record_v1, PeerSignedRelayRecordV1, RelayEndpointV1, RelayTransportV1,
        PRODUCT_DIRECTORY_VERSION_V1,
    },
    product_overlay::{
        peer_id_from_mldsa65_public_key_v1, TransportVerifier, MLDSA65_PUBLIC_KEY_BYTES,
        MLDSA65_SIGNATURE_BYTES, MLDSA65_SUITE,
    },
    product_service_lease::IrohServiceEndpointV1,
};
use anyhow::{ensure, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"NVLOCR01";
const HEADER: usize = 8 + 3 * 8 + 2;
type Pins = (u64, [u8; 32], [u8; 32]);

pub fn native_locator_id_v1(pins: Pins) -> String {
    let mut hash = Sha256::new();
    hash.update(b"NOVONET/native-chain-locator/v1\0");
    hash.update(pins.0.to_be_bytes());
    hash.update(pins.1);
    hash.update(pins.2);
    format!(
        "native-chain-locator-v1:{}",
        URL_SAFE_NO_PAD.encode(hash.finalize())
    )
}

pub fn encode_node_locator_v1(record: &PeerSignedRelayRecordV1, pins: Pins) -> Result<Vec<u8>> {
    ensure!(
        record.version == PRODUCT_DIRECTORY_VERSION_V1
            && record.suite == MLDSA65_SUITE
            && record.relay_public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
            && record.signature.len() == MLDSA65_SIGNATURE_BYTES
            && record.relay_peer_id == peer_id_from_mldsa65_public_key_v1(&record.relay_public_key)
            && record.record_id == native_locator_id_v1(pins),
        "native locator profile or fixed domain differs"
    );
    let endpoint = IrohServiceEndpointV1::from_record(record)?.endpoint()?;
    ensure!(
        record.endpoints == [endpoint.clone()],
        "native locator projection would change signed fields"
    );
    let size = HEADER + endpoint.uri.len() + MLDSA65_SIGNATURE_BYTES;
    ensure!(
        size <= super::MAX_BODY,
        "native locator exceeds original discovery budget"
    );
    let mut wire = Vec::with_capacity(size);
    wire.extend_from_slice(MAGIC);
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
    ensure!(
        wire.len() >= HEADER + MLDSA65_SIGNATURE_BYTES && wire.len() <= super::MAX_BODY,
        "native locator wire size"
    );
    ensure!(
        &wire[..8] == MAGIC && expected_key.len() == MLDSA65_PUBLIC_KEY_BYTES,
        "native locator version or independently selected key"
    );
    let length = u16::from_be_bytes(wire[32..34].try_into()?) as usize;
    ensure!(
        wire.len() == HEADER + length + MLDSA65_SIGNATURE_BYTES,
        "noncanonical native locator length"
    );
    let record = PeerSignedRelayRecordV1 {
        version: PRODUCT_DIRECTORY_VERSION_V1,
        suite: MLDSA65_SUITE,
        record_id: native_locator_id_v1(pins),
        relay_peer_id: peer_id_from_mldsa65_public_key_v1(expected_key),
        relay_public_key: expected_key.to_vec(),
        endpoints: vec![RelayEndpointV1 {
            transport: RelayTransportV1::IrohRendezvousV1,
            uri: std::str::from_utf8(&wire[HEADER..HEADER + length])?.to_owned(),
            priority: 0,
            max_sessions: 4,
            max_bytes_per_minute: 65536,
        }],
        issued_at_ms: u64::from_be_bytes(wire[8..16].try_into()?),
        expires_at_ms: u64::from_be_bytes(wire[16..24].try_into()?),
        sequence: u64::from_be_bytes(wire[24..32].try_into()?),
        signature: wire[HEADER + length..].to_vec(),
    };
    validate_relay_record_v1(&record, now, verifier)?;
    IrohServiceEndpointV1::from_record(&record)?;
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
