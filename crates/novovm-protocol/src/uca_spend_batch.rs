//! Lossless APFL columns for canonical NUAS v3 transactions. Concrete accounts
//! are signed fields, not external hints. This is not signature/authority proof.
use crate::apfl_column::Column;
use crate::uca_spend::{
    decode_asset_action_v2, encode_asset_action_v2, AssetSpendAuthorityV3, AssetSpendIntentV3,
    SignedAssetSpendV3, SpendEncodingError, MAX_SIGNED_SPEND_BYTES, MIN_SIGNED_SPEND_BYTES,
    MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES, NATIVE_NOV_ASSET,
};
use sha2::Digest;
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"NUAPFL03";
const VERSION: u16 = 3;
const FIELDS: usize = 15;
const MIN_RAW: usize = MIN_SIGNED_SPEND_BYTES;
type Result<T> = std::result::Result<T, SpendEncodingError>;

#[derive(Clone, Copy, Debug)]
pub struct AssetSpendBatchLimitsV3 {
    pub transactions: usize,
    pub transaction_bytes: usize,
    /// Expanded canonical NUAS bytes. Owned metadata is separately bounded by
    /// row count and must also be charged by the retaining owner.
    pub body_bytes: usize,
}

impl AssetSpendBatchLimitsV3 {
    fn check(self, rows: usize, total: usize, max: usize) -> Result<()> {
        if rows == 0
            || rows > self.transactions
            || u32::try_from(rows).is_err()
            || max < MIN_RAW
            || max > self.transaction_bytes.min(MAX_SIGNED_SPEND_BYTES)
            || total > self.body_bytes
            || total < max
            || rows.checked_mul(MIN_RAW).is_none_or(|n| n > total)
        {
            return Err(SpendEncodingError);
        }
        Ok(())
    }
}

/// Each column is a typed signed field: chain, genesis, payer, epoch, key-ref,
/// recipient, amount, max-fee, nonce, start, expiry, public-key, asset, authority
/// update, authority mode. Numeric cells retain NUAS big endian byte order.
/// The outer column framing uses little endian as the existing APFL codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetSpendBatchV3 {
    rows: usize,
    columns: [Column<Vec<u8>>; FIELDS],
    signatures: Vec<u8>,
    canonical_bytes: usize,
    max_transaction_bytes: usize,
}

/// Borrowed, structurally validated row of an immutable batch. This is not a
/// signature or spending capability. Private fields prevent callers from
/// constructing an unchecked view or substituting a row's signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetSpendViewV3<'a> {
    cells: [&'a [u8]; FIELDS],
    signature: &'a [u8],
}

