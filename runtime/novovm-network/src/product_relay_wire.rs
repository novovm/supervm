//! V2 relay carrier, not a new envelope, authentication or flow-control version.
//! Data/Delivery carry ciphertext verbatim; other variants retain their V1 JSON.
//! A connection must negotiate this carrier before using these entry points.

use crate::product_overlay::SecureNovoRudpEnvelopeV1;
use crate::product_relay::{
    OpaqueRelayDeliveryV1, ProductRelayWireMessageV1, PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
};
use anyhow::{bail, Context, Result};
use serde::{de, Deserialize, Deserializer};
use std::io::{self, Write};

pub(crate) const PRODUCT_RELAY_WEBSOCKET_SUBPROTOCOL_V2: &str = "novovm.relay.binary.v2";
const MAGIC: &[u8; 8] = b"NVRLY002";
const DATA: u8 = 1;
const DELIVERY: u8 = 2;

pub(crate) fn encode_message_v2(message: &ProductRelayWireMessageV1) -> Result<Vec<u8>> {
    let (tag, envelope, delivery) = match message {
        ProductRelayWireMessageV1::Data(envelope) => (DATA, envelope, None),
        ProductRelayWireMessageV1::Delivery(delivery) => {
            (DELIVERY, &delivery.envelope, Some(delivery))
        }
        _ => {
            let mut output = BoundedJson(Vec::new());
            serde_json::to_writer(&mut output, message).context("encode relay V2 control JSON")?;
            return Ok(output.0);
        }
    };

    // Compute and validate the complete size before allocating or copying.
    let mut length = WireLength(MAGIC.len() + 1);
    if let Some(delivery) = delivery {
        length.variable(delivery.source_peer_id.as_bytes())?;
        length.variable(delivery.target_peer_id.as_bytes())?;
        length.fixed(8)?;
    }
    length.fixed(2 + 16 + 8 + 12)?;
    length.variable(envelope.sender_peer_id.as_bytes())?;
    length.variable(envelope.recipient_peer_id.as_bytes())?;
    length.variable(&envelope.ciphertext)?;

    let mut output = Vec::new();
    output.try_reserve_exact(length.0)?;
    output.extend_from_slice(MAGIC);
    output.push(tag);
    if let Some(delivery) = delivery {
        put_variable(&mut output, delivery.source_peer_id.as_bytes())?;
        put_variable(&mut output, delivery.target_peer_id.as_bytes())?;
        output.extend_from_slice(&delivery.received_at_ms.to_be_bytes());
    }
    output.extend_from_slice(&envelope.version.to_be_bytes());
    output.extend_from_slice(&envelope.session_id);
    put_variable(&mut output, envelope.sender_peer_id.as_bytes())?;
    put_variable(&mut output, envelope.recipient_peer_id.as_bytes())?;
    output.extend_from_slice(&envelope.sequence.to_be_bytes());
    output.extend_from_slice(&envelope.nonce);
    put_variable(&mut output, &envelope.ciphertext)?;
    debug_assert_eq!(output.len(), length.0);
    Ok(output)
}

pub(crate) fn decode_message_v2(wire: &[u8]) -> Result<ProductRelayWireMessageV1> {
    if wire.len() > PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 {
        bail!("relay V2 message exceeds wire limit");
    }
    if wire.starts_with(MAGIC) {
        // All framing, lengths, UTF-8 and trailing bytes are checked using
        // borrowed views. No owned ID or ciphertext exists until this succeeds.
        return Ok(BinaryMessage::preflight(wire)?.into_owned());
    }

    // Reject the old JSON Data/Delivery carriers without constructing their
    // ciphertext vectors, including when body precedes kind. This also checks
    // the complete JSON syntax before the ordinary control deserialization.
    serde_json::from_slice::<ControlJsonKind>(wire).context("preflight relay V2 control JSON")?;
    let message = serde_json::from_slice(wire).context("decode relay V2 control JSON")?;
    match message {
        ProductRelayWireMessageV1::Data(_) | ProductRelayWireMessageV1::Delivery(_) => {
            bail!("relay V2 Data/Delivery require the binary carrier")
        }
        _ => Ok(message),
    }
}

struct WireLength(usize);

impl WireLength {
    fn fixed(&mut self, count: usize) -> Result<()> {
        self.0 = self
            .0
            .checked_add(count)
            .filter(|length| *length <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1)
            .context("relay V2 encoded message exceeds wire limit")?;
        Ok(())
    }

