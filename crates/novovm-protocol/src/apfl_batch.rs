//! Versioned domain-neutral APFL byte columns. Reuses the original Transfer
//! dictionary/shared/dense representation, but does NOT change NVAPFLV3 wire.
//! No compression of ciphertext, generated signatures, execution or authority.
use crate::apfl_column::Column;
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"NVAPFLC1";
const HEADER: usize = 60;
pub type Result<T> = std::result::Result<T, &'static str>;
#[derive(Clone, Copy)]
pub struct Limits {
    pub rows: usize,
    pub columns: usize,
    pub field_bytes: usize,
    pub wire_bytes: usize,
    pub expanded_bytes: usize,
}
impl Limits {
    fn check(self) -> Result<()> {
        if !(1..=1024).contains(&self.rows)
            || !(1..=32).contains(&self.columns)
            || self.field_bytes == 0
            || self.wire_bytes < HEADER
            || self.wire_bytes > 64 * 1024 * 1024
            || self.expanded_bytes == 0
            || self.expanded_bytes > 64 * 1024 * 1024
        {
            return Err("APFL limits");
        }
        Ok(())
    }
}
/// No Debug: columns may carry encrypted-channel capabilities. Blobs borrow
/// the caller's protected wire buffer, never allocate one copy per message.
pub struct BatchView<'a> {
    pub schema: u64,
    pub context: [u8; 32],
    rows: usize,
    columns: Vec<Column<&'a [u8]>>,
    expanded: usize,
}
impl<'a> BatchView<'a> {
    pub fn rows(&self) -> usize {
        self.rows
    }
    pub fn columns(&self) -> usize {
        self.columns.len()
    }
    pub fn expanded_bytes(&self) -> usize {
        self.expanded
    }
    pub fn field(&self, row: usize, column: usize) -> Result<&'a [u8]> {
        if row >= self.rows || column >= self.columns.len() {
            return Err("APFL field index");
        }
        Ok(*self.columns[column].value(row))
    }
}