impl AssetSpendViewV3<'_> {
    /// Decode the intent while leaving the public key and signature borrowed.
    /// Reading a column view is not signature or spending authorization.
    pub fn intent(&self) -> Result<AssetSpendIntentV3> {
        let cell = |field: usize| self.cells[field];
        let array8 = |field| cell(field).try_into().map_err(|_| SpendEncodingError);
        let (authority_update, recipient_creation) = decode_asset_action_v2(cell(13))?;
        Ok(AssetSpendIntentV3 {
            authority: AssetSpendAuthorityV3::try_from(cell(14)[0])?,
            chain_id: u64::from_be_bytes(array8(0)?),
            genesis: cell(1).try_into().map_err(|_| SpendEncodingError)?,
            payer: cell(2).to_vec(),
            authorization_epoch: u64::from_be_bytes(array8(3)?),
            spend_key_ref: cell(4).try_into().map_err(|_| SpendEncodingError)?,
            recipient: cell(5).to_vec(),
            asset: cell(12)[0],
            amount: u128::from_be_bytes(cell(6).try_into().map_err(|_| SpendEncodingError)?),
            max_fee: u128::from_be_bytes(cell(7).try_into().map_err(|_| SpendEncodingError)?),
            nonce: u64::from_be_bytes(array8(8)?),
            not_before_height: u64::from_be_bytes(array8(9)?),
            expires_at_height: u64::from_be_bytes(array8(10)?),
            authority_update,
            recipient_creation,
        })
    }

    /// Original canonical intent encoding, without copying the key/signature.
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.intent()?.signing_bytes()
    }

    /// Same canonical signed bytes as `SignedAssetSpendV3::encode`, without
    /// allocating a second public key/signature buffer for plan commitments.
    pub fn update_canonical_digest(&self, digest: &mut impl Digest) -> Result<()> {
        digest.update(self.signing_bytes()?);
        digest.update(self.cells[11]);
        digest.update(self.signature);
        Ok(())
    }

    /// Compare every canonical field, public key and signature directly. An
    /// intent hash alone cannot establish that a signature fact is reusable.
    pub fn matches_signed(&self, signed: &SignedAssetSpendV3) -> Result<bool> {
        let i = &signed.intent;
        Ok(self.cells[0] == i.chain_id.to_be_bytes()
            && self.cells[1] == i.genesis
            && self.cells[2] == i.payer
            && self.cells[3] == i.authorization_epoch.to_be_bytes()
            && self.cells[4] == i.spend_key_ref
            && self.cells[5] == i.recipient
            && self.cells[6] == i.amount.to_be_bytes()
            && self.cells[7] == i.max_fee.to_be_bytes()
            && self.cells[8] == i.nonce.to_be_bytes()
            && self.cells[9] == i.not_before_height.to_be_bytes()
            && self.cells[10] == i.expires_at_height.to_be_bytes()
            && self.cells[11] == signed.public_key
            && self.cells[12] == [i.asset]
            && self.cells[13]
                == encode_asset_action_v2(&i.authority_update, &i.recipient_creation)?
            && self.cells[14] == [i.authority as u8]
            && self.signature == signed.signature)
    }

    fn encoded_len(&self) -> usize {
        MIN_RAW + self.cells[2].len() + self.cells[5].len() - 40 + self.cells[13].len() - 1
    }
}

