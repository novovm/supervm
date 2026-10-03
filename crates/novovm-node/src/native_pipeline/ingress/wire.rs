//! The existing native `NNX1 / 3` Transfer wire, not a new transaction format.
//!
//! Reviewed local migration from the archived protocol's `tx_wire.rs`, node's
//! `nov_native_tx_to_adapter_tx_ir_v1`, and adapter API's `ir::compute_hash` and
//! `native_signing::tx_signing_message_v1`. No legacy crate or general TxIR is
//! imported. In particular, the full fee policy remains in the signed intent.
//!
//! Decoding here is a NEW strict Transfer admission boundary: it rejects old
//! versions, other transaction kinds, trailing bytes and noncanonical encodings.
//! The old general postcard decoder did not enforce all of these restrictions.
//! Transfer has no execution/privacy/verification mode fields. Execute and
//! Governance are rejected, never interpreted as a Transfer with default modes.
//!
//! These public values are unauthenticated. The digests do not verify a signer,
//! balance, fee, nonce, or chain authority. V3 binds `chain_id`, NOT a genesis
//! commitment; this migration does not add replay protection between genesis
//! configurations using the same chain id.

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const HEADER: &[u8; 5] = b"NNX1\x03";
const UNSIGNED_DOMAIN: &[u8] = b"novovm-native-tx-unsigned-commitment-v3";
const INTENT_DOMAIN: &[u8] = b"novovm-native-signed-intent-v3\0";
const SIGNING_DOMAIN: &[u8] = b"novovm_adapter_tx_sig_v2";
const SIGNATURE_BYTES: usize = 96;

/// Signed fee-policy fields; their economic interpretation is not a codec job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeePolicy {
    pub pay_asset: String,
    pub max_pay_amount: u128,
    pub slippage_bps: u32,
}

/// An owned, unverified Transfer. `signature` is public key (32) + signature (64).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferV3 {
    pub chain_id: u64,
    pub from: Vec<u8>,
    pub to: Vec<u8>,
    pub asset: String,
    pub amount: u128,
    pub nonce: u64,
    pub fee_policy: FeePolicy,
    pub signature: Vec<u8>,
}

/// Borrowed signed fields. Neither a view nor its hashes authenticate a signer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeePolicyView<'a> {
    pub pay_asset: &'a str,
    pub max_pay_amount: u128,
    pub slippage_bps: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferView<'a> {
    pub chain_id: u64,
    pub from: &'a [u8],
    pub to: &'a [u8],
    pub asset: &'a str,
    pub amount: u128,
    pub nonce: u64,
    pub fee_policy: FeePolicyView<'a>,
    pub signature: &'a [u8],
}

impl TransferV3 {
    pub fn as_view(&self) -> TransferView<'_> {
        TransferView {
            chain_id: self.chain_id,
            from: &self.from,
            to: &self.to,
            asset: &self.asset,
            amount: self.amount,
            nonce: self.nonce,
            fee_policy: FeePolicyView {
                pay_asset: &self.fee_policy.pay_asset,
                max_pay_amount: self.fee_policy.max_pay_amount,
                slippage_bps: self.fee_policy.slippage_bps,
            },
            signature: &self.signature,
        }
    }
}

// These private borrowed mirrors deliberately preserve the original postcard
// field order and Transfer's enum tag 0. Borrowed slices/str inspect declared
// lengths against the input before any owned field allocation. The one-variant
// enum rejects every other tag before trying to decode its body.
#[derive(Serialize, Deserialize)]
struct Wire<'a> {
    chain_id: u64,
    #[serde(borrow)]
    kind: Kind<'a>,
    signature: &'a [u8],
}

#[derive(Serialize, Deserialize)]
enum Kind<'a> {
    Transfer(#[serde(borrow)] Transfer<'a>),
}

#[derive(Serialize, Deserialize)]
struct Transfer<'a> {
    from: &'a [u8],
    to: &'a [u8],
    asset: &'a str,
    amount: u128,
    nonce: u64,
    fee_policy: Fee<'a>,
}

