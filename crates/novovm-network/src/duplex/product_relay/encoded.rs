//! One bounded immutable carrier-v2 representation, prepared outside the
//! session lock. Active/offline queues use one codec, never two wire copies.
//! Decoded compatibility APIs are not the daemon send path.
use super::{OpaqueRelayDeliveryV1, ProductRelayWireMessageV1, RelayPeerHandshakeDeliveryV1};
use crate::duplex::product_relay_wire::{decode_message_v2, encode_message_v2};

pub(super) struct EncodedDeliveryV1(Box<[u8]>);

impl EncodedDeliveryV1 {
    pub(super) fn prepare(message: ProductRelayWireMessageV1) -> Option<Self> {
        encode_message_v2(&message)
            .ok()
            .map(|bytes| Self(bytes.into_boxed_slice()))
    }

    pub(super) fn wire_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) fn into_data(self) -> OpaqueRelayDeliveryV1 {
        match decode_message_v2(&self.0).expect("locally encoded immutable delivery") {
            ProductRelayWireMessageV1::Delivery(delivery) => delivery,
            _ => unreachable!("data inbox contains only locally encoded Data deliveries"),
        }
    }

    pub(super) fn into_control(self) -> RelayPeerHandshakeDeliveryV1 {
        match decode_message_v2(&self.0).expect("locally encoded immutable delivery") {
            ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) => delivery,
            _ => unreachable!("control inbox contains only locally encoded handshakes"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duplex::{SecureNovoRudpEnvelopeV1, PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1};

    #[test]
    fn encoded_delivery_respects_existing_wire_bound_including_metadata() {
        let delivery = OpaqueRelayDeliveryV1 {
            source_peer_id: String::new(),
            target_peer_id: String::new(),
            received_at_ms: 0,
            envelope: SecureNovoRudpEnvelopeV1 {
                version: 1,
                session_id: [0; 16],
                sender_peer_id: String::new(),
                recipient_peer_id: String::new(),
                sequence: 0,
                nonce: [0; 12],
                ciphertext: vec![9; PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 - 75],
            },
        };
        let encoded =
            EncodedDeliveryV1::prepare(ProductRelayWireMessageV1::Delivery(delivery.clone()))
                .unwrap();
        assert_eq!(encoded.len(), PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
        assert_eq!(encoded.into_data(), delivery);
        let mut oversized = delivery;
        oversized.envelope.ciphertext.push(9);
        assert!(
            EncodedDeliveryV1::prepare(ProductRelayWireMessageV1::Delivery(oversized)).is_none()
        );
    }
}
