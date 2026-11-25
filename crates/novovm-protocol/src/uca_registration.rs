//! Canonical proof of possession for a post-genesis UCA registration.
use crate::{
    native_query::UcaIdentityIdV1,
    uca_spend::{MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const REGISTRATION_CONTEXT_V1: &[u8] = b"NOVOVM/UCA/identity-registration/v1";
const MAGIC: &[u8; 6] = b"NUIR\x01\x41";
const MAX_INTENT_BYTES: usize = 256;
const MAX_WINDOW: u64 = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaRegistrationIntentV1 {
    pub chain_id: u64,
    pub genesis: [u8; 32],
    pub identity: UcaIdentityIdV1,
    pub primary_key_ref: [u8; 32],
    pub valid_from_height: u64,
    pub valid_until_height: u64,
}

#[derive(Debug, Error)]
#[error("invalid UCA registration encoding or bounds")]
pub struct RegistrationEncodingError;

type Result<T> = std::result::Result<T, RegistrationEncodingError>;

impl UcaRegistrationIntentV1 {
    fn encoded(&self) -> Result<Vec<u8>> {
        if self.chain_id == 0
            || self.genesis == [0; 32]
            || self.primary_key_ref == [0; 32]
            || self.valid_from_height == 0
            || self.valid_until_height <= self.valid_from_height
            || self.valid_until_height - self.valid_from_height > MAX_WINDOW
        {
            return Err(RegistrationEncodingError);
        }
        let bytes = postcard::to_allocvec(self).map_err(|_| RegistrationEncodingError)?;
        if bytes.len() > MAX_INTENT_BYTES {
            return Err(RegistrationEncodingError);
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
pub struct SignedUcaRegistrationV1 {
    pub intent: UcaRegistrationIntentV1,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl SignedUcaRegistrationV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.public_key.len() != MLDSA65_PUBLIC_KEY_BYTES
            || self.signature.len() != MLDSA65_SIGNATURE_BYTES
        {
            return Err(RegistrationEncodingError);
        }
        let intent = self.intent.encoded()?;
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(intent.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&intent);
        bytes.extend_from_slice(&self.public_key);
        bytes.extend_from_slice(&self.signature);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 8 || !bytes.starts_with(MAGIC) {
            return Err(RegistrationEncodingError);
        }
        let count = usize::from(u16::from_be_bytes([bytes[6], bytes[7]]));
        let public_end = 8 + count + MLDSA65_PUBLIC_KEY_BYTES;
        if count == 0
            || count > MAX_INTENT_BYTES
            || bytes.len() != public_end + MLDSA65_SIGNATURE_BYTES
        {
            return Err(RegistrationEncodingError);
        }
        let (intent, tail): (UcaRegistrationIntentV1, _) =
            postcard::take_from_bytes(&bytes[8..8 + count])
                .map_err(|_| RegistrationEncodingError)?;
        if !tail.is_empty() || intent.encoded()? != bytes[8..8 + count] {
            return Err(RegistrationEncodingError);
        }
        Ok(Self {
            intent,
            public_key: bytes[8 + count..public_end].to_vec(),
            signature: bytes[public_end..].to_vec(),
        })
    }
}
