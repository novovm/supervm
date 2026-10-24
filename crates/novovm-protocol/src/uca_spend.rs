//! Shared ML-DSA-65 NOV spend intent. Encoding is not signature verification or
//! authority. Native ingress activation and trusted UCA state are separate gates.
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MLDSA65_PUBLIC_KEY_BYTES: usize = 1952;
pub const MLDSA65_SIGNATURE_BYTES: usize = 3309;
pub const SPEND_CONTEXT_V1: &[u8] = b"NOVOVM/UCA/NOV-spend/v1";
pub const MAX_UCA_ID_BYTES: usize = 128;
pub const MAX_SPEND_WINDOW: u64 = 256;
// NUAS, version=1, suite=65 (ML-DSA external pure), purpose=1 (NOV transfer).
const HEADER: &[u8] = b"NUAS\x01\x41\x01";
pub const MAX_SIGNED_SPEND_BYTES: usize = 7
    + 8
    + 32
    + 129
    + 8
    + 32
    + 129
    + 16
    + 16
    + 8
    + 8
    + 8
    + MLDSA65_PUBLIC_KEY_BYTES
    + MLDSA65_SIGNATURE_BYTES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UcaSpendIntentV1 {
    pub chain_id: u64,
    pub genesis: [u8; 32],
    /// Existing stable UCA identifier, never a new address derived from this key.
    pub account_id: String,
    pub authorization_epoch: u64,
    pub spend_key_ref: [u8; 32],
    pub recipient_uca: String,
    /// Native NOV base units (nine decimal places). No implicit foreign asset.
    pub amount: u128,
    pub max_fee: u128,
    pub nonce: u64,
    pub not_before_height: u64,
    /// Exclusive, checked against execution height by the state policy consumer.
    pub expires_at_height: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SignedUcaSpendV1 {
    pub intent: UcaSpendIntentV1,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid ML-DSA-65 UCA spend encoding, key binding or bounds")]
pub struct SpendEncodingError;
type Result<T> = std::result::Result<T, SpendEncodingError>;

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_UCA_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

/// Asset-purpose key reference, intentionally distinct from the administrative
/// primary-key reference. Derivation alone does not grant spending permission.
pub fn spend_key_ref_v1(public_key: &[u8]) -> Result<[u8; 32]> {
    if public_key.len() != MLDSA65_PUBLIC_KEY_BYTES {
        return Err(SpendEncodingError);
    }
    let mut h = Sha256::new();
    h.update(b"novovm-uca-spend-key/mldsa65/v1\0");
    h.update(public_key);
    Ok(h.finalize().into())
}

impl UcaSpendIntentV1 {
    pub fn validate(&self) -> Result<()> {
        if self.chain_id == 0
            || self.genesis == [0; 32]
            || !valid_id(&self.account_id)
            || !valid_id(&self.recipient_uca)
            || self.authorization_epoch == 0
            || self.spend_key_ref == [0; 32]
            || self.expires_at_height <= self.not_before_height
            || self.expires_at_height - self.not_before_height > MAX_SPEND_WINDOW
            || self.nonce == u64::MAX
            || self.amount.checked_add(self.max_fee).is_none()
        {
            return Err(SpendEncodingError);
        }
        Ok(())
    }

    /// Unframed message for external-pure ML-DSA with SPEND_CONTEXT_V1. The
    /// existing exec signer adds FIPS 204's external-pure framing exactly once.
    /// All integers use big endian; identifiers use u8 length + exact ASCII.
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(
            MAX_SIGNED_SPEND_BYTES - MLDSA65_PUBLIC_KEY_BYTES - MLDSA65_SIGNATURE_BYTES,
        );
        out.extend_from_slice(HEADER);
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.genesis);
        out.push(self.account_id.len() as u8);
        out.extend_from_slice(self.account_id.as_bytes());
        out.extend_from_slice(&self.authorization_epoch.to_be_bytes());
        out.extend_from_slice(&self.spend_key_ref);
        out.push(self.recipient_uca.len() as u8);
        out.extend_from_slice(self.recipient_uca.as_bytes());
        out.extend_from_slice(&self.amount.to_be_bytes());
        out.extend_from_slice(&self.max_fee.to_be_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out.extend_from_slice(&self.not_before_height.to_be_bytes());
        out.extend_from_slice(&self.expires_at_height.to_be_bytes());
        Ok(out)
    }

    /// Stable spend replay domain: key/epoch/recipient/amount deliberately absent.
    /// Rotation or recovery therefore cannot create a fresh nonce bucket.
    pub fn nonce_identity(&self) -> Result<[u8; 32]> {
        self.validate()?;
        let mut h = Sha256::new();
        h.update(b"novovm-uca-nov-spend-nonce/v1\0");
        h.update(self.chain_id.to_be_bytes());
        h.update(self.genesis);
        h.update([self.account_id.len() as u8]);
        h.update(self.account_id.as_bytes());
        Ok(h.finalize().into())
    }
}

impl SignedUcaSpendV1 {
    pub fn validate(&self) -> Result<()> {
        self.intent.validate()?;
        if self.signature.len() != MLDSA65_SIGNATURE_BYTES
            || spend_key_ref_v1(&self.public_key)? != self.intent.spend_key_ref
        {
            return Err(SpendEncodingError);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = self.intent.signing_bytes()?;
        out.extend_from_slice(&self.public_key);
        out.extend_from_slice(&self.signature);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_SIGNED_SPEND_BYTES || !bytes.starts_with(HEADER) {
            return Err(SpendEncodingError);
        }
        let mut r = Reader(&bytes[HEADER.len()..]);
        let intent = UcaSpendIntentV1 {
            chain_id: u64::from_be_bytes(r.array()?),
            genesis: r.array()?,
            account_id: r.id()?,
            authorization_epoch: u64::from_be_bytes(r.array()?),
            spend_key_ref: r.array()?,
            recipient_uca: r.id()?,
            amount: u128::from_be_bytes(r.array()?),
            max_fee: u128::from_be_bytes(r.array()?),
            nonce: u64::from_be_bytes(r.array()?),
            not_before_height: u64::from_be_bytes(r.array()?),
            expires_at_height: u64::from_be_bytes(r.array()?),
        };
        intent.validate()?;
        // Check the entire fixed tail before allocating either crypto field.
        if r.0.len() != MLDSA65_PUBLIC_KEY_BYTES + MLDSA65_SIGNATURE_BYTES {
            return Err(SpendEncodingError);
        }
        let public_key = r.take(MLDSA65_PUBLIC_KEY_BYTES)?.to_vec();
        let signature = r.take(MLDSA65_SIGNATURE_BYTES)?.to_vec();
        let signed = Self {
            intent,
            public_key,
            signature,
        };
        signed.validate()?;
        Ok(signed)
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        if self.0.len() < count {
            return Err(SpendEncodingError);
        }
        let (value, rest) = self.0.split_at(count);
        self.0 = rest;
        Ok(value)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?.try_into().map_err(|_| SpendEncodingError)
    }
    fn id(&mut self) -> Result<String> {
        let len = self.array::<1>()?[0] as usize;
        if len == 0 || len > MAX_UCA_ID_BYTES {
            return Err(SpendEncodingError);
        }
        let value = std::str::from_utf8(self.take(len)?).map_err(|_| SpendEncodingError)?;
        if !valid_id(value) {
            return Err(SpendEncodingError);
        }
        Ok(value.to_owned())
    }
}

#[cfg(test)]
#[path = "uca_spend_tests.rs"]
mod tests;
