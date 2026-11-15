//! Purpose-bound ML-DSA application authority, not asset permission or a login.
//! Encoding does not admit a transaction or establish a trusted state root.
use crate::{
    native_query::UcaIdentityIdV1,
    uca_spend::{MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const APPLICATION_CONTEXT_V1: &[u8] = b"NOVOVM/UCA/application-authority/v1";
pub const MAX_COMMAND_WINDOW: u64 = 256;
const MAGIC: &[u8; 6] = b"NUAP\x01\x41";
const MAX_INTENT_BYTES: usize = 512;
const MAX_SIGNED_BYTES: usize =
    6 + 2 + MAX_INTENT_BYTES + MLDSA65_PUBLIC_KEY_BYTES + MLDSA65_SIGNATURE_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationGrantSelectorV1 {
    pub chain_id: u64,
    pub genesis: [u8; 32],
    pub identity: UcaIdentityIdV1,
    pub application: [u8; 32],
    /// Reference to the selected A service's authenticated ML-DSA key.
    pub service: [u8; 32],
    /// Reference to B's application-purpose ML-DSA key, not its UCA root key.
    pub device: [u8; 32],
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid UCA application authority encoding or bounds")]
pub struct ApplicationEncodingError;
type Result<T> = std::result::Result<T, ApplicationEncodingError>;

impl ApplicationGrantSelectorV1 {
    pub fn validate(&self) -> Result<()> {
        if self.chain_id == 0
            || self.genesis == [0; 32]
            || self.application == [0; 32]
            || self.service == [0; 32]
            || self.device == [0; 32]
        {
            return Err(ApplicationEncodingError);
        }
        Ok(())
    }
    pub fn grant_id(&self) -> Result<[u8; 32]> {
        self.validate()?;
        let mut hash = Sha256::new();
        hash.update(b"NOVOVM/UCA/application-grant-id/v1\0");
        hash.update(postcard::to_allocvec(self).map_err(|_| ApplicationEncodingError)?);
        Ok(hash.finalize().into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationGrantActionV1 {
    Authorize {
        not_before_height: u64,
        expires_at_height: u64,
    },
    Revoke,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationGrantIntentV1 {
    pub selector: ApplicationGrantSelectorV1,
    pub issuer_key_ref: [u8; 32],
    /// Compare-and-swap against the current record, including revoked tombstones.
    pub expected_revision: u64,
    pub valid_from_height: u64,
    pub valid_until_height: u64,
    pub action: ApplicationGrantActionV1,
}
impl ApplicationGrantIntentV1 {
    pub fn validate(&self) -> Result<()> {
        self.selector.validate()?;
        if self.issuer_key_ref == [0; 32]
            || self.expected_revision == u64::MAX
            || self.valid_until_height <= self.valid_from_height
            || self.valid_until_height - self.valid_from_height > MAX_COMMAND_WINDOW
        {
            return Err(ApplicationEncodingError);
        }
        if let ApplicationGrantActionV1::Authorize {
            not_before_height,
            expires_at_height,
        } = self.action
        {
            if self.expected_revision == u64::MAX - 1
                || not_before_height < self.valid_from_height
                || expires_at_height <= not_before_height
            {
                return Err(ApplicationEncodingError);
            }
        }
        Ok(())
    }
    fn encoded(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = postcard::to_allocvec(self).map_err(|_| ApplicationEncodingError)?;
        if bytes.len() > MAX_INTENT_BYTES {
            return Err(ApplicationEncodingError);
        }
        Ok(bytes)
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&self.encoded()?);
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedApplicationGrantV1 {
    pub intent: ApplicationGrantIntentV1,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}
impl SignedApplicationGrantV1 {
    pub fn validate(&self) -> Result<()> {
        self.intent.validate()?;
        if self.public_key.len() != MLDSA65_PUBLIC_KEY_BYTES
            || self.signature.len() != MLDSA65_SIGNATURE_BYTES
        {
            return Err(ApplicationEncodingError);
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let intent = self.intent.encoded()?;
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(intent.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&intent);
        bytes.extend_from_slice(&self.public_key);
        bytes.extend_from_slice(&self.signature);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 8 || bytes.len() > MAX_SIGNED_BYTES || !bytes.starts_with(MAGIC) {
            return Err(ApplicationEncodingError);
        }
        let count = usize::from(u16::from_be_bytes([bytes[6], bytes[7]]));
        if count == 0
            || count > MAX_INTENT_BYTES
            || bytes.len() != 8 + count + MLDSA65_PUBLIC_KEY_BYTES + MLDSA65_SIGNATURE_BYTES
        {
            return Err(ApplicationEncodingError);
        }
        let (intent, tail): (ApplicationGrantIntentV1, _) =
            postcard::take_from_bytes(&bytes[8..8 + count])
                .map_err(|_| ApplicationEncodingError)?;
        if !tail.is_empty() || intent.encoded()? != bytes[8..8 + count] {
            return Err(ApplicationEncodingError);
        }
        let public_end = 8 + count + MLDSA65_PUBLIC_KEY_BYTES;
        let signed = Self {
            intent,
            public_key: bytes[8 + count..public_end].to_vec(),
            signature: bytes[public_end..].to_vec(),
        };
        signed.validate()?;
        Ok(signed)
    }
}

#[cfg(test)]
mod tests;
