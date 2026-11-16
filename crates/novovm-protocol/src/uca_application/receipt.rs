use super::ApplicationEncodingError;
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 6] = b"NUAR\x01\x41";
const MAX_RECEIPT_BYTES: usize = 256;

/// Application grant state recorded by node execution, not a finality proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationGrantReceiptV1 {
    pub command_id: [u8; 32],
    pub grant_id: [u8; 32],
    pub issuer_key_ref: [u8; 32],
    pub previous_revision: u64,
    pub revision: u64,
    pub revoked: bool,
    pub not_before_height: u64,
    pub expires_at_height: u64,
}

impl ApplicationGrantReceiptV1 {
    pub fn validate(&self) -> Result<(), ApplicationEncodingError> {
        if self.command_id == [0; 32]
            || self.grant_id == [0; 32]
            || self.issuer_key_ref == [0; 32]
            || self.previous_revision.checked_add(1) != Some(self.revision)
            || self.expires_at_height <= self.not_before_height
            || (self.revision == u64::MAX && !self.revoked)
        {
            return Err(ApplicationEncodingError);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, ApplicationEncodingError> {
        self.validate()?;
        let payload = postcard::to_allocvec(self).map_err(|_| ApplicationEncodingError)?;
        if MAGIC.len() + payload.len() > MAX_RECEIPT_BYTES {
            return Err(ApplicationEncodingError);
        }
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&payload);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ApplicationEncodingError> {
        if bytes.len() > MAX_RECEIPT_BYTES || !bytes.starts_with(MAGIC) {
            return Err(ApplicationEncodingError);
        }
        let (receipt, tail): (Self, _) = postcard::take_from_bytes(&bytes[MAGIC.len()..])
            .map_err(|_| ApplicationEncodingError)?;
        if !tail.is_empty() || receipt.encode()? != bytes {
            return Err(ApplicationEncodingError);
        }
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> ApplicationGrantReceiptV1 {
        ApplicationGrantReceiptV1 {
            command_id: [1; 32],
            grant_id: [2; 32],
            issuer_key_ref: [3; 32],
            previous_revision: 0,
            revision: 1,
            revoked: false,
            not_before_height: 10,
            expires_at_height: 100,
        }
    }

    fn unchecked_bytes(receipt: &ApplicationGrantReceiptV1) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&postcard::to_allocvec(receipt).unwrap());
        bytes
    }

    fn assert_invalid(receipt: &ApplicationGrantReceiptV1) {
        assert_eq!(receipt.validate(), Err(ApplicationEncodingError));
        assert_eq!(receipt.encode(), Err(ApplicationEncodingError));
        assert_eq!(
            ApplicationGrantReceiptV1::decode(&unchecked_bytes(receipt)),
            Err(ApplicationEncodingError)
        );
    }

    #[test]
    fn authorization_and_revocation_have_canonical_roundtrips() {
        for revoked in [false, true] {
            let mut receipt = receipt();
            receipt.revoked = revoked;
            let bytes = receipt.encode().unwrap();
            assert_eq!(receipt.validate(), Ok(()));
            assert_eq!(&bytes[..6], b"NUAR\x01\x41");
            assert!(bytes.len() <= MAX_RECEIPT_BYTES);
            assert_eq!(bytes, unchecked_bytes(&receipt));
            let decoded = ApplicationGrantReceiptV1::decode(&bytes).unwrap();
            assert_eq!(decoded, receipt);
            assert_eq!(decoded.encode().unwrap(), bytes);
        }
    }

    #[test]
    fn maximum_revision_is_a_revoked_terminal_value() {
        let mut receipt = receipt();
        receipt.previous_revision = u64::MAX - 2;
        receipt.revision = u64::MAX - 1;
        receipt.not_before_height = u64::MAX - 1;
        receipt.expires_at_height = u64::MAX;
        let bytes = receipt.encode().unwrap();
        assert_eq!(ApplicationGrantReceiptV1::decode(&bytes).unwrap(), receipt);

        receipt.previous_revision = u64::MAX - 1;
        receipt.revision = u64::MAX;
        assert_invalid(&receipt);
        receipt.revoked = true;
        let bytes = receipt.encode().unwrap();
        assert!(bytes.len() <= MAX_RECEIPT_BYTES);
        assert_eq!(ApplicationGrantReceiptV1::decode(&bytes).unwrap(), receipt);
    }

    #[test]
    fn zero_ids_and_issuer_are_rejected() {
        for field in 0..3 {
            let mut receipt = receipt();
            match field {
                0 => receipt.command_id = [0; 32],
                1 => receipt.grant_id = [0; 32],
                2 => receipt.issuer_key_ref = [0; 32],
                _ => unreachable!(),
            }
            assert_invalid(&receipt);
        }
    }

    #[test]
    fn revision_mismatch_and_checked_overflow_are_rejected() {
        for (previous_revision, revision) in [
            (0, 0),
            (0, 2),
            (1, 1),
            (2, 1),
            (u64::MAX, 0),
            (u64::MAX, u64::MAX),
        ] {
            for revoked in [false, true] {
                let mut receipt = receipt();
                receipt.previous_revision = previous_revision;
                receipt.revision = revision;
                receipt.revoked = revoked;
                assert_invalid(&receipt);
            }
        }
    }

    #[test]
    fn height_window_boundaries_are_checked() {
        for revoked in [false, true] {
            for (not_before_height, expires_at_height) in [
                (0, 0),
                (10, 10),
                (10, 9),
                (u64::MAX, u64::MAX),
                (u64::MAX, 0),
            ] {
                let mut receipt = receipt();
                receipt.revoked = revoked;
                receipt.not_before_height = not_before_height;
                receipt.expires_at_height = expires_at_height;
                assert_invalid(&receipt);
            }
            for (not_before_height, expires_at_height) in [(0, 1), (0, u64::MAX)] {
                let mut receipt = receipt();
                receipt.revoked = revoked;
                receipt.not_before_height = not_before_height;
                receipt.expires_at_height = expires_at_height;
                let bytes = receipt.encode().unwrap();
                assert_eq!(ApplicationGrantReceiptV1::decode(&bytes).unwrap(), receipt);
            }
        }
    }

    #[test]
    fn truncation_trailing_bytes_and_excessive_lengths_are_rejected() {
        let bytes = receipt().encode().unwrap();
        for length in 0..bytes.len() {
            assert!(ApplicationGrantReceiptV1::decode(&bytes[..length]).is_err());
        }
        for suffix in [vec![0], vec![1, 2], bytes.clone()] {
            let mut trailing = bytes.clone();
            trailing.extend_from_slice(&suffix);
            assert!(ApplicationGrantReceiptV1::decode(&trailing).is_err());
        }
        for length in [MAX_RECEIPT_BYTES, MAX_RECEIPT_BYTES + 1, 1024] {
            let mut oversized = bytes.clone();
            oversized.resize(length, 0);
            assert!(ApplicationGrantReceiptV1::decode(&oversized).is_err());
        }
    }

    #[test]
    fn magic_version_domain_and_invalid_boolean_tampering_are_rejected() {
        let bytes = receipt().encode().unwrap();
        for offset in 0..MAGIC.len() {
            let mut bad = bytes.clone();
            bad[offset] ^= 1;
            assert!(ApplicationGrantReceiptV1::decode(&bad).is_err());
        }
        let mut bad = bytes;
        let revoked_offset = MAGIC.len() + 3 * 32 + 2;
        bad[revoked_offset] = 2;
        assert!(ApplicationGrantReceiptV1::decode(&bad).is_err());
    }

    #[test]
    fn noncanonical_and_overlong_integer_encodings_are_rejected() {
        let bytes = receipt().encode().unwrap();
        let integers_start = MAGIC.len() + 3 * 32;
        for offset in [0, 1, 3, 4].map(|offset| integers_start + offset) {
            let mut bad = bytes.clone();
            bad.splice(offset..=offset, [bytes[offset] | 0x80, 0]);
            assert!(ApplicationGrantReceiptV1::decode(&bad).is_err());
        }
        for replacement in [
            vec![0x80; 11],
            vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02],
        ] {
            let mut bad = bytes.clone();
            bad.splice(integers_start..=integers_start, replacement);
            assert!(ApplicationGrantReceiptV1::decode(&bad).is_err());
        }
    }
}
