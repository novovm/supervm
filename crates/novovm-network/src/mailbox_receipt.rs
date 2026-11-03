//! Storage evidence only. A node signature never represents recipient delivery.
#[cfg(any(feature = "iroh-transport", test))]
use crate::product_overlay::TransportIdentity;
use crate::product_overlay::{
    TransportVerifier, MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES, MLDSA65_SUITE,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreReceiptV1 {
    pub version: u8,
    pub suite: u16,
    pub node: Vec<u8>,
    pub mailbox: [u8; 32],
    pub object: [u8; 32],
    pub digest: [u8; 32],
    pub bytes: u64,
    pub expires: i64,
    pub signature: Vec<u8>,
}
impl StoreReceiptV1 {
    // Called ONLY after real custody commits (or its still-retained duplicate).
    // Schema 3 retains no consumed-object evidence. A retry after deletion must
    // commit ciphertext again. The same tuple represents the same obligation, never
    // a second reward, extended original TTL or proof of device delivery.
    #[cfg(any(feature = "iroh-transport", test))]
    pub(crate) fn committed(
        key: &TransportIdentity,
        mailbox: [u8; 32],
        object: [u8; 32],
        ciphertext: &[u8],
        expires: i64,
    ) -> Result<Self> {
        let mut receipt = Self {
            version: 2,
            suite: MLDSA65_SUITE,
            node: key.public_key().to_vec(),
            mailbox,
            object,
            digest: Sha256::digest(ciphertext).into(),
            bytes: ciphertext.len() as u64,
            expires,
            signature: Vec::new(),
        };
        receipt.signature = key.sign(
            &receipt.input(),
            b"NOVONET/mailbox-storage-receipt/ML-DSA-65/v2",
        )?;
        Ok(receipt)
    }
    fn input(&self) -> Vec<u8> {
        let mut bytes = b"novonet.mailbox.storage-receipt.mldsa65.v2\0".to_vec();
        bytes.push(self.version);
        bytes.extend_from_slice(&self.suite.to_be_bytes());
        bytes.extend_from_slice(&self.node);
        for field in [self.mailbox, self.object, self.digest] {
            bytes.extend_from_slice(&field);
        }
        bytes.extend_from_slice(&self.bytes.to_be_bytes());
        bytes.extend_from_slice(&self.expires.to_be_bytes());
        bytes
    }
    pub fn verify(
        &self,
        node: &[u8],
        mailbox: [u8; 32],
        object: [u8; 32],
        ciphertext: &[u8],
        expires: i64,
        verifier: &dyn TransportVerifier,
    ) -> Result<()> {
        ensure!(
            self.version == 2
                && self.suite == MLDSA65_SUITE
                && self.node.len() == MLDSA65_PUBLIC_KEY_BYTES
                && self.signature.len() == MLDSA65_SIGNATURE_BYTES
                && self.node == node
                && self.mailbox == mailbox
                && self.object == object
                && self.digest == <[u8; 32]>::from(Sha256::digest(ciphertext))
                && self.bytes == ciphertext.len() as u64
                && self.expires == expires
                && expires > 0
                && !ciphertext.is_empty(),
            "storage receipt binding"
        );
        verifier.verify(
            node,
            &self.input(),
            b"NOVONET/mailbox-storage-receipt/ML-DSA-65/v2",
            &self.signature,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_receipt_binds_native_signer_ciphertext_tuple_and_suite() -> Result<()> {
        let key = crate::test_identity::key(10);
        let other = crate::test_identity::key(11);
        let receipt = StoreReceiptV1::committed(&key, [2; 32], [3; 32], b"ciphertext", 5000)?;
        receipt.verify(
            key.public_key(),
            [2; 32],
            [3; 32],
            b"ciphertext",
            5000,
            &other,
        )?;
        assert!(receipt
            .verify(
                other.public_key(),
                [2; 32],
                [3; 32],
                b"ciphertext",
                5000,
                &key
            )
            .is_err());
        assert!(receipt
            .verify(key.public_key(), [2; 32], [3; 32], b"changed", 5000, &key)
            .is_err());
        assert!(receipt
            .verify(
                key.public_key(),
                [4; 32],
                [3; 32],
                b"ciphertext",
                5000,
                &key
            )
            .is_err());
        assert!(receipt
            .verify(
                key.public_key(),
                [2; 32],
                [4; 32],
                b"ciphertext",
                5000,
                &key
            )
            .is_err());
        assert!(receipt
            .verify(
                key.public_key(),
                [2; 32],
                [3; 32],
                b"ciphertext",
                5001,
                &key
            )
            .is_err());
        for (version, suite) in [(1, MLDSA65_SUITE), (2, 0), (2, 44)] {
            let mut bad = receipt.clone();
            bad.version = version;
            bad.suite = suite;
            assert!(bad
                .verify(
                    key.public_key(),
                    [2; 32],
                    [3; 32],
                    b"ciphertext",
                    5000,
                    &key
                )
                .is_err());
        }
        let mut bad = receipt.clone();
        bad.signature[0] ^= 1;
        assert!(bad
            .verify(
                key.public_key(),
                [2; 32],
                [3; 32],
                b"ciphertext",
                5000,
                &key
            )
            .is_err());
        Ok(())
    }
}
