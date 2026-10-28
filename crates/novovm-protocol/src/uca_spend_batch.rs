//! Lossless APFL columns for canonical NUAS v2 transactions. Concrete accounts
//! are signed fields, not external hints. This is not signature/authority proof.
use crate::apfl_column::Column;
use crate::uca_spend::{
    decode_authority_update_v2, encode_authority_update_v2, AssetSpendIntentV2, SignedAssetSpendV2,
    SpendEncodingError, MAX_SIGNED_SPEND_BYTES, MIN_SIGNED_SPEND_BYTES, MLDSA65_PUBLIC_KEY_BYTES,
    MLDSA65_SIGNATURE_BYTES, NATIVE_NOV_ASSET,
};
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"NUAPFL02";
const VERSION: u16 = 2;
const FIELDS: usize = 14;
const MIN_RAW: usize = MIN_SIGNED_SPEND_BYTES;
type Result<T> = std::result::Result<T, SpendEncodingError>;

#[derive(Clone, Copy, Debug)]
pub struct AssetSpendBatchLimitsV2 {
    pub transactions: usize,
    pub transaction_bytes: usize,
    /// Expanded canonical NUAS bytes. Owned metadata is separately bounded by
    /// row count and must also be charged by the retaining owner.
    pub body_bytes: usize,
}

impl AssetSpendBatchLimitsV2 {
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
/// update. Numeric cells retain NUAS big endian byte order.
/// The outer column framing uses little endian as the existing APFL codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetSpendBatchV2 {
    rows: usize,
    columns: [Column<Vec<u8>>; FIELDS],
    signatures: Vec<u8>,
    canonical_bytes: usize,
    max_transaction_bytes: usize,
}

