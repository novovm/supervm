//! Shared ML-DSA-65 NOV asset-account spend intent. UCA identity lookup is not
//! transaction addressing. Encoding is not signature verification or authority.
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MLDSA65_PUBLIC_KEY_BYTES: usize = 1952;
pub const MLDSA65_SIGNATURE_BYTES: usize = 3309;
pub const SPEND_CONTEXT_V2: &[u8] = b"NOVOVM/asset/NOV-spend/v2";
pub const NATIVE_NOV_ASSET: u8 = 1;
// Identity management retains this bound; asset intents contain no UCA id.
pub const MAX_UCA_ID_BYTES: usize = 128;
pub const MAX_SPEND_WINDOW: u64 = 256;
pub const MAX_ASSET_GRANTS: usize = 8;
const MAX_AUTHORITY_UPDATE_BYTES: usize = 1 + 8 + 1 + 32 + 33 + 1 + MAX_ASSET_GRANTS * 81;
// NUAS, version=2, suite=65 (ML-DSA external pure), purpose=1 (NOV transfer).
// Version 1 UCA-addressed bytes MUST NOT be reinterpreted as asset accounts.
const HEADER: &[u8] = b"NUAS\x02\x41\x01";
pub const MAX_SIGNED_SPEND_BYTES: usize = 7
    + 8
    + 32
    + 33
    + 8
    + 32
    + 33
    + 1
    + 16
    + 16
    + 8
    + 8
    + 8
    + 1
    + MAX_AUTHORITY_UPDATE_BYTES
    + MLDSA65_PUBLIC_KEY_BYTES
    + MLDSA65_SIGNATURE_BYTES;
pub const MIN_SIGNED_SPEND_BYTES: usize = MAX_SIGNED_SPEND_BYTES - MAX_AUTHORITY_UPDATE_BYTES - 24;

/// Complete replacement grant set for an account-authorized management action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetGrantConfigV2 {
    pub key_ref: [u8; 32],
    pub permits_nov_transfer: bool,
    pub not_before_height: u64,
    pub expires_at_height: u64,
    pub max_debit_per_intent: u128,
    pub remaining_debit_budget: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetAuthorityUpdateV2 {
    pub use_recovery: bool,
    pub next_epoch: u64,
    pub active: bool,
    pub management_key_ref: [u8; 32],
    pub recovery_key_ref: Option<[u8; 32]>,
    pub grants: Vec<AssetGrantConfigV2>,
}

