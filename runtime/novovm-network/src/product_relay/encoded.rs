//! A single bounded immutable V1 wire representation, prepared outside the
//! session lock. Decoded compatibility APIs are not the daemon send path.
use super::PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1;
use super::{OpaqueRelayDeliveryV1, ProductRelayWireMessageV1, RelayPeerHandshakeDeliveryV1};
use std::io::{self, Write};

pub(super) struct EncodedDeliveryV1(Box<[u8]>);

impl EncodedDeliveryV1 {
    pub(super) fn prepare(message: ProductRelayWireMessageV1) -> Option<Self> {
        let mut output = BoundedBytes(Vec::new());
        serde_json::to_writer(&mut output, &message).ok()?;
        Some(Self(output.0.into_boxed_slice()))
    }

    pub(super) fn wire_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) fn into_data(self) -> OpaqueRelayDeliveryV1 {
        match serde_json::from_slice(&self.0).expect("locally encoded immutable delivery") {
            ProductRelayWireMessageV1::Delivery(delivery) => delivery,
            _ => unreachable!("data inbox contains only locally encoded Data deliveries"),
        }
    }

    pub(super) fn into_control(self) -> RelayPeerHandshakeDeliveryV1 {
        match serde_json::from_slice(&self.0).expect("locally encoded immutable delivery") {
            ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) => delivery,
            _ => unreachable!("control inbox contains only locally encoded handshakes"),
        }
    }
}

struct BoundedBytes(Vec<u8>);

impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .0
            .len()
            .checked_add(bytes.len())
            .filter(|size| *size <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1)
            .ok_or_else(|| io::Error::other("encoded relay delivery exceeds wire limit"))?;
        if next > self.0.capacity() {
            let capacity = self
                .0
                .capacity()
                .saturating_mul(2)
                .max(4096)
                .max(next)
                .min(PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
            self.0
                .try_reserve_exact(capacity - self.0.len())
                .map_err(|_| io::Error::other("encoded relay delivery allocation failed"))?;
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization_buffer_cannot_grow_past_the_existing_wire_bound() {
        let mut output = BoundedBytes(Vec::new());
        let chunk = [9u8; 4096];
        for _ in 0..PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 / chunk.len() {
            output.write_all(&chunk).unwrap();
            assert!(output.0.capacity() <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
        }
        assert_eq!(output.0.len(), PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
        assert!(output.write_all(&[1]).is_err());
        assert_eq!(output.0.len(), PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
        assert!(output.0.iter().all(|byte| *byte == 9));
        assert_eq!(output.write(&[]).unwrap(), 0);
    }
}
