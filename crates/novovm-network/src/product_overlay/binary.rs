//! Carrier-independent Data subset of the existing NVRLY002 binary codec.
//! Source: product_relay_wire.rs. The historical magic is retained, but this
//! codec neither contacts a relay nor accepts JSON/Delivery/control messages.
//! All lengths are preflighted before allocation. Authentication remains with
//! the existing E2eSecureChannelV1, not this framing adapter.

use super::SecureNovoRudpEnvelopeV1;
use anyhow::{ensure, Context, Result};

const MAGIC: &[u8; 8] = b"NVRLY002";
const DATA: u8 = 1;

pub fn encode(envelope: &SecureNovoRudpEnvelopeV1, limit: usize) -> Result<Vec<u8>> {
    let mut size = 9usize + 2 + 16 + 8 + 12;
    for bytes in [
        envelope.sender_peer_id.as_bytes(),
        envelope.recipient_peer_id.as_bytes(),
        &envelope.ciphertext,
    ] {
        u32::try_from(bytes.len()).context("binary field exceeds u32")?;
        size = size
            .checked_add(4)
            .and_then(|s| s.checked_add(bytes.len()))
            .filter(|s| *s <= limit)
            .context("binary envelope exceeds carrier limit")?;
    }
    let mut out = Vec::new();
    out.try_reserve_exact(size)?;
    out.extend_from_slice(MAGIC);
    out.push(DATA);
    out.extend_from_slice(&envelope.version.to_be_bytes());
    out.extend_from_slice(&envelope.session_id);
    put_variable(&mut out, envelope.sender_peer_id.as_bytes());
    put_variable(&mut out, envelope.recipient_peer_id.as_bytes());
    out.extend_from_slice(&envelope.sequence.to_be_bytes());
    out.extend_from_slice(&envelope.nonce);
    put_variable(&mut out, &envelope.ciphertext);
    debug_assert_eq!(out.len(), size);
    Ok(out)
}

fn put_variable(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let bytes = self.0.get(..length).context("truncated binary field")?;
        self.0 = &self.0[length..];
        Ok(bytes)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into()?)
    }
    fn variable(&mut self) -> Result<&'a [u8]> {
        let length = usize::try_from(u32::from_be_bytes(self.array()?))?;
        self.take(length)
    }
    fn string(&mut self) -> Result<&'a str> {
        Ok(std::str::from_utf8(self.variable()?)?)
    }
}

pub fn decode(wire: &[u8], limit: usize) -> Result<SecureNovoRudpEnvelopeV1> {
    let (mut envelope, ciphertext) = decode_header(wire, limit)?;
    envelope.ciphertext = wire[ciphertext].to_vec();
    Ok(envelope)
}

pub(crate) fn decode_owned(mut wire: Vec<u8>, limit: usize) -> Result<SecureNovoRudpEnvelopeV1> {
    let (mut envelope, ciphertext) = decode_header(&wire, limit)?;
    let length = ciphertext.len();
    // Reuse the transport's packet allocation for in-place authenticated open.
    wire.copy_within(ciphertext, 0);
    wire.truncate(length);
    envelope.ciphertext = wire;
    Ok(envelope)
}

fn decode_header(
    wire: &[u8],
    limit: usize,
) -> Result<(SecureNovoRudpEnvelopeV1, std::ops::Range<usize>)> {
    ensure!(wire.len() <= limit, "binary envelope exceeds carrier limit");
    let mut cursor = Cursor(wire);
    ensure!(
        cursor.take(8)? == MAGIC && cursor.array::<1>()?[0] == DATA,
        "binary envelope requires NVRLY002 Data"
    );
    let version = u16::from_be_bytes(cursor.array()?);
    let session_id = cursor.array()?;
    let sender = cursor.string()?;
    let recipient = cursor.string()?;
    let sequence = u64::from_be_bytes(cursor.array()?);
    let nonce = cursor.array()?;
    let ciphertext = cursor.variable()?;
    ensure!(cursor.0.is_empty(), "trailing binary envelope bytes");
    let range = wire.len() - ciphertext.len()..wire.len();
    Ok((
        SecureNovoRudpEnvelopeV1 {
            version,
            session_id,
            sender_peer_id: sender.to_owned(),
            recipient_peer_id: recipient.to_owned(),
            sequence,
            nonce,
            ciphertext: Vec::new(),
        },
        range,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_existing_data_layout_and_rejects_bad_framing() {
        let envelope = SecureNovoRudpEnvelopeV1 {
            version: 1,
            session_id: [2; 16],
            sender_peer_id: "a".into(),
            recipient_peer_id: "b".into(),
            sequence: 3,
            nonce: [4; 12],
            ciphertext: vec![5; 17],
        };
        let wire = encode(&envelope, 1200).unwrap();
        // Layout independently assembled in the original NVRLY002 field order.
        let mut expected = b"NVRLY002\x01\x00\x01".to_vec();
        expected.extend([2; 16]);
        expected.extend(b"\x00\x00\x00\x01a\x00\x00\x00\x01b");
        expected.extend(3u64.to_be_bytes());
        expected.extend([4; 12]);
        expected.extend(17u32.to_be_bytes());
        expected.extend([5; 17]);
        assert_eq!(wire, expected);
        assert_eq!(decode(&wire, 1200).unwrap(), envelope);
        let mut owned = wire.clone();
        owned.reserve(32);
        let allocation = owned.as_ptr();
        let capacity = owned.capacity();
        let decoded = decode_owned(owned, 1200).unwrap();
        assert_eq!(decoded, envelope);
        assert_eq!(decoded.ciphertext.as_ptr(), allocation);
        assert_eq!(decoded.ciphertext.capacity(), capacity);
        for end in 0..wire.len() {
            assert!(decode(&wire[..end], 1200).is_err());
            assert!(decode_owned(wire[..end].to_vec(), 1200).is_err());
        }
        assert!(encode(&envelope, wire.len() - 1).is_err());
        assert!(decode(&wire, wire.len() - 1).is_err());
        assert!(decode_owned(wire.clone(), wire.len() - 1).is_err());
        let mut bad = wire.clone();
        bad.push(0);
        assert!(decode(&bad, 1200).is_err());
        assert!(decode_owned(bad.clone(), 1200).is_err());
        bad = wire.clone();
        bad[8] = 2;
        assert!(decode(&bad, 1200).is_err());
        assert!(decode_owned(bad.clone(), 1200).is_err());
        bad = wire;
        bad[27..31].fill(255);
        assert!(decode(&bad, 1200).is_err());
        assert!(decode_owned(bad, 1200).is_err());
        assert!(decode(br#"{"kind":"data"}"#, 1200).is_err());
    }
}
