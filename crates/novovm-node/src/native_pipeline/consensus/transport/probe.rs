//! Opt-in non-business payload on the original control lane. No supplied data,
//! account, execution effect, signature grant or additional reliability layer.
use super::*;

pub const MAX_LIFETIME_MS: u64 = 10_000;
pub const WIRE_BYTES: usize = 32 * 3 + 8 * 2 + 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkProbe {
    pub nonce: Hash,
    pub sender: Hash,
    pub recipient: Hash,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

impl NetworkProbe {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.nonce != [0; 32], "empty probe nonce");
        ensure!(
            self.sender != [0; 32] && self.recipient != [0; 32] && self.sender != self.recipient,
            "invalid probe endpoints"
        );
        ensure!(
            self.expires_at_ms > self.issued_at_ms
                && self.expires_at_ms - self.issued_at_ms <= MAX_LIFETIME_MS,
            "probe lifetime exceeds bound"
        );
        Ok(())
    }

    pub fn validate_time(&self, wall_ms: u64) -> Result<()> {
        self.validate()?;
        ensure!(
            self.issued_at_ms <= wall_ms && wall_ms < self.expires_at_ms,
            "probe outside validity"
        );
        Ok(())
    }

    pub(super) fn append(&self, out: &mut Vec<u8>) -> Result<()> {
        self.validate()?;
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.sender);
        out.extend_from_slice(&self.recipient);
        out.extend_from_slice(&self.issued_at_ms.to_be_bytes());
        out.extend_from_slice(&self.expires_at_ms.to_be_bytes());
        // There is no arbitrary payload argument or business content channel.
        out.extend((0..256).map(|i| self.nonce[i % 32] ^ i as u8));
        Ok(())
    }

    pub(super) fn read(reader: &mut Reader<'_>) -> Result<Self> {
        let probe = Self {
            nonce: reader.hash()?,
            sender: reader.hash()?,
            recipient: reader.hash()?,
            issued_at_ms: reader.u64()?,
            expires_at_ms: reader.u64()?,
        };
        probe.validate()?;
        ensure!(
            reader
                .take(256)?
                .iter()
                .enumerate()
                .all(|(i, b)| *b == probe.nonce[i % 32] ^ i as u8),
            "probe pattern mismatch"
        );
        Ok(probe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_probe_roundtrip_rejects_payload_and_shape_changes() -> Result<()> {
        let probe = NetworkProbe {
            nonce: [1; 32],
            sender: [2; 32],
            recipient: [3; 32],
            issued_at_ms: 10,
            expires_at_ms: 20,
        };
        let limits = DecodeLimits {
            transactions: 1,
            transaction_bytes: 512,
            body_bytes: 512,
            message_bytes: 512,
        };
        let bytes = encode(&Message::NetworkProbe(probe.clone()), limits)?;
        assert_eq!(bytes.len(), PREFIX_BYTES + WIRE_BYTES);
        assert_eq!(message_lane(&bytes)?, 0);
        assert_eq!(
            decode(&bytes, limits)?,
            Message::NetworkProbe(probe.clone())
        );
        assert!(probe.validate_time(9).is_err());
        assert!(probe.validate_time(20).is_err());
        probe.validate_time(10)?;
        for len in 0..bytes.len() {
            assert!(decode(&bytes[..len], limits).is_err());
        }
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(decode(&corrupt, limits).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode(&trailing, limits).is_err());
        assert!(NetworkProbe {
            nonce: [0; 32],
            ..probe.clone()
        }
        .validate()
        .is_err());
        assert!(NetworkProbe {
            recipient: probe.sender,
            ..probe.clone()
        }
        .validate()
        .is_err());
        assert!(NetworkProbe {
            expires_at_ms: 10 + MAX_LIFETIME_MS + 1,
            ..probe
        }
        .validate()
        .is_err());
        Ok(())
    }
}