impl AssetSpendBatchV3 {
    /// Rebatch existing validated columns without expanding, encoding and
    /// reparsing transactions. The selected order and all signature bytes are
    /// preserved; the new retaining owner must still reserve its own budget.
    pub fn from_views(
        views: &[AssetSpendViewV3<'_>],
        limits: AssetSpendBatchLimitsV3,
    ) -> Result<Self> {
        let total = views.iter().try_fold(0usize, |n, row| {
            n.checked_add(row.encoded_len()).ok_or(SpendEncodingError)
        })?;
        let max = views
            .iter()
            .map(AssetSpendViewV3::encoded_len)
            .max()
            .unwrap_or(0);
        limits.check(views.len(), total, max)?;
        let mut signatures = Vec::with_capacity(
            views
                .len()
                .checked_mul(MLDSA65_SIGNATURE_BYTES)
                .ok_or(SpendEncodingError)?,
        );
        for row in views {
            signatures.extend_from_slice(row.signature);
        }
        Ok(Self {
            rows: views.len(),
            columns: std::array::from_fn(|field| {
                Column::collect(views.iter().map(|row| row.cells[field])).map(<[u8]>::to_vec)
            }),
            signatures,
            canonical_bytes: total,
            max_transaction_bytes: max,
        })
    }

    pub fn from_raw(raw: &[Vec<u8>], limits: AssetSpendBatchLimitsV3) -> Result<Self> {
        let total = raw.iter().try_fold(0usize, |n, row| {
            n.checked_add(row.len()).ok_or(SpendEncodingError)
        })?;
        let max = raw.iter().map(Vec::len).max().unwrap_or(0);
        limits.check(raw.len(), total, max)?;
        let mut values: [Vec<Vec<u8>>; FIELDS] = std::array::from_fn(|_| Vec::new());
        let mut signatures = Vec::new();
        for wire in raw {
            let tx = SignedAssetSpendV3::decode(wire)?;
            if tx.encode()? != *wire {
                return Err(SpendEncodingError);
            }
            let i = tx.intent;
            let cells = [
                i.chain_id.to_be_bytes().to_vec(),
                i.genesis.to_vec(),
                i.payer,
                i.authorization_epoch.to_be_bytes().to_vec(),
                i.spend_key_ref.to_vec(),
                i.recipient,
                i.amount.to_be_bytes().to_vec(),
                i.max_fee.to_be_bytes().to_vec(),
                i.nonce.to_be_bytes().to_vec(),
                i.not_before_height.to_be_bytes().to_vec(),
                i.expires_at_height.to_be_bytes().to_vec(),
                tx.public_key,
                vec![i.asset],
                encode_asset_action_v2(&i.authority_update, &i.recipient_creation)?,
                vec![i.authority as u8],
            ];
            for (field, cell) in cells.into_iter().enumerate() {
                validate_cell(field, &cell)?;
                values[field].push(cell);
            }
            signatures.extend_from_slice(&tx.signature);
        }
        Ok(Self {
            rows: raw.len(),
            columns: values.map(|column| Column::collect(column.into_iter())),
            signatures,
            canonical_bytes: total,
            max_transaction_bytes: max,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(self.rows)
                .map_err(|_| SpendEncodingError)?
                .to_le_bytes(),
        );
        out.extend_from_slice(
            &u64::try_from(self.canonical_bytes)
                .map_err(|_| SpendEncodingError)?
                .to_le_bytes(),
        );
        out.extend_from_slice(
            &u32::try_from(self.max_transaction_bytes)
                .map_err(|_| SpendEncodingError)?
                .to_le_bytes(),
        );
        for column in &self.columns {
            out.extend_from_slice(
                &u32::try_from(column.values.len())
                    .map_err(|_| SpendEncodingError)?
                    .to_le_bytes(),
            );
            for cell in &column.values {
                out.extend_from_slice(
                    &u32::try_from(cell.len())
                        .map_err(|_| SpendEncodingError)?
                        .to_le_bytes(),
                );
                out.extend_from_slice(cell);
            }
            for index in &column.indexes {
                out.extend_from_slice(&index.to_le_bytes());
            }
        }
        out.extend_from_slice(&self.signatures);
        Ok(out)
    }

    pub fn decode(bytes: &[u8], limits: AssetSpendBatchLimitsV3) -> Result<Self> {
        let wire_limit = limits
            .body_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(4096))
            .ok_or(SpendEncodingError)?;
        if bytes.len() > wire_limit {
            return Err(SpendEncodingError);
        }
        let mut reader = Reader(bytes);
        if reader.take(8)? != MAGIC || reader.u16()? != VERSION {
            return Err(SpendEncodingError);
        }
        let rows = reader.u32()? as usize;
        let total = usize::try_from(reader.u64()?).map_err(|_| SpendEncodingError)?;
        let max = reader.u32()? as usize;
        limits.check(rows, total, max)?;
        // Borrow dictionaries first. No full key/signature payload is allocated
        // before framing, indexes and expanded budgets have all been checked.
        // Each dictionary entry consumes at least its four-byte wire length;
        // no allocation capacity is derived from an unchecked declared count.
        let mut borrowed = Vec::new();
        for field in 0..FIELDS {
            borrowed.push(read_column(&mut reader, rows, field)?);
        }
        let signatures = reader.take(
            rows.checked_mul(MLDSA65_SIGNATURE_BYTES)
                .ok_or(SpendEncodingError)?,
        )?;
        if !reader.0.is_empty() {
            return Err(SpendEncodingError);
        }
        let mut actual_total = 0usize;
        let mut actual_max = 0usize;
        for row in 0..rows {
            let len = MIN_RAW + borrowed[2].value(row).len() + borrowed[5].value(row).len() - 40
                + borrowed[13].value(row).len()
                - 1;
            actual_total = actual_total.checked_add(len).ok_or(SpendEncodingError)?;
            actual_max = actual_max.max(len);
        }
        if (total, max) != (actual_total, actual_max) {
            return Err(SpendEncodingError);
        }
        let columns: [Column<Vec<u8>>; FIELDS] = borrowed
            .into_iter()
            .map(|c| c.map(<[u8]>::to_vec))
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|_| SpendEncodingError)?;
        let batch = Self {
            rows,
            columns,
            signatures: signatures.to_vec(),
            canonical_bytes: total,
            max_transaction_bytes: max,
        };
        // Preserve the single NUAS contract, including key-ref/domain/range
        // checks. This is structural validation, NOT cryptographic verification.
        for row in 0..rows {
            batch.row(row)?.validate()?;
        }
        Ok(batch)
    }

