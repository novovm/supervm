// Fixed migration source: a7db795 runtime/novovm-aoem/src/storage_wire.rs.
//! Reviewed subset of the packaged AOSQ/AOSR v1 ABI; no application schema.

use super::{StorageConfig, StorageLimits, StorageWrite};
use anyhow::{bail, ensure, Context, Result};

pub(super) const MAX_WIRE_BYTES: usize = 256 * 1024 * 1024;
pub(super) const MAX_ITEMS: usize = 1_000_000;
pub(super) const OPEN: u16 = 1;
pub(super) const GET: u16 = 3;
pub(super) const MULTI_GET: u16 = 4;
pub(super) const WRITE_BATCH: u16 = 5;

fn add_length(size: &mut usize, extra: usize, max: usize) -> Result<()> {
    *size = size
        .checked_add(extra)
        .context("storage wire length overflow")?;
    ensure!(*size <= max, "storage wire request exceeds byte limit");
    Ok(())
}

fn start(opcode: u16, length: usize) -> Result<Vec<u8>> {
    let mut request = Vec::with_capacity(length);
    request.extend_from_slice(b"AOSQ");
    request.extend_from_slice(&1u16.to_le_bytes());
    request.extend_from_slice(&opcode.to_le_bytes());
    request.extend_from_slice(&u32::try_from(length - 12)?.to_le_bytes());
    Ok(request)
}