    fn variable(&mut self, bytes: &[u8]) -> Result<()> {
        u32::try_from(bytes.len()).context("relay V2 field length exceeds u32")?;
        self.fixed(4)?;
        self.fixed(bytes.len())
    }
}

fn put_variable(output: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let length = u32::try_from(bytes.len()).context("relay V2 field length exceeds u32")?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let bytes = self.0.get(..length).context("truncated relay V2 field")?;
        self.0 = &self.0[length..];
        Ok(bytes)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into()?)
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn variable(&mut self) -> Result<&'a [u8]> {
        let length = usize::try_from(u32::from_be_bytes(self.array()?))?;
        self.take(length)
    }

    fn string(&mut self) -> Result<&'a str> {
        std::str::from_utf8(self.variable()?).context("relay V2 identifier is not UTF-8")
    }
}

struct EnvelopeView<'a> {
    version: u16,
    session_id: [u8; 16],
    sender_peer_id: &'a str,
    recipient_peer_id: &'a str,
    sequence: u64,
    nonce: [u8; 12],
    ciphertext: &'a [u8],
}

impl<'a> EnvelopeView<'a> {
    fn read(cursor: &mut Cursor<'a>) -> Result<Self> {
        Ok(Self {
            version: u16::from_be_bytes(cursor.array()?),
            session_id: cursor.array()?,
            sender_peer_id: cursor.string()?,
            recipient_peer_id: cursor.string()?,
            sequence: cursor.u64()?,
            nonce: cursor.array()?,
            ciphertext: cursor.variable()?,
        })
    }

    fn into_owned(self) -> SecureNovoRudpEnvelopeV1 {
        SecureNovoRudpEnvelopeV1 {
            version: self.version,
            session_id: self.session_id,
            sender_peer_id: self.sender_peer_id.to_owned(),
            recipient_peer_id: self.recipient_peer_id.to_owned(),
            sequence: self.sequence,
            nonce: self.nonce,
            ciphertext: self.ciphertext.to_vec(),
        }
    }
}

struct DeliveryView<'a> {
    source_peer_id: &'a str,
    target_peer_id: &'a str,
    received_at_ms: u64,
}

struct BinaryMessage<'a> {
    envelope: EnvelopeView<'a>,
    delivery: Option<DeliveryView<'a>>,
}

impl<'a> BinaryMessage<'a> {
    fn preflight(wire: &'a [u8]) -> Result<Self> {
        let mut cursor = Cursor(wire);
        if cursor.take(MAGIC.len())? != MAGIC {
            bail!("invalid relay V2 magic");
        }
        let delivery = match cursor.array::<1>()?[0] {
            DATA => None,
            DELIVERY => Some(DeliveryView {
                source_peer_id: cursor.string()?,
                target_peer_id: cursor.string()?,
                received_at_ms: cursor.u64()?,
            }),
            _ => bail!("unknown relay V2 binary tag"),
        };
        let envelope = EnvelopeView::read(&mut cursor)?;
        if !cursor.0.is_empty() {
            bail!("trailing relay V2 binary bytes");
        }
        Ok(Self { envelope, delivery })
    }

    fn into_owned(self) -> ProductRelayWireMessageV1 {
        let envelope = self.envelope.into_owned();
        match self.delivery {
            None => ProductRelayWireMessageV1::Data(envelope),
            Some(delivery) => ProductRelayWireMessageV1::Delivery(OpaqueRelayDeliveryV1 {
                source_peer_id: delivery.source_peer_id.to_owned(),
                target_peer_id: delivery.target_peer_id.to_owned(),
                received_at_ms: delivery.received_at_ms,
                envelope,
            }),
        }
    }
}

#[derive(Deserialize)]
struct ControlJsonKind {
    #[serde(rename = "kind")]
    _kind: ControlKind,
}

struct ControlKind;

impl<'de> Deserialize<'de> for ControlKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;

        impl de::Visitor<'_> for KindVisitor {
            type Value = ControlKind;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a relay control message kind")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "data" | "delivery" => Err(E::custom(
                        "relay V2 Data/Delivery require the binary carrier",
                    )),
                    _ => Ok(ControlKind),
                }
            }
        }

        deserializer.deserialize_str(KindVisitor)
    }
}

struct BoundedJson(Vec<u8>);

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .0
            .len()
            .checked_add(bytes.len())
            .filter(|size| *size <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1)
            .ok_or_else(|| io::Error::other("relay V2 control JSON exceeds wire limit"))?;
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
                .map_err(|_| io::Error::other("relay V2 control JSON allocation failed"))?;
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
