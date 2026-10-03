//! Lossless structural columns for the ORIGINAL signed NNX1/3 Transfer.
//! This codec carries untrusted intent, not authentication or execution evidence.
//! Equal field values share a dictionary entry; first-use dictionary order and
//! row indexes are canonical. A fully distinct column is a dense residual column
//! (implicit identity indexes), and a shared column has implicit zero indexes.
//! Signatures are always the complete, unchanged 32-byte key + 64-byte signature.
//! No seed, fixture formula, default fee, or generated signature is permitted.

use super::wire::{decode_transfer_view_v3, FeePolicyView, TransferView};
use anyhow::{ensure, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const MAGIC: &[u8; 8] = b"NVAPFLV3";
const VERSION: u16 = 1;
const SIGNATURE_BYTES: usize = 96;
// Original canonical V3 with two 20-byte addresses, empty asset names and the
// smallest integer encodings. Used only as a pre-allocation lower bound.
const MIN_RAW_BYTES: usize = 152;

#[derive(Clone, Copy, Debug)]
pub struct ApflLimits {
    pub transactions: usize,
    pub transaction_bytes: usize,
    /// Budget for EXPANDED original signed V3 bytes, not compressed bytes.
    pub body_bytes: usize,
}

impl ApflLimits {
    fn check(&self, rows: usize, total: usize, max: usize) -> Result<()> {
        ensure!(
            rows > 0 && rows <= self.transactions && u32::try_from(rows).is_ok(),
            "APFL transaction count exceeds budget"
        );
        ensure!(
            max >= MIN_RAW_BYTES && max <= self.transaction_bytes,
            "APFL expanded transaction exceeds budget"
        );
        ensure!(
            total <= self.body_bytes && total >= max,
            "APFL expanded body exceeds budget"
        );
        ensure!(
            rows.checked_mul(MIN_RAW_BYTES)
                .is_some_and(|minimum| minimum <= total),
            "APFL expanded body below minimum size"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Column<T> {
    values: Vec<T>,
    // Empty for shared or dense columns; otherwise exactly one u32 per row.
    indexes: Vec<u32>,
}

impl<T> Column<T> {
    fn value(&self, index: usize) -> &T {
        let slot = if self.values.len() == 1 {
            0
        } else if self.indexes.is_empty() {
            index
        } else {
            self.indexes[index] as usize
        };
        &self.values[slot]
    }
    fn map<U>(self, convert: impl Fn(T) -> U) -> Column<U> {
        Column {
            values: self.values.into_iter().map(convert).collect(),
            indexes: self.indexes,
        }
    }
}

impl<T: Clone + Ord> Column<T> {
    fn collect(input: impl Iterator<Item = T>) -> Self {
        let mut known = BTreeMap::new();
        let mut values = Vec::new();
        let mut indexes = Vec::new();
        for value in input {
            let index = *known.entry(value.clone()).or_insert_with(|| {
                let index = values.len() as u32;
                values.push(value);
                index
            });
            indexes.push(index);
        }
        if values.len() == 1 || values.len() == indexes.len() {
            indexes.clear();
        }
        Self { values, indexes }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Columns<B, S, R> {
    rows: usize,
    chain: Column<u64>,
    from: Column<B>,
    to: Column<B>,
    asset: Column<S>,
    amount: Column<u128>,
    nonce: Column<u64>,
    pay_asset: Column<S>,
    max_pay: Column<u128>,
    slippage: Column<u32>,
    signatures: R,
    canonical_bytes: usize,
    max_transaction_bytes: usize,
}

type Owned = Columns<Vec<u8>, String, Vec<u8>>;
type Borrowed<'a> = Columns<&'a [u8], &'a str, &'a [u8]>;

impl<B: AsRef<[u8]>, S: AsRef<str>, R: AsRef<[u8]>> Columns<B, S, R> {
    fn row(&self, index: usize) -> Result<TransferView<'_>> {
        ensure!(index < self.rows, "APFL row index out of range");
        Ok(TransferView {
            chain_id: *self.chain.value(index),
            from: self.from.value(index).as_ref(),
            to: self.to.value(index).as_ref(),
            asset: self.asset.value(index).as_ref(),
            amount: *self.amount.value(index),
            nonce: *self.nonce.value(index),
            fee_policy: FeePolicyView {
                pay_asset: self.pay_asset.value(index).as_ref(),
                max_pay_amount: *self.max_pay.value(index),
                slippage_bps: *self.slippage.value(index),
            },
            signature: &self.signatures.as_ref()
                [index * SIGNATURE_BYTES..(index + 1) * SIGNATURE_BYTES],
        })
    }
    fn measure(&self, limits: ApflLimits) -> Result<(usize, usize)> {
        let mut total = 0usize;
        let mut max = 0usize;
        for index in 0..self.rows {
            let size = self.row(index)?.encoded_len()?;
            ensure!(
                size <= limits.transaction_bytes,
                "APFL expanded transaction exceeds budget"
            );
            total = total
                .checked_add(size)
                .context("APFL expanded size overflow")?;
            ensure!(
                total <= limits.body_bytes,
                "APFL expanded body exceeds budget"
            );
            max = max.max(size);
        }
        limits.check(self.rows, total, max)?;
        Ok((total, max))
    }
}

impl Borrowed<'_> {
    fn into_owned(self) -> Owned {
        Columns {
            rows: self.rows,
            chain: self.chain,
            from: self.from.map(<[u8]>::to_vec),
            to: self.to.map(<[u8]>::to_vec),
            asset: self.asset.map(str::to_owned),
            amount: self.amount,
            nonce: self.nonce,
            pay_asset: self.pay_asset.map(str::to_owned),
            max_pay: self.max_pay,
            slippage: self.slippage,
            signatures: self.signatures.to_vec(),
            canonical_bytes: self.canonical_bytes,
            max_transaction_bytes: self.max_transaction_bytes,
        }
    }
}

/// Immutable, cheap-to-clone structure. No expanded Vec<Vec<u8>> is retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApflTransferBatch(Arc<Owned>);

impl ApflTransferBatch {
    pub fn from_raw(raw: &[Vec<u8>], limits: ApflLimits) -> Result<Self> {
        let total = raw.iter().try_fold(0usize, |size, tx| {
            size.checked_add(tx.len()).context("APFL raw size overflow")
        })?;
        let max = raw.iter().map(Vec::len).max().unwrap_or(0);
        limits.check(raw.len(), total, max)?;
        let views = raw
            .iter()
            .map(|raw| decode_transfer_view_v3(raw, limits.transaction_bytes))
            .collect::<Result<Vec<_>>>()?;
        let mut signatures = Vec::with_capacity(raw.len() * SIGNATURE_BYTES);
        for row in &views {
            signatures.extend_from_slice(row.signature);
        }
        let columns = Columns {
            rows: views.len(),
            chain: Column::collect(views.iter().map(|row| row.chain_id)),
            from: Column::collect(views.iter().map(|row| row.from)).map(<[u8]>::to_vec),
            to: Column::collect(views.iter().map(|row| row.to)).map(<[u8]>::to_vec),
            asset: Column::collect(views.iter().map(|row| row.asset)).map(str::to_owned),
            amount: Column::collect(views.iter().map(|row| row.amount)),
            nonce: Column::collect(views.iter().map(|row| row.nonce)),
            pay_asset: Column::collect(views.iter().map(|row| row.fee_policy.pay_asset))
                .map(str::to_owned),
            max_pay: Column::collect(views.iter().map(|row| row.fee_policy.max_pay_amount)),
            slippage: Column::collect(views.iter().map(|row| row.fee_policy.slippage_bps)),
            signatures,
            canonical_bytes: total,
            max_transaction_bytes: max,
        };
        Ok(Self(Arc::new(columns)))
    }

    pub fn decode(bytes: &[u8], limits: ApflLimits) -> Result<Self> {
        // A canonical encoding can have fixed-width residual overhead. Bound
        // even hostile input before scanning. Expanded budgets remain separate.
        let wire_limit = limits
            .body_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(256))
            .context("APFL wire budget overflow")?;
        ensure!(bytes.len() <= wire_limit, "APFL wire exceeds budget");
        let mut reader = Reader { bytes, offset: 0 };
        ensure!(
            reader.take(8)? == MAGIC && reader.u16()? == VERSION,
            "APFL codec/version mismatch"
        );
        let rows = reader.u32()? as usize;
        let total = usize::try_from(reader.u64()?).context("APFL expanded body size overflow")?;
        let max = reader.u32()? as usize;
        limits.check(rows, total, max)?;
        let mut scan = reader;
        // Allocation-free framing, field-width, UTF-8 and index preflight.
        for kind in [
            Kind::U64,
            Kind::Account,
            Kind::Account,
            Kind::Text,
            Kind::U128,
            Kind::U64,
            Kind::Text,
            Kind::U128,
            Kind::U32,
        ] {
            scan_column(&mut scan, rows, kind, limits.transaction_bytes)?;
        }
        scan.take(
            rows.checked_mul(SIGNATURE_BYTES)
                .context("APFL signature size overflow")?,
        )?;
        ensure!(scan.remaining() == 0, "APFL trailing bytes");
        // Only bounded borrowed metadata/numeric columns are allocated here.
        // Actual expanded sizes are measured BEFORE copying dictionaries or
        // signatures, and no expanded transaction vector is ever constructed.
        let parsed = Borrowed {
            rows,
            chain: read_column(&mut reader, rows, Reader::u64)?,
            from: read_column(&mut reader, rows, Reader::account)?,
            to: read_column(&mut reader, rows, Reader::account)?,
            asset: read_column(&mut reader, rows, Reader::text)?,
            amount: read_column(&mut reader, rows, Reader::u128)?,
            nonce: read_column(&mut reader, rows, Reader::u64)?,
            pay_asset: read_column(&mut reader, rows, Reader::text)?,
            max_pay: read_column(&mut reader, rows, Reader::u128)?,
            slippage: read_column(&mut reader, rows, Reader::u32)?,
            signatures: reader.take(rows * SIGNATURE_BYTES)?,
            canonical_bytes: total,
            max_transaction_bytes: max,
        };
        ensure!(
            parsed.measure(limits)? == (total, max),
            "APFL false expanded size declaration"
        );
        Ok(Self(Arc::new(parsed.into_owned())))
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let data = &self.0;
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&u32::try_from(data.rows)?.to_le_bytes());
        out.extend_from_slice(&u64::try_from(data.canonical_bytes)?.to_le_bytes());
        out.extend_from_slice(&u32::try_from(data.max_transaction_bytes)?.to_le_bytes());
        write_column(&mut out, &data.chain, |out, value| {
            out.extend_from_slice(&value.to_le_bytes());
            Ok(())
        })?;
        write_column(&mut out, &data.from, |out, value| put_blob(out, value))?;
        write_column(&mut out, &data.to, |out, value| put_blob(out, value))?;
        write_column(&mut out, &data.asset, |out, value| {
            put_blob(out, value.as_bytes())
        })?;
        write_column(&mut out, &data.amount, |out, value| {
            out.extend_from_slice(&value.to_le_bytes());
            Ok(())
        })?;
        write_column(&mut out, &data.nonce, |out, value| {
            out.extend_from_slice(&value.to_le_bytes());
            Ok(())
        })?;
        write_column(&mut out, &data.pay_asset, |out, value| {
            put_blob(out, value.as_bytes())
        })?;
        write_column(&mut out, &data.max_pay, |out, value| {
            out.extend_from_slice(&value.to_le_bytes());
            Ok(())
        })?;
        write_column(&mut out, &data.slippage, |out, value| {
            out.extend_from_slice(&value.to_le_bytes());
            Ok(())
        })?;
        out.extend_from_slice(&data.signatures);
        Ok(out)
    }

    pub fn len(&self) -> usize {
        self.0.rows
    }
    pub fn is_empty(&self) -> bool {
        self.0.rows == 0
    }
    pub fn row(&self, index: usize) -> Result<TransferView<'_>> {
        self.0.row(index)
    }
    pub fn canonical_raw(&self, index: usize) -> Result<Vec<u8>> {
        self.row(index)?.encode()
    }
    pub fn canonical_bytes(&self) -> usize {
        self.0.canonical_bytes
    }
    pub fn max_transaction_bytes(&self) -> usize {
        self.0.max_transaction_bytes
    }
}

fn write_column<T>(
    out: &mut Vec<u8>,
    column: &Column<T>,
    mut write: impl FnMut(&mut Vec<u8>, &T) -> Result<()>,
) -> Result<()> {
    out.extend_from_slice(&u32::try_from(column.values.len())?.to_le_bytes());
    for value in &column.values {
        write(out, value)?;
    }
    for index in &column.indexes {
        out.extend_from_slice(&index.to_le_bytes());
    }
    Ok(())
}

fn put_blob(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

#[derive(Clone, Copy)]
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .context("APFL offset overflow")?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .context("truncated APFL field")?;
        self.offset = end;
        Ok(bytes)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn u128(&mut self) -> Result<u128> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into()?))
    }
    fn blob(&mut self) -> Result<&'a [u8]> {
        let size = self.u32()? as usize;
        self.take(size)
    }
    fn account(&mut self) -> Result<&'a [u8]> {
        let value = self.blob()?;
        ensure!(
            matches!(value.len(), 20 | 32),
            "APFL account must be 20 or 32 bytes"
        );
        Ok(value)
    }
    fn text(&mut self) -> Result<&'a str> {
        std::str::from_utf8(self.blob()?).context("APFL text is not UTF-8")
    }
}