fn append_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    output.extend_from_slice(&u32::try_from(bytes.len())?.to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

pub(super) fn open_request(path: &str, config: &StorageConfig) -> Result<Vec<u8>> {
    config.validate()?;
    ensure!(
        !path.trim().is_empty() && !path.as_bytes().contains(&0),
        "storage path invalid"
    );
    let mut length = 12 + 4 + 4 + 8 + 8 + 4 + 4 + 1;
    add_length(&mut length, path.len(), config.limits.max_request_bytes)?;
    let mut request = start(OPEN, length)?;
    append_bytes(&mut request, path.as_bytes())?;
    request.extend_from_slice(&config.max_open_files.to_le_bytes());
    request.extend_from_slice(&config.write_buffer_bytes.to_le_bytes());
    request.extend_from_slice(&config.block_cache_bytes.to_le_bytes());
    request.extend_from_slice(&config.max_background_jobs.to_le_bytes());
    request.extend_from_slice(&1u32.to_le_bytes()); // mandatory sync_every=1
    request.push(u8::from(config.compression));
    Ok(request)
}

pub(super) fn read_request(
    database: u64,
    keys: &[&[u8]],
    single: bool,
    limits: StorageLimits,
) -> Result<Vec<u8>> {
    limits.validate()?;
    ensure!(database != 0, "storage database id is zero");
    ensure!(
        !keys.is_empty() && keys.len() <= limits.max_items && (!single || keys.len() == 1),
        "storage read count invalid"
    );
    // The native ABI lacks a per-read result cap. For a database written under
    // the same value bound, this controls worst-case native response size even
    // for duplicate keys. Foreign/externally corrupted values remain outside
    // that preallocation guarantee and are rejected after the native return.
    let max_result = limits
        .max_value_bytes
        .checked_add(5)
        .and_then(|bytes| bytes.checked_mul(keys.len()))
        .and_then(|bytes| bytes.checked_add(20))
        .context("storage read response bound overflow")?;
    ensure!(
        max_result <= limits.max_response_bytes,
        "storage read response budget exceeded"
    );
    let mut length = 12 + 8 + 8 + if single { 0 } else { 4 };
    for key in keys {
        ensure!(
            !key.is_empty() && key.len() <= limits.max_key_bytes,
            "storage read key outside bound"
        );
        add_length(&mut length, 4, limits.max_request_bytes)?;
        add_length(&mut length, key.len(), limits.max_request_bytes)?;
    }
    let mut request = start(if single { GET } else { MULTI_GET }, length)?;
    request.extend_from_slice(&database.to_le_bytes());
    request.extend_from_slice(&0u64.to_le_bytes()); // current live database
    if !single {
        request.extend_from_slice(&u32::try_from(keys.len())?.to_le_bytes());
    }
    for key in keys {
        append_bytes(&mut request, key)?;
    }
    Ok(request)
}

pub(super) fn write_request(
    database: u64,
    writes: &[StorageWrite],
    limits: StorageLimits,
) -> Result<Vec<u8>> {
    limits.validate()?;
    ensure!(database != 0, "storage database id is zero");
    ensure!(
        !writes.is_empty() && writes.len() <= limits.max_items,
        "storage write count outside bound"
    );
    let mut length = 12 + 8 + 4;
    for write in writes {
        let (key, value) = match write {
            StorageWrite::Put { key, value } => (key, Some(value)),
            StorageWrite::Delete { key } => (key, None),
        };
        ensure!(
            !key.is_empty() && key.len() <= limits.max_key_bytes,
            "storage write key outside bound"
        );
        add_length(&mut length, 1 + 4, limits.max_request_bytes)?;
        add_length(&mut length, key.len(), limits.max_request_bytes)?;
        if let Some(value) = value {
            ensure!(
                value.len() <= limits.max_value_bytes,
                "storage write value outside bound"
            );
            add_length(&mut length, 4, limits.max_request_bytes)?;
            add_length(&mut length, value.len(), limits.max_request_bytes)?;
        }
    }
    let mut request = start(WRITE_BATCH, length)?;
    request.extend_from_slice(&database.to_le_bytes());
    request.extend_from_slice(&u32::try_from(writes.len())?.to_le_bytes());
    for write in writes {
        match write {
            StorageWrite::Put { key, value } => {
                request.push(1);
                append_bytes(&mut request, key)?;
                append_bytes(&mut request, value)?;
            }
            StorageWrite::Delete { key } => {
                request.push(2);
                append_bytes(&mut request, key)?;
            }
        }
    }
    Ok(request)
}

pub(super) fn response_payload(response: &[u8], opcode: u16, status: i32) -> Result<&[u8]> {
    ensure!(
        response.len() >= 16 && &response[..4] == b"AOSR",
        "storage response header invalid"
    );
    ensure!(
        u16::from_le_bytes(response[4..6].try_into()?) == 1,
        "storage response version invalid"
    );
    ensure!(
        u16::from_le_bytes(response[6..8].try_into()?) == opcode,
        "storage response opcode mismatch"
    );
    let inner_status = i32::from_le_bytes(response[8..12].try_into()?);
    let length = usize::try_from(u32::from_le_bytes(response[12..16].try_into()?))?;
    ensure!(
        response.len() - 16 == length,
        "storage response length mismatch"
    );
    ensure!(
        status == inner_status,
        "storage response return status mismatch"
    );
    if status != 0 {
        let detail =
            std::str::from_utf8(&response[16..]).context("storage error detail is not UTF-8")?;
        bail!("AOEM storage provider rejected operation: status={status}: {detail}");
    }
    Ok(&response[16..])
}

pub(super) fn database_id(payload: &[u8]) -> Result<u64> {
    let id = u64::from_le_bytes(
        payload
            .try_into()
            .context("storage database id response length invalid")?,
    );
    ensure!(id != 0, "storage provider returned zero database id");
    Ok(id)
}

pub(super) fn write_ack(payload: &[u8], expected: usize) -> Result<()> {
    let count = u32::from_le_bytes(
        payload
            .try_into()
            .context("storage write acknowledgement length invalid")?,
    ) as usize;
    ensure!(
        count == expected,
        "storage write acknowledgement count mismatch"
    );
    Ok(())
}

pub(super) fn values(
    payload: &[u8],
    expected: usize,
    limits: StorageLimits,
) -> Result<Vec<Option<Vec<u8>>>> {
    ensure!(payload.len() >= 4, "storage read response truncated");
    let count = u32::from_le_bytes(payload[..4].try_into()?) as usize;
    ensure!(
        count == expected && count <= limits.max_items,
        "storage read response count mismatch"
    );
    let mut offset = 4usize;
    // Check the minimum encoding before allocating a result vector.
    ensure!(
        count
            .checked_mul(5)
            .is_some_and(|size| size <= payload.len() - 4),
        "storage read response entries truncated"
    );
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let header = payload
            .get(offset..offset + 5)
            .context("storage read value header truncated")?;
        offset += 5;
        let found = header[0];
        let length = u32::from_le_bytes(header[1..5].try_into()?) as usize;
        ensure!(
            found <= 1 && (found == 1 || length == 0),
            "storage read presence encoding invalid"
        );
        ensure!(
            length <= limits.max_value_bytes,
            "storage read value exceeds bound"
        );
        let end = offset
            .checked_add(length)
            .context("storage value length overflow")?;
        let value = payload
            .get(offset..end)
            .context("storage read value truncated")?;
        result.push(if found == 1 {
            Some(value.to_vec())
        } else {
            None
        });
        offset = end;
    }
    ensure!(
        offset == payload.len(),
        "storage read response has trailing bytes"
    );
    Ok(result)
}