    pub fn view(&self, row: usize) -> Result<AssetSpendViewV3<'_>> {
        if row >= self.rows {
            return Err(SpendEncodingError);
        }
        Ok(AssetSpendViewV3 {
            cells: std::array::from_fn(|field| self.columns[field].value(row).as_slice()),
            signature: &self.signatures
                [row * MLDSA65_SIGNATURE_BYTES..(row + 1) * MLDSA65_SIGNATURE_BYTES],
        })
    }

    pub fn row(&self, row: usize) -> Result<SignedAssetSpendV3> {
        let view = self.view(row)?;
        Ok(SignedAssetSpendV3 {
            intent: view.intent()?,
            public_key: view.cells[11].to_vec(),
            signature: view.signature.to_vec(),
        })
    }

    pub fn len(&self) -> usize {
        self.rows
    }
    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }
    pub fn canonical_bytes(&self) -> usize {
        self.canonical_bytes
    }
    pub fn max_transaction_bytes(&self) -> usize {
        self.max_transaction_bytes
    }
    pub fn raw_transaction(&self, index: usize) -> Result<Vec<u8>> {
        self.row(index)?.encode()
    }

    /// Exact canonical signed row length without rebuilding or allocating it.
    pub fn transaction_bytes(&self, index: usize) -> Result<usize> {
        if index >= self.rows {
            return Err(SpendEncodingError);
        }
        Ok(
            MIN_RAW + self.columns[2].value(index).len() + self.columns[5].value(index).len() - 40
                + self.columns[13].value(index).len()
                - 1,
        )
    }
    pub fn signed_accounts(&self, index: usize) -> Result<(&[u8], &[u8])> {
        if index >= self.rows {
            return Err(SpendEncodingError);
        }
        Ok((self.columns[2].value(index), self.columns[5].value(index)))
    }
    /// Logical retained allocations including spare capacity, excluding allocator
    /// bookkeeping and caller Arc counters. Charge the whole batch, not one row.
    pub fn retained_bytes(&self) -> Result<usize> {
        self.columns.iter().try_fold(
            std::mem::size_of::<Self>()
                .checked_add(self.signatures.capacity())
                .ok_or(SpendEncodingError)?,
            |n, column| {
                let n = n
                    .checked_add(
                        column
                            .values
                            .capacity()
                            .checked_mul(std::mem::size_of::<Vec<u8>>())
                            .ok_or(SpendEncodingError)?,
                    )
                    .and_then(|n| n.checked_add(column.indexes.capacity().checked_mul(4)?))
                    .ok_or(SpendEncodingError)?;
                column.values.iter().try_fold(n, |n, v| {
                    n.checked_add(v.capacity()).ok_or(SpendEncodingError)
                })
            },
        )
    }
}

fn validate_cell(field: usize, cell: &[u8]) -> Result<()> {
    let valid = match field {
        0 | 3 | 8 | 9 | 10 => cell.len() == 8,
        1 | 4 => cell.len() == 32,
        6 | 7 => cell.len() == 16,
        2 | 5 => matches!(cell.len(), 20 | 32),
        11 => cell.len() == MLDSA65_PUBLIC_KEY_BYTES,
        12 => cell == [NATIVE_NOV_ASSET],
        13 => decode_asset_action_v2(cell).is_ok(),
        14 => cell.len() == 1 && AssetSpendAuthorityV3::try_from(cell[0]).is_ok(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(SpendEncodingError)
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.0.len() {
            return Err(SpendEncodingError);
        }
        let (value, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(value)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| SpendEncodingError)?,
        ))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| SpendEncodingError)?,
        ))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| SpendEncodingError)?,
        ))
    }
}

