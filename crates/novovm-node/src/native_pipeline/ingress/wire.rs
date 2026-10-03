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

fn borrowed<'a>(tx: &'a TransferV3, signature: &'a [u8]) -> Result<Wire<'a>> {
    validate_accounts(&tx.from, &tx.to)?;
    Ok(Wire {
        chain_id: tx.chain_id,
        kind: Kind::Transfer(Transfer {
            from: &tx.from,
            to: &tx.to,
            asset: &tx.asset,
            amount: tx.amount,
            nonce: tx.nonce,
            fee_policy: Fee {
                pay_asset: &tx.fee_policy.pay_asset,
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
    ensure!(
        encode_borrowed(&wire)? == raw,
        "noncanonical native V3 Transfer"
    );
    Ok(TransferV3 {
        chain_id: wire.chain_id,
        from: transfer.from.to_vec(),
        to: transfer.to.to_vec(),
        asset: transfer.asset.to_owned(),
        amount: transfer.amount,
        nonce: transfer.nonce,
        fee_policy: FeePolicy {
            pay_asset: transfer.fee_policy.pay_asset.to_owned(),
            max_pay_amount: transfer.fee_policy.max_pay_amount,
            slippage_bps: transfer.fee_policy.slippage_bps,
        },
        signature: wire.signature.to_vec(),
    })
}

/// Encode the original wire. An empty signature is supported for wallet signing;
/// such a pre-signature encoding is NOT accepted by `decode_transfer_v3`.
pub fn encode_transfer_v3(tx: &TransferV3) -> Result<Vec<u8>> {
    ensure!(
        tx.signature.is_empty() || tx.signature.len() == SIGNATURE_BYTES,
        "native V3 signature must be empty or 96 bytes"
    );
    encode_borrowed(&borrowed(tx, &tx.signature)?)
}

/// Original complete-wire unsigned commitment, including an encoded empty
/// signature field. Signature bytes themselves are deliberately excluded.
pub fn unsigned_commitment(tx: &TransferV3) -> Result<[u8; 32]> {
    let bytes = encode_borrowed(&borrowed(tx, &[])?)?;
    let mut hash = Sha256::new();
    hash.update(UNSIGNED_DOMAIN);
    hash.update(u64::try_from(bytes.len())?.to_le_bytes());
    hash.update(bytes);
    Ok(hash.finalize().into())
}

fn signed_data(tx: &TransferV3) -> Result<Vec<u8>> {
    let size = INTENT_DOMAIN
        .len()
        .checked_add(32)
        .and_then(|n| n.checked_add(tx.asset.len()))
        .context("native signed intent length overflow")?;
    let mut data = Vec::with_capacity(size);
    data.extend_from_slice(INTENT_DOMAIN);
    data.extend_from_slice(&unsigned_commitment(tx)?);
    data.extend_from_slice(tx.asset.as_bytes());
    Ok(data)
}

fn hash_with_data(tx: &TransferV3, data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(&tx.from);
    hash.update(&tx.to);
    hash.update(tx.amount.to_le_bytes());
    hash.update(tx.nonce.to_le_bytes());
    hash.update(data);
    hash.finalize().into()
}

/// Original canonical TxIR hash after its V3 complete-intent envelope is added.
/// It is not SHA256(raw wire), and does not include the signature payload.
pub fn canonical_tx_hash(tx: &TransferV3) -> Result<[u8; 32]> {
    Ok(hash_with_data(tx, &signed_data(tx)?))
}

fn update_framed(hash: &mut Sha256, bytes: &[u8]) -> Result<()> {
    hash.update(u64::try_from(bytes.len())?.to_le_bytes());
    hash.update(bytes);
    Ok(())
}

/// Exact original adapter-v2 signing digest for a V3 Transfer. Do not replace
/// this with either `unsigned_commitment` or `canonical_tx_hash` when signing.
pub fn signing_message(tx: &TransferV3) -> Result<[u8; 32]> {
    let data = signed_data(tx)?;
    let canonical_hash = hash_with_data(tx, &data);
    let mut hash = Sha256::new();
    hash.update(SIGNING_DOMAIN);
    hash.update(tx.chain_id.to_le_bytes());
    hash.update([0]); // TxType::Transfer
    hash.update(tx.nonce.to_le_bytes());
    hash.update(tx.amount.to_le_bytes());
    hash.update(21_000_u64.to_le_bytes());
    hash.update(1_u64.to_le_bytes());
    update_framed(&mut hash, &tx.from)?;
    hash.update([0, 0, 0]); // account_id, fee_owner, nonce_owner: None
    hash.update([1]); // to: Some
    update_framed(&mut hash, &tx.to)?;
    update_framed(&mut hash, &data)?;
    hash.update([0]); // TxExecutionPolicyV1::Standard
    hash.update(0_u64.to_le_bytes()); // no EVM access list
    hash.update([0, 0]); // source_chain, target_chain: None
    update_framed(&mut hash, &canonical_hash)?;
    Ok(hash.finalize().into())
}

#[cfg(test)]
mod tests;