/// Caller supplies columns, not serialized row objects. Shared fields are
/// encoded once; arbitrary unique ciphertext is retained losslessly as residual.
pub fn encode(
    schema: u64,
    context: [u8; 32],
    rows: usize,
    columns: &[Column<&[u8]>],
    limits: Limits,
) -> Result<Vec<u8>> {
    limits.check()?;
    if schema == 0
        || rows == 0
        || rows > limits.rows
        || columns.is_empty()
        || columns.len() > limits.columns
    {
        return Err("APFL shape");
    }
    let mut size = HEADER;
    let mut expanded = 0usize;
    for column in columns {
        validate_column(column, rows)?;
        size = size.checked_add(4).ok_or("APFL length overflow")?;
        for value in &column.values {
            if value.len() > limits.field_bytes {
                return Err("APFL field budget");
            }
            size = size
                .checked_add(4 + value.len())
                .ok_or("APFL length overflow")?;
        }
        size = size
            .checked_add(4 * column.indexes.len())
            .ok_or("APFL length overflow")?;
        for row in 0..rows {
            expanded = expanded
                .checked_add(column.value(row).len())
                .filter(|n| *n <= limits.expanded_bytes)
                .ok_or("APFL expanded budget")?;
        }
    }
    if size > limits.wire_bytes {
        return Err("APFL wire budget");
    }
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(rows as u16).to_le_bytes());
    out.extend_from_slice(&(columns.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&schema.to_le_bytes());
    out.extend_from_slice(&context);
    out.extend_from_slice(&(expanded as u32).to_le_bytes());
    for column in columns {
        out.extend_from_slice(&(column.values.len() as u32).to_le_bytes());
        for value in &column.values {
            out.extend_from_slice(&(value.len() as u32).to_le_bytes());
            out.extend_from_slice(value);
        }
        for index in &column.indexes {
            out.extend_from_slice(&index.to_le_bytes());
        }
    }
    Ok(out)
}
fn validate_column(column: &Column<&[u8]>, rows: usize) -> Result<()> {
    let n = column.values.len();
    if n == 0 || n > rows || column.values.iter().collect::<BTreeSet<_>>().len() != n {
        return Err("APFL dictionary");
    }
    if n == 1 || n == rows {
        if !column.indexes.is_empty() {
            return Err("APFL implicit indexes");
        }
    } else {
        if column.indexes.len() != rows {
            return Err("APFL index count");
        }
        let mut next = 0;
        for &index in &column.indexes {
            if index as usize >= n || index > next {
                return Err("APFL noncanonical index");
            }
            if index == next {
                next += 1;
            }
        }
        if next as usize != n {
            return Err("APFL unused dictionary value");
        }
    }
    Ok(())
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let result = self.0.get(..n).ok_or("APFL truncated")?;
        self.0 = &self.0[n..];
        Ok(result)
    }
    fn u32(&mut self) -> Result<usize> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }
}
pub fn decode(bytes: &[u8], schema: u64, limits: Limits) -> Result<BatchView<'_>> {
    limits.check()?;
    if bytes.len() < HEADER
        || bytes.len() > limits.wire_bytes
        || &bytes[..8] != MAGIC
        || bytes[8..10] != 1u16.to_le_bytes()
        || bytes[14..16] != [0, 0]
    {
        return Err("APFL header");
    }
    let rows = u16::from_le_bytes(bytes[10..12].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(bytes[12..14].try_into().unwrap()) as usize;
    let actual_schema = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let declared = u32::from_le_bytes(bytes[56..60].try_into().unwrap()) as usize;
    if schema == 0
        || actual_schema != schema
        || rows == 0
        || rows > limits.rows
        || count == 0
        || count > limits.columns
        || declared > limits.expanded_bytes
    {
        return Err("APFL shape/schema/budget");
    }
    let mut reader = Reader(&bytes[HEADER..]);
    let mut columns = Vec::with_capacity(count);
    let mut expanded = 0usize;
    for _ in 0..count {
        let n = reader.u32()?;
        if n == 0 || n > rows || n > reader.0.len() / 4 {
            return Err("APFL dictionary count");
        }
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            let len = reader.u32()?;
            if len > limits.field_bytes {
                return Err("APFL field budget");
            }
            values.push(reader.take(len)?);
        }
        let mut indexes = Vec::new();
        if n != 1 && n != rows {
            if rows > reader.0.len() / 4 {
                return Err("APFL truncated indexes");
            }
            indexes.reserve(rows);
            for _ in 0..rows {
                indexes.push(reader.u32()? as u32);
            }
        }
        let column = Column { values, indexes };
        validate_column(&column, rows)?;
        for row in 0..rows {
            expanded = expanded
                .checked_add(column.value(row).len())
                .filter(|n| *n <= limits.expanded_bytes)
                .ok_or("APFL expanded budget")?;
        }
        columns.push(column);
    }
    if !reader.0.is_empty() || expanded != declared {
        return Err("APFL trailing/expanded mismatch");
    }
    Ok(BatchView {
        schema,
        context: bytes[24..56].try_into().unwrap(),
        rows,
        columns,
        expanded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const L: Limits = Limits {
        rows: 32,
        columns: 4,
        field_bytes: 1024,
        wire_bytes: 8192,
        expanded_bytes: 8192,
    };
    #[test]
    fn shared_dictionary_dense_and_empty_residuals_are_lossless_and_borrowed() {
        let shared = [7u8; 64];
        let columns = [
            Column::collect([shared.as_slice(); 4].into_iter()),
            Column::collect([b"a".as_slice(), b"b", b"a", b"b"].into_iter()),
            Column::collect([b"".as_slice(), b"1", b"2", b"3"].into_iter()),
        ];
        let bytes = encode(7, [9; 32], 4, &columns, L).unwrap();
        let view = decode(&bytes, 7, L).unwrap();
        assert_eq!(
            view.field(0, 0).unwrap().as_ptr(),
            view.field(3, 0).unwrap().as_ptr()
        );
        assert_eq!(
            view.field(1, 1).unwrap().as_ptr(),
            view.field(3, 1).unwrap().as_ptr()
        );
        for row in 0..4 {
            for (col, c) in columns.iter().enumerate() {
                assert_eq!(view.field(row, col).unwrap(), *c.value(row));
            }
        }
        assert!(bytes.len() < view.expanded_bytes());
        assert_eq!(encode(7, [9; 32], 4, &view.columns, L).unwrap(), bytes);
        for n in 0..bytes.len() {
            assert!(decode(&bytes[..n], 7, L).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode(&trailing, 7, L).is_err());
        assert!(decode(&bytes, 8, L).is_err());
        assert!(decode(
            &bytes,
            7,
            Limits {
                expanded_bytes: 64,
                ..L
            }
        )
        .is_err());
        let mut huge = bytes.clone();
        huge[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&huge, 7, L).is_err());
        let bad = Column {
            values: vec![b"a".as_slice(), b"b"],
            indexes: vec![1, 0, 1, 0],
        };
        assert!(encode(7, [9; 32], 4, &[bad], L).is_err());
    }
}