fn read_column<'a>(r: &mut Reader<'a>, rows: usize, field: usize) -> Result<Column<&'a [u8]>> {
    let count = r.u32()? as usize;
    if count == 0 || count > rows || count > r.0.len() / 4 {
        return Err(SpendEncodingError);
    }
    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..count {
        let len = r.u32()? as usize;
        let value = r.take(len)?;
        validate_cell(field, value)?;
        if !seen.insert(value) {
            return Err(SpendEncodingError);
        }
        values.push(value);
    }
    let mut indexes = Vec::new();
    if count != 1 && count != rows {
        if rows > r.0.len() / 4 {
            return Err(SpendEncodingError);
        }
        let mut next = 0u32;
        for _ in 0..rows {
            let index = r.u32()?;
            if index as usize >= count || index > next {
                return Err(SpendEncodingError);
            }
            if index == next {
                next += 1;
            }
            indexes.push(index);
        }
        if next as usize != count {
            return Err(SpendEncodingError);
        }
    }
    Ok(Column { values, indexes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uca_spend::spend_key_ref_v1;
    type Fixture = (Vec<Vec<u8>>, AssetSpendBatchLimitsV3);
    fn fixture() -> Fixture {
        let key = vec![3; MLDSA65_PUBLIC_KEY_BYTES];
        let raw = (0..4)
            .map(|nonce| {
                SignedAssetSpendV3 {
                    intent: AssetSpendIntentV3 {
                        authority: AssetSpendAuthorityV3::DelegatedGrant,
                        chain_id: 1,
                        genesis: [2; 32],
                        payer: vec![5; 20],
                        authorization_epoch: 1,
                        spend_key_ref: spend_key_ref_v1(&key).unwrap(),
                        recipient: vec![if nonce % 2 == 0 { 6 } else { 7 }; 32],
                        asset: NATIVE_NOV_ASSET,
                        amount: 80,
                        max_fee: 10,
                        nonce,
                        not_before_height: 1,
                        expires_at_height: 10,
                        authority_update: None,
                        recipient_creation: None,
                    },
                    public_key: key.clone(),
                    signature: vec![nonce as u8; MLDSA65_SIGNATURE_BYTES],
                }
                .encode()
                .unwrap()
            })
            .collect::<Vec<_>>();
        let limits = AssetSpendBatchLimitsV3 {
            transactions: 4,
            transaction_bytes: MAX_SIGNED_SPEND_BYTES,
            body_bytes: 4 * MAX_SIGNED_SPEND_BYTES,
        };
        (raw, limits)
    }
    #[test]
    fn structural_roundtrip_preserves_every_signed_account_and_byte() {
        let (raw, limits) = fixture();
        let batch = AssetSpendBatchV3::from_raw(&raw, limits).unwrap();
        let wire = batch.encode().unwrap();
        assert!(wire.len() < raw.iter().map(Vec::len).sum::<usize>());
        assert_eq!(batch.columns[11].values.len(), 1);
        let decoded = AssetSpendBatchV3::decode(&wire, limits).unwrap();
        assert_eq!(decoded, batch);
        assert_eq!(decoded.encode().unwrap(), wire);
        for (i, expected) in raw.iter().enumerate() {
            let signed = SignedAssetSpendV3::decode(expected).unwrap();
            assert_eq!(&decoded.raw_transaction(i).unwrap(), expected);
            assert_eq!(
                decoded.signed_accounts(i).unwrap(),
                (
                    signed.intent.payer.as_slice(),
                    signed.intent.recipient.as_slice()
                )
            );
            let mut digest = sha2::Sha256::new();
            decoded
                .view(i)
                .unwrap()
                .update_canonical_digest(&mut digest)
                .unwrap();
            assert_eq!(digest.finalize(), sha2::Sha256::digest(expected));
        }
        assert!(decoded.raw_transaction(4).is_err());
        assert!(decoded.transaction_bytes(4).is_err());
        assert!(decoded.signed_accounts(4).is_err());
        assert!(decoded.retained_bytes().unwrap() >= decoded.signatures.len());
    }

    #[test]
    fn signed_authority_modes_remain_distinct_in_columns_rows_and_commitments() {
        let (mut raw, limits) = fixture();
        let mut controller = SignedAssetSpendV3::decode(&raw[0]).unwrap();
        controller.intent.authority = AssetSpendAuthorityV3::Controller;
        raw[1] = controller.encode().unwrap();
        let batch = AssetSpendBatchV3::from_raw(&raw, limits).unwrap();
        assert_eq!(batch.columns[14].values.len(), 2);
        let wire = batch.encode().unwrap();
        assert!(wire.starts_with(b"NUAPFL03"));
        let decoded = AssetSpendBatchV3::decode(&wire, limits).unwrap();
        assert_eq!(
            decoded.row(0).unwrap().intent.authority,
            AssetSpendAuthorityV3::DelegatedGrant
        );
        assert_eq!(
            decoded.row(1).unwrap().intent.authority,
            AssetSpendAuthorityV3::Controller
        );
        assert!(!decoded
            .view(0)
            .unwrap()
            .matches_signed(&controller)
            .unwrap());
        assert_ne!(
            decoded.view(0).unwrap().signing_bytes().unwrap(),
            decoded.view(1).unwrap().signing_bytes().unwrap()
        );
        for (row, expected) in raw.iter().enumerate() {
            assert_eq!(&decoded.raw_transaction(row).unwrap(), expected);
        }
        let mut unknown_mode = batch.clone();
        unknown_mode.columns[14].values[0] = vec![2];
        assert!(AssetSpendBatchV3::decode(&unknown_mode.encode().unwrap(), limits).is_err());
        let mut old = wire;
        old[..8].copy_from_slice(b"NUAPFL02");
        old[8..10].copy_from_slice(&2u16.to_le_bytes());
        assert!(AssetSpendBatchV3::decode(&old, limits).is_err());
    }

    #[test]
    fn borrowed_rebatch_preserves_canonical_order_and_enforces_original_limits() {
        let (raw, limits) = fixture();
        let left = AssetSpendBatchV3::from_raw(&raw[..2], limits).unwrap();
        let right = AssetSpendBatchV3::from_raw(&raw[2..], limits).unwrap();
        let views = [
            right.view(1).unwrap(),
            left.view(0).unwrap(),
            right.view(0).unwrap(),
        ];
        let expected_raw = vec![raw[3].clone(), raw[0].clone(), raw[2].clone()];
        let expected = AssetSpendBatchV3::from_raw(&expected_raw, limits).unwrap();
        let actual = AssetSpendBatchV3::from_views(&views, limits).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual.encode().unwrap(), expected.encode().unwrap());
        assert_eq!(actual.columns[11].values.len(), 1);
        assert!(left.view(2).is_err());
        assert!(AssetSpendBatchV3::from_views(&[], limits).is_err());
        for limited in [
            AssetSpendBatchLimitsV3 {
                transactions: 2,
                ..limits
            },
            AssetSpendBatchLimitsV3 {
                transaction_bytes: actual.max_transaction_bytes() - 1,
                ..limits
            },
            AssetSpendBatchLimitsV3 {
                body_bytes: actual.canonical_bytes() - 1,
                ..limits
            },
        ] {
            assert!(AssetSpendBatchV3::from_views(&views, limited).is_err());
        }
    }

    #[test]
    fn borrowed_row_match_binds_all_fields_key_and_signature_not_just_intent_hash() {
        let (raw, limits) = fixture();
        let batch = AssetSpendBatchV3::from_raw(&raw, limits).unwrap();
        let view = batch.view(0).unwrap();
        let signed = SignedAssetSpendV3::decode(&raw[0]).unwrap();
        assert_eq!(
            view.signing_bytes().unwrap(),
            signed.intent.signing_bytes().unwrap()
        );
        assert!(view.matches_signed(&signed).unwrap());
        for field in 0..16 {
            let mut changed = signed.clone();
            let i = &mut changed.intent;
            match field {
                0 => i.chain_id += 1,
                1 => i.genesis[0] ^= 1,
                2 => i.payer[0] ^= 1,
                3 => i.authorization_epoch += 1,
                4 => i.spend_key_ref[0] ^= 1,
                5 => i.recipient[0] ^= 1,
                6 => i.amount += 1,
                7 => i.max_fee += 1,
                8 => i.nonce += 1,
                9 => i.not_before_height += 1,
                10 => i.expires_at_height += 1,
                11 => changed.public_key[0] ^= 1,
                12 => i.asset ^= 1,
                13 => changed.signature[0] ^= 1,
                14 => {
                    changed.signature.pop().unwrap();
                }
                15 => i.authority = AssetSpendAuthorityV3::Controller,
                _ => unreachable!(),
            }
            assert!(!view.matches_signed(&changed).unwrap(), "field {field}");
        }
        assert_ne!(view, batch.view(1).unwrap());
    }
    #[test]
    fn bounds_headers_truncation_and_trailing_bytes_fail_closed() {
        let (raw, limits) = fixture();
        let wire = AssetSpendBatchV3::from_raw(&raw, limits)
            .unwrap()
            .encode()
            .unwrap();
        for end in [0, 7, 9, 25, wire.len() - 1] {
            assert!(AssetSpendBatchV3::decode(&wire[..end], limits).is_err());
        }
        for offset in [0, 8, 10, 14, 22] {
            let mut bad = wire.clone();
            bad[offset] ^= 0x40;
            assert!(AssetSpendBatchV3::decode(&bad, limits).is_err());
        }
        let mut bad = wire.clone();
        bad.push(0);
        assert!(AssetSpendBatchV3::decode(&bad, limits).is_err());
        assert!(AssetSpendBatchV3::decode(
            &wire,
            AssetSpendBatchLimitsV3 {
                transactions: 3,
                ..limits
            }
        )
        .is_err());
        assert!(AssetSpendBatchV3::decode(
            &wire,
            AssetSpendBatchLimitsV3 {
                transaction_bytes: MIN_RAW,
                ..limits
            }
        )
        .is_err());
        assert!(AssetSpendBatchV3::decode(
            &wire,
            AssetSpendBatchLimitsV3 {
                body_bytes: MIN_RAW,
                ..limits
            }
        )
        .is_err());
        assert!(AssetSpendBatchV3::from_raw(&[], limits).is_err());
        let mut legacy = wire.clone();
        legacy[7] = b'1';
        legacy[8] = 1;
        assert!(AssetSpendBatchV3::decode(&legacy, limits).is_err());
    }
    #[test]
    fn dictionary_canonicality_and_original_nuas_contract_are_checked() {
        let (raw, limits) = fixture();
        let batch = AssetSpendBatchV3::from_raw(&raw, limits).unwrap();
        let mut bad = batch.clone();
        bad.columns[5].indexes[0] = 1;
        assert!(AssetSpendBatchV3::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[5].indexes.fill(0);
        assert!(AssetSpendBatchV3::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[5].indexes[2] = 2;
        assert!(AssetSpendBatchV3::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[5].values[1] = bad.columns[5].values[0].clone();
        assert!(AssetSpendBatchV3::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[11].values[0][0] ^= 1;
        assert!(AssetSpendBatchV3::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[8].values[0] = u64::MAX.to_be_bytes().to_vec();
        assert!(AssetSpendBatchV3::decode(&bad.encode().unwrap(), limits).is_err());
        // The codec does NOT claim to verify a signature or authorize accounts.
        let mut unverified = batch.clone();
        unverified.signatures[0] ^= 1;
        assert!(AssetSpendBatchV3::decode(&unverified.encode().unwrap(), limits).is_ok());
    }

    #[test]
    fn mixed_spend_and_authority_updates_preserve_original_rows_and_byte_budgets() {
        use crate::uca_spend::{
            AssetAccountCreationV2, AssetAuthorityUpdateV2, AssetGrantConfigV2,
        };
        let (mut raw, limits) = fixture();
        let mut management = SignedAssetSpendV3::decode(&raw[1]).unwrap();
        management.intent.authority = AssetSpendAuthorityV3::Controller;
        management.intent.amount = 0;
        management.intent.recipient = management.intent.payer.clone();
        management.intent.authority_update = Some(AssetAuthorityUpdateV2 {
            use_recovery: true,
            next_epoch: 2,
            active: true,
            management_key_ref: [9; 32],
            recovery_key_ref: Some([8; 32]),
            grants: vec![AssetGrantConfigV2 {
                key_ref: [7; 32],
                permits_nov_transfer: true,
                not_before_height: 1,
                expires_at_height: 100,
                max_debit_per_intent: 10,
                remaining_debit_budget: 100,
            }],
        });
        raw[1] = management.encode().unwrap();
        let mut opening = SignedAssetSpendV3::decode(&raw[2]).unwrap();
        let creation = AssetAccountCreationV2 {
            management_key_ref: [12; 32],
            recovery_key_ref: Some([11; 32]),
            grants: vec![AssetGrantConfigV2 {
                key_ref: [10; 32],
                permits_nov_transfer: true,
                not_before_height: 1,
                expires_at_height: 100,
                max_debit_per_intent: 10,
                remaining_debit_budget: 100,
            }],
        };
        opening.intent.recipient = creation
            .account(opening.intent.chain_id, &opening.intent.genesis)
            .unwrap()
            .to_vec();
        opening.intent.recipient_creation = Some(creation);
        raw[2] = opening.encode().unwrap();
        let batch = AssetSpendBatchV3::from_raw(&raw, limits).unwrap();
        assert_eq!(batch.canonical_bytes(), raw.iter().map(Vec::len).sum());
        let wire = batch.encode().unwrap();
        let decoded = AssetSpendBatchV3::decode(&wire, limits).unwrap();
        let views = (0..decoded.len())
            .map(|index| decoded.view(index).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            AssetSpendBatchV3::from_views(&views, limits).unwrap(),
            decoded
        );
        for (index, bytes) in raw.iter().enumerate() {
            assert_eq!(decoded.raw_transaction(index).unwrap(), *bytes);
            assert_eq!(decoded.transaction_bytes(index).unwrap(), bytes.len());
            let signed = SignedAssetSpendV3::decode(bytes).unwrap();
            assert!(decoded.row(index).unwrap() == signed);
            assert!(decoded
                .view(index)
                .unwrap()
                .matches_signed(&signed)
                .unwrap());
            let mut digest = sha2::Sha256::new();
            decoded
                .view(index)
                .unwrap()
                .update_canonical_digest(&mut digest)
                .unwrap();
            assert_eq!(digest.finalize(), sha2::Sha256::digest(bytes));
        }
        let mut changed = management;
        changed.intent.authority_update.as_mut().unwrap().grants[0].remaining_debit_budget += 1;
        assert!(!decoded.view(1).unwrap().matches_signed(&changed).unwrap());
        let mut wrong = batch;
        wrong.columns[13].values.push(vec![0]);
        assert!(AssetSpendBatchV3::decode(&wrong.encode().unwrap(), limits).is_err());
        assert!(AssetSpendBatchV3::decode(
            &wire,
            AssetSpendBatchLimitsV3 {
                body_bytes: decoded.canonical_bytes() - 1,
                ..limits
            }
        )
        .is_err());
    }
}
