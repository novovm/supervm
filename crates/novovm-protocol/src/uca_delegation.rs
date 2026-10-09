//! One-intent UCA delegation envelope for reviewed native integration.
//! This is NOT a new accepted native wire version or a network/chat credential.
//! Account/root associations must not be disclosed to chat peers or relays.
use thiserror::Error;

const MAGIC: &[u8; 5] = b"NUCD\x01";
const DOMAIN: &[u8] = b"novovm-uca-native-intent-delegation-v1\0";
pub const MAX_ACCOUNT_BYTES: usize = 128;
pub const MAX_WINDOW_BLOCKS: u64 = 256;
pub const MAX_ENVELOPE_BYTES: usize = 5 + 1 + 8 + 32 + 1 + 128 + 8 + 32 + 32 + 32 + 8 + 8 + 64;

/// Only administrative identity purposes. No transfer/spending permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UcaDelegationPurposeV1 {
    AuthorizeDevice = 1,
    RevokeDevice = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UcaDelegationClaimsV1 {
    pub chain_id: u64,
    pub genesis: [u8; 32],
    pub account_id: String,
    pub account_epoch: u64,
    pub app_scope: [u8; 32],
    /// Administrative transaction signer, never the peer-visible chat identity.
    pub delegate_public_key: [u8; 32],
    /// Existing native_tx_unsigned_commitment_v3, including nonce and fee intent.
    pub native_intent: [u8; 32],
    pub not_before_height: u64,
    pub expires_at_height: u64,
    pub purpose: UcaDelegationPurposeV1,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SignedUcaDelegationV1 {
    pub claims: UcaDelegationClaimsV1,
    pub signature: [u8; 64],
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid UCA native delegation encoding or bounds")]
pub struct UcaDelegationEncodingError;
type Result<T> = std::result::Result<T, UcaDelegationEncodingError>;

impl UcaDelegationClaimsV1 {
    pub fn validate(&self) -> Result<()> {
        if self.chain_id == 0
            || self.genesis == [0; 32]
            || self.account_epoch == 0
            || self.app_scope == [0; 32]
            || self.delegate_public_key == [0; 32]
            || self.native_intent == [0; 32]
            || self.account_id.is_empty()
            || self.account_id.len() > MAX_ACCOUNT_BYTES
            || !self
                .account_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
            || self.expires_at_height <= self.not_before_height
            || self.expires_at_height - self.not_before_height > MAX_WINDOW_BLOCKS
        {
            return Err(UcaDelegationEncodingError);
        }
        Ok(())
    }

    fn encoded(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(MAX_ENVELOPE_BYTES);
        out.extend_from_slice(MAGIC);
        out.push(self.purpose as u8);
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.genesis);
        out.push(self.account_id.len() as u8);
        out.extend_from_slice(self.account_id.as_bytes());
        out.extend_from_slice(&self.account_epoch.to_be_bytes());
        out.extend_from_slice(&self.app_scope);
        out.extend_from_slice(&self.delegate_public_key);
        out.extend_from_slice(&self.native_intent);
        out.extend_from_slice(&self.not_before_height.to_be_bytes());
        out.extend_from_slice(&self.expires_at_height.to_be_bytes());
        Ok(out)
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        let mut out = DOMAIN.to_vec();
        out.extend_from_slice(&self.encoded()?);
        Ok(out)
    }
}

impl SignedUcaDelegationV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut out = self.claims.encoded()?;
        out.extend_from_slice(&self.signature);
        Ok(out)
    }

    pub fn decode(input: &[u8]) -> Result<Self> {
        if input.len() > MAX_ENVELOPE_BYTES || !input.starts_with(MAGIC) {
            return Err(UcaDelegationEncodingError);
        }
        struct Reader<'a>(&'a [u8]);
        impl Reader<'_> {
            fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
                if self.0.len() < N {
                    return Err(UcaDelegationEncodingError);
                }
                let (head, tail) = self.0.split_at(N);
                self.0 = tail;
                head.try_into().map_err(|_| UcaDelegationEncodingError)
            }
        }
        let mut r = Reader(&input[5..]);
        let purpose = match r.take::<1>()?[0] {
            1 => UcaDelegationPurposeV1::AuthorizeDevice,
            2 => UcaDelegationPurposeV1::RevokeDevice,
            _ => return Err(UcaDelegationEncodingError),
        };
        let chain_id = u64::from_be_bytes(r.take()?);
        let genesis = r.take()?;
        let length = usize::from(r.take::<1>()?[0]);
        if length == 0 || length > MAX_ACCOUNT_BYTES || r.0.len() < length {
            return Err(UcaDelegationEncodingError);
        }
        let account_id = std::str::from_utf8(&r.0[..length])
            .map_err(|_| UcaDelegationEncodingError)?
            .to_owned();
        r.0 = &r.0[length..];
        let claims = UcaDelegationClaimsV1 {
            chain_id,
            genesis,
            account_id,
            account_epoch: u64::from_be_bytes(r.take()?),
            app_scope: r.take()?,
            delegate_public_key: r.take()?,
            native_intent: r.take()?,
            not_before_height: u64::from_be_bytes(r.take()?),
            expires_at_height: u64::from_be_bytes(r.take()?),
            purpose,
        };
        let signature = r.take()?;
        if !r.0.is_empty() {
            return Err(UcaDelegationEncodingError);
        }
        claims.validate()?;
        Ok(Self { claims, signature })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn fixed_cross_language_encoding_vector() {
        let proof = SignedUcaDelegationV1 {
            claims: UcaDelegationClaimsV1 {
                chain_id: 1,
                genesis: [2; 32],
                account_id: "a".into(),
                account_epoch: 1,
                app_scope: [3; 32],
                delegate_public_key: [4; 32],
                native_intent: [5; 32],
                not_before_height: 10,
                expires_at_height: 20,
                purpose: UcaDelegationPurposeV1::AuthorizeDevice,
            },
            signature: [6; 64],
        };
        let bytes = proof.encode().unwrap();
        assert_eq!(bytes.len(), 232);
        // Digest generated independently with Python struct.pack big-endian fields.
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "db9ce4a2864523e9ffc24a23e944346b100da6267a30168e6bd0b69c54616865"
        );
        assert!(SignedUcaDelegationV1::decode(&bytes).unwrap() == proof);
    }
}
