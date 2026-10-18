//! Storage evidence only. A node signature never represents recipient delivery.
use anyhow::{ensure, Result};
use ed25519_dalek::{Signature, VerifyingKey};
#[cfg(feature = "iroh-transport")]
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreReceiptV1 {
    pub version: u8,
    pub node: [u8; 32],
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
    // commit ciphertext again. The same tuple gives the same signature, never
    // a second reward, extended original TTL or proof of device delivery.
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn committed(
        key: &SigningKey,
        mailbox: [u8; 32],
        object: [u8; 32],
        ciphertext: &[u8],
        expires: i64,
    ) -> Self {
        let mut receipt = Self {
            version: 1,
            node: key.verifying_key().to_bytes(),
            mailbox,
            object,
            digest: Sha256::digest(ciphertext).into(),
            bytes: ciphertext.len() as u64,
            expires,
            signature: Vec::new(),
        };
        receipt.signature = key.sign(&receipt.input()).to_bytes().to_vec();
        receipt
    }
    fn input(&self) -> Vec<u8> {
        let mut bytes = b"novonet.mailbox.storage-receipt.v1\0".to_vec();
        bytes.push(self.version);
        for field in [self.node, self.mailbox, self.object, self.digest] {
            bytes.extend_from_slice(&field);
        }
        bytes.extend_from_slice(&self.bytes.to_be_bytes());
        bytes.extend_from_slice(&self.expires.to_be_bytes());
        bytes
    }
    pub fn verify(
        &self,
        node: [u8; 32],
        mailbox: [u8; 32],
        object: [u8; 32],
        ciphertext: &[u8],
        expires: i64,
    ) -> Result<()> {
        ensure!(
            self.version == 1
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
        let key = VerifyingKey::from_bytes(&node)?;
        ensure!(!key.is_weak(), "weak storage signer");
        key.verify_strict(&self.input(), &Signature::from_slice(&self.signature)?)?;
        Ok(())
    }
}