#[derive(Serialize, Deserialize)]
struct Fee<'a> {
    pay_asset: &'a str,
    max_pay_amount: u128,
    slippage_bps: u32,
}

fn validate_accounts(from: &[u8], to: &[u8]) -> Result<()> {
    ensure!(
        matches!(from.len(), 20 | 32),
        "invalid Transfer from length"
    );
    ensure!(matches!(to.len(), 20 | 32), "invalid Transfer to length");
    Ok(())
}

fn borrowed<'a>(tx: TransferView<'a>, signature: &'a [u8]) -> Result<Wire<'a>> {
    validate_accounts(tx.from, tx.to)?;
    Ok(Wire {
        chain_id: tx.chain_id,
        kind: Kind::Transfer(Transfer {
            from: tx.from,
            to: tx.to,
            asset: tx.asset,
            amount: tx.amount,
            nonce: tx.nonce,
            fee_policy: Fee {
                pay_asset: tx.fee_policy.pay_asset,
                max_pay_amount: tx.fee_policy.max_pay_amount,
                slippage_bps: tx.fee_policy.slippage_bps,
            },
        }),
        signature,
    })
}

fn encode_borrowed(wire: &Wire<'_>) -> Result<Vec<u8>> {
    let payload = postcard::to_allocvec(wire).context("encode native V3 Transfer")?;
    let size = HEADER
        .len()
        .checked_add(payload.len())
        .context("native V3 Transfer length overflow")?;
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(HEADER);
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

/// Decode only an exactly encoded, signed V3 Transfer within `max_bytes`.
/// The limit applies BEFORE parsing. Strings and byte fields remain borrowed
/// until the complete body, field sizes and canonical encoding have passed.
pub fn decode_transfer_v3(raw: &[u8], max_bytes: usize) -> Result<TransferV3> {
    Ok(decode_transfer_view_v3(raw, max_bytes)?.into_owned())
}

// Compare canonical serialization without allocating an expanded transaction.
struct Canonical<'a>(&'a [u8]);
impl postcard::ser_flavors::Flavor for Canonical<'_> {
    type Output = ();
    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        self.try_extend(&[byte])
    }
    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if !self.0.starts_with(bytes) {
            return Err(postcard::Error::SerializeBufferFull);
        }
        self.0 = &self.0[bytes.len()..];
        Ok(())
    }
    fn finalize(self) -> postcard::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(postcard::Error::SerializeBufferFull)
        }
    }
}

// Coalesce postcard's small scalar writes on the stack. This flavor emits the
// very same bytes as the owned encoder without allocating a transaction Vec.
struct CanonicalDigest<'a> {
    digest: &'a mut Sha256,
    buffer: [u8; 256],
    used: usize,
}

impl CanonicalDigest<'_> {
    fn flush(&mut self) {
        if self.used != 0 {
            self.digest.update(&self.buffer[..self.used]);
            self.used = 0;
        }
    }
}

impl postcard::ser_flavors::Flavor for CanonicalDigest<'_> {
    type Output = ();

    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        self.buffer[self.used] = byte;
        self.used += 1;
        if self.used == self.buffer.len() {
            self.flush();
        }
        Ok(())
    }

    fn try_extend(&mut self, mut bytes: &[u8]) -> postcard::Result<()> {
        let prefix = bytes.len().min(self.buffer.len() - self.used);
        self.buffer[self.used..self.used + prefix].copy_from_slice(&bytes[..prefix]);
        self.used += prefix;
        bytes = &bytes[prefix..];
        if self.used == self.buffer.len() {
            self.flush();
        }
        if bytes.len() >= self.buffer.len() {
            self.digest.update(bytes);
        } else if !bytes.is_empty() {
            self.buffer[..bytes.len()].copy_from_slice(bytes);
            self.used = bytes.len();
        }
        Ok(())
    }

    fn finalize(mut self) -> postcard::Result<()> {
        self.flush();
        Ok(())
    }
}