impl AssetSpendBatchV2 {
    pub fn from_raw(raw: &[Vec<u8>], limits: AssetSpendBatchLimitsV2) -> Result<Self> {
        let total = raw.iter().try_fold(0usize, |n, row| {
            n.checked_add(row.len()).ok_or(SpendEncodingError)
        })?;
        let max = raw.iter().map(Vec::len).max().unwrap_or(0);
        limits.check(raw.len(), total, max)?;
        let mut values: [Vec<Vec<u8>>; FIELDS] = std::array::from_fn(|_| Vec::new());
        let mut signatures = Vec::new();
        for wire in raw {
            let tx = SignedAssetSpendV2::decode(wire)?;
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
                encode_authority_update_v2(&i.authority_update)?,
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

    pub fn decode(bytes: &[u8], limits: AssetSpendBatchLimitsV2) -> Result<Self> {
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

    pub fn row(&self, row: usize) -> Result<SignedAssetSpendV2> {
        if row >= self.rows {
            return Err(SpendEncodingError);
        }
        let cell = |field: usize| self.columns[field].value(row).as_slice();
        let array8 = |field| cell(field).try_into().map_err(|_| SpendEncodingError);
        Ok(SignedAssetSpendV2 {
            intent: AssetSpendIntentV2 {
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
                authority_update: decode_authority_update_v2(cell(13))?,
            },
            public_key: cell(11).to_vec(),
            signature: self.signatures
                [row * MLDSA65_SIGNATURE_BYTES..(row + 1) * MLDSA65_SIGNATURE_BYTES]
                .to_vec(),
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
        13 => decode_authority_update_v2(cell).is_ok(),
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
    type Fixture = (Vec<Vec<u8>>, AssetSpendBatchLimitsV2);
    fn fixture() -> Fixture {
        let key = vec![3; MLDSA65_PUBLIC_KEY_BYTES];
        let raw = (0..4)
            .map(|nonce| {
                SignedAssetSpendV2 {
                    intent: AssetSpendIntentV2 {
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
                    },
                    public_key: key.clone(),
                    signature: vec![nonce as u8; MLDSA65_SIGNATURE_BYTES],
                }
                .encode()
                .unwrap()
            })
            .collect::<Vec<_>>();
        let limits = AssetSpendBatchLimitsV2 {
            transactions: 4,
            transaction_bytes: MAX_SIGNED_SPEND_BYTES,
            body_bytes: 4 * MAX_SIGNED_SPEND_BYTES,
        };
        (raw, limits)
    }
    #[test]
    fn structural_roundtrip_preserves_every_signed_account_and_byte() {
        let (raw, limits) = fixture();
        let batch = AssetSpendBatchV2::from_raw(&raw, limits).unwrap();
        let wire = batch.encode().unwrap();
        assert!(wire.len() < raw.iter().map(Vec::len).sum::<usize>());
        assert_eq!(batch.columns[11].values.len(), 1);
        let decoded = AssetSpendBatchV2::decode(&wire, limits).unwrap();
        assert_eq!(decoded, batch);
        assert_eq!(decoded.encode().unwrap(), wire);
        for (i, expected) in raw.iter().enumerate() {
            let signed = SignedAssetSpendV2::decode(expected).unwrap();
            assert_eq!(&decoded.raw_transaction(i).unwrap(), expected);
            assert_eq!(
                decoded.signed_accounts(i).unwrap(),
                (
                    signed.intent.payer.as_slice(),
                    signed.intent.recipient.as_slice()
                )
            );
        }
        assert!(decoded.raw_transaction(4).is_err());
        assert!(decoded.transaction_bytes(4).is_err());
        assert!(decoded.signed_accounts(4).is_err());
        assert!(decoded.retained_bytes().unwrap() >= decoded.signatures.len());
    }
    #[test]
    fn bounds_headers_truncation_and_trailing_bytes_fail_closed() {
        let (raw, limits) = fixture();
        let wire = AssetSpendBatchV2::from_raw(&raw, limits)
            .unwrap()
            .encode()
            .unwrap();
        for end in [0, 7, 9, 25, wire.len() - 1] {
            assert!(AssetSpendBatchV2::decode(&wire[..end], limits).is_err());
        }
        for offset in [0, 8, 10, 14, 22] {
            let mut bad = wire.clone();
            bad[offset] ^= 0x40;
            assert!(AssetSpendBatchV2::decode(&bad, limits).is_err());
        }
        let mut bad = wire.clone();
        bad.push(0);
        assert!(AssetSpendBatchV2::decode(&bad, limits).is_err());
        assert!(AssetSpendBatchV2::decode(
            &wire,
            AssetSpendBatchLimitsV2 {
                transactions: 3,
                ..limits
            }
        )
        .is_err());
        assert!(AssetSpendBatchV2::decode(
            &wire,
            AssetSpendBatchLimitsV2 {
                transaction_bytes: MIN_RAW,
                ..limits
            }
        )
        .is_err());
        assert!(AssetSpendBatchV2::decode(
            &wire,
            AssetSpendBatchLimitsV2 {
                body_bytes: MIN_RAW,
                ..limits
            }
        )
        .is_err());
        assert!(AssetSpendBatchV2::from_raw(&[], limits).is_err());
        let mut legacy = wire.clone();
        legacy[7] = b'1';
        legacy[8] = 1;
        assert!(AssetSpendBatchV2::decode(&legacy, limits).is_err());
    }
    #[test]
    fn dictionary_canonicality_and_original_nuas_contract_are_checked() {
        let (raw, limits) = fixture();
        let batch = AssetSpendBatchV2::from_raw(&raw, limits).unwrap();
        let mut bad = batch.clone();
        bad.columns[5].indexes[0] = 1;
        assert!(AssetSpendBatchV2::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[5].indexes.fill(0);
        assert!(AssetSpendBatchV2::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[5].indexes[2] = 2;
        assert!(AssetSpendBatchV2::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[5].values[1] = bad.columns[5].values[0].clone();
        assert!(AssetSpendBatchV2::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[11].values[0][0] ^= 1;
        assert!(AssetSpendBatchV2::decode(&bad.encode().unwrap(), limits).is_err());
        let mut bad = batch.clone();
        bad.columns[8].values[0] = u64::MAX.to_be_bytes().to_vec();
        assert!(AssetSpendBatchV2::decode(&bad.encode().unwrap(), limits).is_err());
        // The codec does NOT claim to verify a signature or authorize accounts.
        let mut unverified = batch.clone();
        unverified.signatures[0] ^= 1;
        assert!(AssetSpendBatchV2::decode(&unverified.encode().unwrap(), limits).is_ok());
    }

    #[test]
    fn mixed_spend_and_authority_updates_preserve_original_rows_and_byte_budgets() {
        use crate::uca_spend::{AssetAuthorityUpdateV2, AssetGrantConfigV2};
        let (mut raw, limits) = fixture();
        let mut management = SignedAssetSpendV2::decode(&raw[1]).unwrap();
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
        let batch = AssetSpendBatchV2::from_raw(&raw, limits).unwrap();
        assert_eq!(batch.canonical_bytes(), raw.iter().map(Vec::len).sum());
        let wire = batch.encode().unwrap();
        let decoded = AssetSpendBatchV2::decode(&wire, limits).unwrap();
        for (index, bytes) in raw.iter().enumerate() {
            assert_eq!(decoded.raw_transaction(index).unwrap(), *bytes);
            assert_eq!(decoded.transaction_bytes(index).unwrap(), bytes.len());
            assert!(decoded.row(index).unwrap() == SignedAssetSpendV2::decode(bytes).unwrap());
        }
        let mut wrong = batch;
        wrong.columns[13].values.push(vec![0]);
        assert!(AssetSpendBatchV2::decode(&wrong.encode().unwrap(), limits).is_err());
        assert!(AssetSpendBatchV2::decode(
            &wire,
            AssetSpendBatchLimitsV2 {
                body_bytes: decoded.canonical_bytes() - 1,
                ..limits
            }
        )
        .is_err());
    }
}