impl AssetAuthorityUpdateV2 {
    pub fn validate(&self) -> Result<()> {
        if self.next_epoch == 0
            || self.management_key_ref == [0; 32]
            || self.recovery_key_ref == Some([0; 32])
            || self.recovery_key_ref == Some(self.management_key_ref)
            || self.grants.len() > MAX_ASSET_GRANTS
        {
            return Err(SpendEncodingError);
        }
        let mut keys = std::collections::BTreeSet::new();
        for grant in &self.grants {
            if grant.key_ref == [0; 32]
                || grant.key_ref == self.management_key_ref
                || Some(grant.key_ref) == self.recovery_key_ref
                || !keys.insert(grant.key_ref)
                || grant.expires_at_height <= grant.not_before_height
                || grant.max_debit_per_intent == 0
                || grant.remaining_debit_budget == 0
            {
                return Err(SpendEncodingError);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSpendIntentV2 {
    pub chain_id: u64,
    pub genesis: [u8; 32],
    /// Stable concrete account. Key rotation never changes this balance/nonce owner.
    pub payer: Vec<u8>,
    pub authorization_epoch: u64,
    pub spend_key_ref: [u8; 32],
    pub recipient: Vec<u8>,
    /// This native transfer contract supports NOV only. Unknown assets fail closed.
    pub asset: u8,
    /// Native NOV base units (nine decimal places). No implicit foreign asset.
    pub amount: u128,
    pub max_fee: u128,
    pub nonce: u64,
    pub not_before_height: u64,
    /// Exclusive, checked against execution height by the state policy consumer.
    pub expires_at_height: u64,
    /// Account management uses the same native pipeline, never UCA identity authority.
    pub authority_update: Option<AssetAuthorityUpdateV2>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SignedAssetSpendV2 {
    pub intent: AssetSpendIntentV2,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid ML-DSA-65 asset spend encoding, key binding or bounds")]
pub struct SpendEncodingError;
type Result<T> = std::result::Result<T, SpendEncodingError>;

fn valid_account(value: &[u8]) -> bool {
    matches!(value.len(), 20 | 32)
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

impl AssetSpendIntentV2 {
    /// Actual retained heap, including spare capacity. No UCA material is held.
    pub fn retained_heap_bytes(&self) -> usize {
        self.payer
            .capacity()
            .saturating_add(self.recipient.capacity())
            .saturating_add(self.authority_update.as_ref().map_or(0, |update| {
                update
                    .grants
                    .capacity()
                    .saturating_mul(std::mem::size_of::<AssetGrantConfigV2>())
            }))
    }

    /// Decode the existing canonical intent, without claiming a signature or
    /// spending authority. Used by bounded native read requests.
    pub fn decode_signing_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_SIGNED_SPEND_BYTES - MLDSA65_PUBLIC_KEY_BYTES - MLDSA65_SIGNATURE_BYTES
            || !bytes.starts_with(HEADER)
        {
            return Err(SpendEncodingError);
        }
        let mut reader = Reader(&bytes[HEADER.len()..]);
        let value = reader.intent()?;
        if !reader.0.is_empty() {
            return Err(SpendEncodingError);
        }
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        if self.chain_id == 0
            || self.genesis == [0; 32]
            || !valid_account(&self.payer)
            || !valid_account(&self.recipient)
            || self.asset != NATIVE_NOV_ASSET
            || self.authorization_epoch == 0
            || self.spend_key_ref == [0; 32]
            || self.expires_at_height <= self.not_before_height
            || self.expires_at_height - self.not_before_height > MAX_SPEND_WINDOW
            || self.nonce == u64::MAX
            || self.amount.checked_add(self.max_fee).is_none()
        {
            return Err(SpendEncodingError);
        }
        if let Some(update) = &self.authority_update {
            update.validate()?;
            if self.amount != 0
                || self.recipient != self.payer
                || self.authorization_epoch.checked_add(1) != Some(update.next_epoch)
            {
                return Err(SpendEncodingError);
            }
        }
        Ok(())
    }

    /// Unframed message for external-pure ML-DSA with SPEND_CONTEXT_V2. The
    /// existing exec signer adds FIPS 204's external-pure framing exactly once.
    /// All integers use big endian; accounts use u8 length + exact bytes.
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(
            MAX_SIGNED_SPEND_BYTES - MLDSA65_PUBLIC_KEY_BYTES - MLDSA65_SIGNATURE_BYTES,
        );
        out.extend_from_slice(HEADER);
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.genesis);
        out.push(self.payer.len() as u8);
        out.extend_from_slice(&self.payer);
        out.extend_from_slice(&self.authorization_epoch.to_be_bytes());
        out.extend_from_slice(&self.spend_key_ref);
        out.push(self.recipient.len() as u8);
        out.extend_from_slice(&self.recipient);
        out.push(self.asset);
        out.extend_from_slice(&self.amount.to_be_bytes());
        out.extend_from_slice(&self.max_fee.to_be_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out.extend_from_slice(&self.not_before_height.to_be_bytes());
        out.extend_from_slice(&self.expires_at_height.to_be_bytes());
        out.extend_from_slice(&encode_authority_update_v2(&self.authority_update)?);
        Ok(out)
    }

    /// Stable spend replay domain: key/epoch/recipient/amount deliberately absent.
    /// Rotation or recovery therefore cannot create a fresh nonce bucket.
    pub fn nonce_identity(&self) -> Result<[u8; 32]> {
        self.validate()?;
        let mut h = Sha256::new();
        h.update(b"novovm-asset-spend-nonce/v2\0");
        h.update(self.chain_id.to_be_bytes());
        h.update(self.genesis);
        h.update([self.payer.len() as u8]);
        h.update(&self.payer);
        Ok(h.finalize().into())
    }
}

impl SignedAssetSpendV2 {
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
        let intent = r.intent()?;
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
    fn intent(&mut self) -> Result<AssetSpendIntentV2> {
        let value = AssetSpendIntentV2 {
            chain_id: u64::from_be_bytes(self.array()?),
            genesis: self.array()?,
            payer: self.account()?,
            authorization_epoch: u64::from_be_bytes(self.array()?),
            spend_key_ref: self.array()?,
            recipient: self.account()?,
            asset: self.array::<1>()?[0],
            amount: u128::from_be_bytes(self.array()?),
            max_fee: u128::from_be_bytes(self.array()?),
            nonce: u64::from_be_bytes(self.array()?),
            not_before_height: u64::from_be_bytes(self.array()?),
            expires_at_height: u64::from_be_bytes(self.array()?),
            authority_update: self.authority_update()?,
        };
        value.validate()?;
        Ok(value)
    }

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
    fn account(&mut self) -> Result<Vec<u8>> {
        let len = self.array::<1>()?[0] as usize;
        if !matches!(len, 20 | 32) {
            return Err(SpendEncodingError);
        }
        Ok(self.take(len)?.to_vec())
    }

    fn boolean(&mut self) -> Result<bool> {
        match self.array::<1>()?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(SpendEncodingError),
        }
    }

    fn authority_update(&mut self) -> Result<Option<AssetAuthorityUpdateV2>> {
        if !self.boolean()? {
            return Ok(None);
        }
        let use_recovery = self.boolean()?;
        let next_epoch = u64::from_be_bytes(self.array()?);
        let active = self.boolean()?;
        let management_key_ref = self.array()?;
        let recovery_key_ref = if self.boolean()? {
            Some(self.array()?)
        } else {
            None
        };
        let count = self.array::<1>()?[0] as usize;
        if count > MAX_ASSET_GRANTS || count > self.0.len() / 81 {
            return Err(SpendEncodingError);
        }
        let mut grants = Vec::with_capacity(count);
        for _ in 0..count {
            grants.push(AssetGrantConfigV2 {
                key_ref: self.array()?,
                permits_nov_transfer: self.boolean()?,
                not_before_height: u64::from_be_bytes(self.array()?),
                expires_at_height: u64::from_be_bytes(self.array()?),
                max_debit_per_intent: u128::from_be_bytes(self.array()?),
                remaining_debit_budget: u128::from_be_bytes(self.array()?),
            });
        }
        let update = AssetAuthorityUpdateV2 {
            use_recovery,
            next_epoch,
            active,
            management_key_ref,
            recovery_key_ref,
            grants,
        };
        update.validate()?;
        Ok(Some(update))
    }
}

pub(crate) fn encode_authority_update_v2(
    value: &Option<AssetAuthorityUpdateV2>,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let Some(value) = value else {
        out.push(0);
        return Ok(out);
    };
    value.validate()?;
    out.push(1);
    out.push(u8::from(value.use_recovery));
    out.extend_from_slice(&value.next_epoch.to_be_bytes());
    out.push(u8::from(value.active));
    out.extend_from_slice(&value.management_key_ref);
    out.push(u8::from(value.recovery_key_ref.is_some()));
    if let Some(key) = value.recovery_key_ref {
        out.extend_from_slice(&key);
    }
    out.push(value.grants.len() as u8);
    for grant in &value.grants {
        out.extend_from_slice(&grant.key_ref);
        out.push(u8::from(grant.permits_nov_transfer));
        out.extend_from_slice(&grant.not_before_height.to_be_bytes());
        out.extend_from_slice(&grant.expires_at_height.to_be_bytes());
        out.extend_from_slice(&grant.max_debit_per_intent.to_be_bytes());
        out.extend_from_slice(&grant.remaining_debit_budget.to_be_bytes());
    }
    Ok(out)
}

pub(crate) fn decode_authority_update_v2(bytes: &[u8]) -> Result<Option<AssetAuthorityUpdateV2>> {
    if bytes.len() > 1 + MAX_AUTHORITY_UPDATE_BYTES {
        return Err(SpendEncodingError);
    }
    let mut reader = Reader(bytes);
    let update = reader.authority_update()?;
    if !reader.0.is_empty() {
        return Err(SpendEncodingError);
    }
    Ok(update)
}

#[cfg(test)]
#[path = "uca_spend_tests.rs"]
mod tests;