/// Allocation-free strict canonical V3 decode; references remain tied to raw.
pub fn decode_transfer_view_v3(raw: &[u8], max_bytes: usize) -> Result<TransferView<'_>> {
    ensure!(
        raw.len() <= max_bytes,
        "native V3 Transfer exceeds byte limit"
    );
    ensure!(raw.starts_with(HEADER), "expected native NNX1 V3 Transfer");
    let (wire, remaining): (Wire<'_>, _) =
        postcard::take_from_bytes(&raw[HEADER.len()..]).context("decode native V3 Transfer")?;
    ensure!(
        remaining.is_empty(),
        "native V3 Transfer has trailing bytes"
    );
    let Kind::Transfer(transfer) = &wire.kind;
    validate_accounts(transfer.from, transfer.to)?;
    ensure!(
        wire.signature.len() == SIGNATURE_BYTES,
        "expected 96-byte native signature"
    );
    postcard::serialize_with_flavor(&wire, Canonical(&raw[HEADER.len()..]))
        .context("noncanonical native V3 Transfer")?;
    Ok(TransferView {
        chain_id: wire.chain_id,
        from: transfer.from,
        to: transfer.to,
        asset: transfer.asset,
        amount: transfer.amount,
        nonce: transfer.nonce,
        fee_policy: FeePolicyView {
            pay_asset: transfer.fee_policy.pay_asset,
            max_pay_amount: transfer.fee_policy.max_pay_amount,
            slippage_bps: transfer.fee_policy.slippage_bps,
        },
        signature: wire.signature,
    })
}

/// Encode the original wire. An empty signature is supported for wallet signing;
/// such a pre-signature encoding is NOT accepted by `decode_transfer_v3`.
pub fn encode_transfer_v3(tx: &TransferV3) -> Result<Vec<u8>> {
    tx.as_view().encode()
}