enum Kind {
    U64,
    Account,
    Text,
    U128,
    U32,
}

fn read_indexes(
    reader: &mut Reader<'_>,
    rows: usize,
    count: usize,
    mut visit: impl FnMut(u32),
) -> Result<()> {
    if count == 1 || count == rows {
        return Ok(());
    }
    let mut next = 0u32;
    for _ in 0..rows {
        let index = reader.u32()?;
        ensure!(
            (index as usize) < count && index <= next,
            "APFL noncanonical or out-of-range index"
        );
        if index == next {
            next += 1;
        }
        visit(index);
    }
    ensure!(next as usize == count, "APFL unused dictionary value");
    Ok(())
}

fn scan_column(reader: &mut Reader<'_>, rows: usize, kind: Kind, max: usize) -> Result<()> {
    let count = reader.u32()? as usize;
    ensure!(count > 0 && count <= rows, "APFL invalid column count");
    for _ in 0..count {
        match kind {
            Kind::U64 => {
                reader.take(8)?;
            }
            Kind::U128 => {
                reader.take(16)?;
            }
            Kind::U32 => {
                reader.take(4)?;
            }
            Kind::Account => {
                reader.account()?;
            }
            Kind::Text => {
                ensure!(
                    reader.text()?.len() <= max,
                    "APFL text exceeds transaction budget"
                );
            }
        }
    }
    read_indexes(reader, rows, count, |_| {})
}

fn read_column<'a, T: Ord>(
    reader: &mut Reader<'a>,
    rows: usize,
    mut read: impl FnMut(&mut Reader<'a>) -> Result<T>,
) -> Result<Column<T>> {
    let count = reader.u32()? as usize;
    ensure!(count > 0 && count <= rows, "APFL invalid column count");
    let values = (0..count)
        .map(|_| read(reader))
        .collect::<Result<Vec<_>>>()?;
    // Reject duplicate shared/dense/dictionary representations of the same rows.
    ensure!(
        values.iter().collect::<BTreeSet<_>>().len() == values.len(),
        "APFL duplicate dictionary value"
    );
    let mut indexes = Vec::new();
    if count != 1 && count != rows {
        indexes.reserve(rows);
    }
    read_indexes(reader, rows, count, |index| indexes.push(index))?;
    Ok(Column { values, indexes })
}

#[cfg(test)]
mod tests;