impl TransferView<'_> {
    pub fn into_owned(self) -> TransferV3 {
        TransferV3 {
            chain_id: self.chain_id,
            from: self.from.to_vec(),
            to: self.to.to_vec(),
            asset: self.asset.to_owned(),
            amount: self.amount,
            nonce: self.nonce,
            fee_policy: FeePolicy {
                pay_asset: self.fee_policy.pay_asset.to_owned(),
                max_pay_amount: self.fee_policy.max_pay_amount,
                slippage_bps: self.fee_policy.slippage_bps,
            },
            signature: self.signature.to_vec(),
        }
    }

    pub fn encode(self) -> Result<Vec<u8>> {
        ensure!(
            self.signature.is_empty() || self.signature.len() == SIGNATURE_BYTES,
            "native V3 signature must be empty or 96 bytes"
        );
        encode_borrowed(&borrowed(self, self.signature)?)
    }

    /// Exact original V3 size without materializing its bytes.
    pub fn encoded_len(self) -> Result<usize> {
        let size: usize = postcard::serialize_with_flavor(
            &borrowed(self, self.signature)?,
            postcard::ser_flavors::Size::default(),
        )?;
        HEADER
            .len()
            .checked_add(size)
            .context("native V3 size overflow")
    }

    /// Compare the COMPLETE canonical V3 encoding, including its signature,
    /// without materializing or decoding another copy. A match is not signature
    /// verification; callers may reuse only an independently verified input.
    /// Like `encode`, an empty wallet pre-signature is representable here.
    pub(crate) fn matches_canonical_bytes(self, raw: &[u8]) -> Result<bool> {
        ensure!(
            self.signature.is_empty() || self.signature.len() == SIGNATURE_BYTES,
            "native V3 signature must be empty or 96 bytes"
        );
        let wire = borrowed(self, self.signature)?;
        let Some(payload) = raw.strip_prefix(HEADER.as_slice()) else {
            return Ok(false);
        };
        match postcard::serialize_with_flavor(&wire, Canonical(payload)) {
            Ok(()) => Ok(true),
            // The comparison flavor uses this only for unequal/truncated/tail
            // bytes; other serialization failures are not treated as matches.
            Err(postcard::Error::SerializeBufferFull) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// Append only the original canonical V3 wire to an existing SHA256 state.
    /// No domain, length or transaction identity is added here: the caller owns
    /// that framing. This is byte projection, NOT authentication or a new hash.
    pub(crate) fn update_canonical_digest(self, digest: &mut Sha256) -> Result<()> {
        ensure!(
            self.signature.is_empty() || self.signature.len() == SIGNATURE_BYTES,
            "native V3 signature must be empty or 96 bytes"
        );
        let wire = borrowed(self, self.signature)?;
        digest.update(HEADER);
        postcard::serialize_with_flavor(
            &wire,
            CanonicalDigest {
                digest,
                buffer: [0; 256],
                used: 0,
            },
        )
        .context("stream canonical native V3 Transfer")?;
        Ok(())
    }

    /// Original complete-wire unsigned commitment, including an encoded empty
    /// signature field. Signature bytes themselves are deliberately excluded.
    pub fn unsigned_commitment(self) -> Result<[u8; 32]> {
        let bytes = encode_borrowed(&borrowed(self, &[])?)?;
        let mut hash = Sha256::new();
        hash.update(UNSIGNED_DOMAIN);
        hash.update(u64::try_from(bytes.len())?.to_le_bytes());
        hash.update(bytes);
        Ok(hash.finalize().into())
    }

    fn signed_data(self) -> Result<Vec<u8>> {
        let size = INTENT_DOMAIN
            .len()
            .checked_add(32)
            .and_then(|n| n.checked_add(self.asset.len()))
            .context("native signed intent length overflow")?;
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(INTENT_DOMAIN);
        data.extend_from_slice(&self.unsigned_commitment()?);
        data.extend_from_slice(self.asset.as_bytes());
        Ok(data)
    }

    fn hash_with_data(self, data: &[u8]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(self.from);
        hash.update(self.to);
        hash.update(self.amount.to_le_bytes());
        hash.update(self.nonce.to_le_bytes());
        hash.update(data);
        hash.finalize().into()
    }

    /// Original canonical TxIR hash after its V3 complete-intent envelope is added.
    /// It is not SHA256(raw wire), and does not include the signature payload.
    pub fn canonical_tx_hash(self) -> Result<[u8; 32]> {
        Ok(self.hash_with_data(&self.signed_data()?))
    }

    /// Exact original adapter-v2 signing digest for a V3 Transfer. Do not replace
    /// this with either `unsigned_commitment` or `canonical_tx_hash` when signing.
    pub fn signing_message(self) -> Result<[u8; 32]> {
        let data = self.signed_data()?;
        let canonical_hash = self.hash_with_data(&data);
        let mut hash = Sha256::new();
        hash.update(SIGNING_DOMAIN);
        hash.update(self.chain_id.to_le_bytes());
        hash.update([0]); // TxType::Transfer
        hash.update(self.nonce.to_le_bytes());
        hash.update(self.amount.to_le_bytes());
        hash.update(21_000_u64.to_le_bytes());
        hash.update(1_u64.to_le_bytes());
        update_framed(&mut hash, self.from)?;
        hash.update([0, 0, 0]); // account_id, fee_owner, nonce_owner: None
        hash.update([1]); // to: Some
        update_framed(&mut hash, self.to)?;
        update_framed(&mut hash, &data)?;
        hash.update([0]); // TxExecutionPolicyV1::Standard
        hash.update(0_u64.to_le_bytes()); // no EVM access list
        hash.update([0, 0]); // source_chain, target_chain: None
        update_framed(&mut hash, &canonical_hash)?;
        Ok(hash.finalize().into())
    }
}

fn update_framed(hash: &mut Sha256, bytes: &[u8]) -> Result<()> {
    hash.update(u64::try_from(bytes.len())?.to_le_bytes());
    hash.update(bytes);
    Ok(())
}

pub fn unsigned_commitment(tx: &TransferV3) -> Result<[u8; 32]> {
    tx.as_view().unsigned_commitment()
}
pub fn canonical_tx_hash(tx: &TransferV3) -> Result<[u8; 32]> {
    tx.as_view().canonical_tx_hash()
}
pub fn signing_message(tx: &TransferV3) -> Result<[u8; 32]> {
    tx.as_view().signing_message()
}

#[cfg(test)]
mod tests;
