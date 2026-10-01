use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

fn envelope(opcode: u16, status: i32, payload: &[u8]) -> Vec<u8> {
    let mut response = b"AOSR".to_vec();
    response.extend_from_slice(&1u16.to_le_bytes());
    response.extend_from_slice(&opcode.to_le_bytes());
    response.extend_from_slice(&status.to_le_bytes());
    response.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    response.extend_from_slice(payload);
    response
}

fn put(key: &[u8], value: &[u8]) -> StorageWrite {
    StorageWrite::Put {
        key: key.to_vec(),
        value: value.to_vec(),
    }
}

#[test]
fn storage_environment_requires_real_durability_without_changing_global_settings() {
    assert!(validate_environment(None, None).is_ok());
    assert!(validate_environment(None, Some(OsStr::new(""))).is_ok());
    for value in ["", "0", "false", "1"] {
        assert!(validate_environment(Some(OsStr::new(value)), None).is_err());
    }
    for value in ["private-storage-path", " ", "\t"] {
        let error = validate_environment(None, Some(OsStr::new(value)))
            .unwrap_err()
            .to_string();
        assert!(error.contains("AOEM_PERSISTENCE_PATH"));
        assert!(!error.contains("private-storage-path"));
    }
}

#[test]
fn storage_open_wire_has_explicit_path_and_mandatory_sync_one() {
    assert_eq!(std::mem::size_of::<CreateOptions>(), 16);
    let config = StorageConfig {
        max_open_files: 23,
        write_buffer_bytes: 12345,
        block_cache_bytes: 54321,
        max_background_jobs: 2,
        compression: false,
        ..StorageConfig::default()
    };
    let actual = wire::open_request("chosen-db", &config).unwrap();
    let mut payload = 9u32.to_le_bytes().to_vec();
    payload.extend_from_slice(b"chosen-db");
    payload.extend_from_slice(&23u32.to_le_bytes());
    payload.extend_from_slice(&12345u64.to_le_bytes());
    payload.extend_from_slice(&54321u64.to_le_bytes());
    payload.extend_from_slice(&2u32.to_le_bytes());
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.push(0);
    let mut expected = b"AOSQ\x01\x00\x01\x00".to_vec();
    expected.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    expected.extend_from_slice(&payload);
    assert_eq!(actual, expected);
    assert!(wire::open_request("", &config).is_err());
    assert!(wire::open_request("a\0b", &config).is_err());
}

#[test]
fn write_wire_is_one_ordered_batch_including_empty_values_and_deletion() {
    let writes = [
        put(b"k", b"one"),
        StorageWrite::Delete { key: b"k".to_vec() },
        put(b"k", b""),
    ];
    let request = wire::write_request(17, &writes, StorageLimits::default()).unwrap();
    let mut expected = 17u64.to_le_bytes().to_vec();
    expected.extend_from_slice(&3u32.to_le_bytes());
    expected.extend_from_slice(&[1, 1, 0, 0, 0, b'k', 3, 0, 0, 0, b'o', b'n', b'e']);
    expected.extend_from_slice(&[2, 1, 0, 0, 0, b'k']);
    expected.extend_from_slice(&[1, 1, 0, 0, 0, b'k', 0, 0, 0, 0]);
    assert_eq!(&request[..8], b"AOSQ\x01\x00\x05\x00");
    assert_eq!(
        u32::from_le_bytes(request[8..12].try_into().unwrap()) as usize,
        expected.len()
    );
    assert_eq!(&request[12..], expected);
}

#[test]
fn storage_wire_checks_counts_bytes_and_worst_case_read_response_before_admission() {
    let limits = StorageLimits {
        max_items: 2,
        max_key_bytes: 3,
        max_value_bytes: 8,
        max_request_bytes: 64,
        max_response_bytes: 64,
    };
    assert!(wire::write_request(1, &[put(b"abc", &[4; 8])], limits).is_ok());
    assert!(wire::write_request(0, &[put(b"k", b"v")], limits).is_err());
    assert!(wire::write_request(1, &[], limits).is_err());
    assert!(wire::write_request(1, &[put(b"", b"v")], limits).is_err());
    assert!(wire::write_request(1, &[put(b"abcd", b"v")], limits).is_err());
    assert!(wire::write_request(1, &[put(b"k", &[4; 9])], limits).is_err());
    assert!(wire::write_request(
        1,
        &[put(b"k", b"v"), put(b"k", b"v"), put(b"k", b"v")],
        limits
    )
    .is_err());
    let byte_limited = StorageLimits {
        max_items: 10,
        max_value_bytes: 30,
        ..limits
    };
    assert!(wire::write_request(
        1,
        &[put(b"key", &[0; 30]), put(b"key", &[0; 30])],
        byte_limited
    )
    .is_err());
    assert!(wire::read_request(1, &[b"a", b"a"], false, byte_limited).is_err());
    assert!(wire::read_request(1, &[b"a", b"a"], false, limits).is_ok());
    assert!(wire::read_request(1, &[b"a", b"a"], true, limits).is_err());
    assert!(wire::read_request(1, &[b""], true, limits).is_err());
    let invalid = StorageLimits {
        max_items: usize::MAX,
        ..limits
    };
    assert!(invalid.validate().is_err());
    assert!(StorageLimits {
        max_response_bytes: usize::MAX,
        ..limits
    }
    .validate()
    .is_err());
    assert!(StorageLimits {
        max_value_bytes: usize::MAX,
        ..limits
    }
    .validate()
    .is_err());
}

#[test]
fn response_envelope_and_acknowledgement_are_exact_not_just_zero_return_code() {
    let good = envelope(wire::WRITE_BATCH, 0, &3u32.to_le_bytes());
    assert!(wire::write_ack(
        wire::response_payload(&good, wire::WRITE_BATCH, 0).unwrap(),
        3
    )
    .is_ok());
    assert!(wire::write_ack(&3u32.to_le_bytes(), 2).is_err());
    assert!(wire::write_ack(&[3, 0, 0, 0, 0], 3).is_err());
    assert!(wire::database_id(&0u64.to_le_bytes()).is_err());
    assert!(wire::database_id(&[1]).is_err());
    assert_eq!(wire::database_id(&17u64.to_le_bytes()).unwrap(), 17);
    for index in [0, 4, 6, 8, 12] {
        let mut damaged = good.clone();
        damaged[index] ^= 1;
        assert!(wire::response_payload(&damaged, wire::WRITE_BATCH, 0).is_err());
    }
    for length in 0..good.len() {
        assert!(wire::response_payload(&good[..length], wire::WRITE_BATCH, 0).is_err());
    }
    assert!(wire::response_payload(&good, wire::WRITE_BATCH, -4).is_err());
    assert!(wire::response_payload(
        &envelope(wire::WRITE_BATCH, -4, b"backend failed"),
        wire::WRITE_BATCH,
        -4
    )
    .unwrap_err()
    .to_string()
    .contains("backend failed"));
    let mut trailing = good;
    trailing.push(0);
    assert!(wire::response_payload(&trailing, wire::WRITE_BATCH, 0).is_err());
}

#[test]
fn read_values_preserve_absence_empty_and_order_and_reject_any_bad_prefix() {
    let limits = StorageLimits::default();
    let mut payload = 4u32.to_le_bytes().to_vec();
    payload.extend_from_slice(&[1, 1, 0, 0, 0, b'a']);
    payload.extend_from_slice(&[0, 0, 0, 0, 0]);
    payload.extend_from_slice(&[1, 0, 0, 0, 0]);
    payload.extend_from_slice(&[1, 1, 0, 0, 0, b'a']);
    assert_eq!(
        wire::values(&payload, 4, limits).unwrap(),
        vec![
            Some(b"a".to_vec()),
            None,
            Some(Vec::new()),
            Some(b"a".to_vec())
        ]
    );
    for length in 0..payload.len() {
        assert!(wire::values(&payload[..length], 4, limits).is_err());
    }
    let mut unknown_presence = payload.clone();
    unknown_presence[4] = 2;
    assert!(wire::values(&unknown_presence, 4, limits).is_err());
    let mut absent_with_bytes = payload.clone();
    absent_with_bytes[4] = 0;
    assert!(wire::values(&absent_with_bytes, 4, limits).is_err());
    let mut trailing = payload;
    trailing.push(0);
    assert!(wire::values(&trailing, 4, limits).is_err());
}

fn library_path() -> PathBuf {
    std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .expect("set explicit trusted AOEM test DLL/SO")
        .into()
}

fn temporary_database(label: &str) -> PathBuf {
    // Never touch an existing operator database or remove a broad directory.
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/storage-tests");
    std::fs::create_dir_all(&parent).expect("create repository-local storage test directory");
    parent.join(format!(
        "novovm-new-storage-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn small_config() -> StorageConfig {
    StorageConfig {
        max_open_files: 64,
        write_buffer_bytes: 4 * 1024 * 1024,
        block_cache_bytes: 4 * 1024 * 1024,
        max_background_jobs: 2,
        compression: false,
        ..StorageConfig::default()
    }
}

#[test]
#[ignore = "requires explicit packaged AOEM library; storage component, not chain durability acceptance"]
fn real_storage_atomic_batch_order_multiget_fresh_read_and_cold_reopen() -> Result<()> {
    let database = temporary_database("roundtrip");
    let library = library_path();
    let mut storage = StorageSession::open(&library, &database, small_config())?;
    assert_eq!(storage.get(b"missing")?, None);
    assert_eq!(storage.multi_get(&[])?, Vec::<Option<Vec<u8>>>::new());
    assert!(storage.atomic_write_batch(&[]).is_err());
    assert!(!storage.is_poisoned());
    storage.atomic_write_batch(&[
        put(b"data", b"first"),
        put(b"empty", b""),
        StorageWrite::Delete {
            key: b"data".to_vec(),
        },
        put(b"data", b"final"),
        put(b"marker", b"same atomic batch"),
    ])?;
    assert_eq!(
        storage.multi_get(&[
            b"data".to_vec(),
            b"absent".to_vec(),
            b"empty".to_vec(),
            b"data".to_vec()
        ])?,
        vec![
            Some(b"final".to_vec()),
            None,
            Some(Vec::new()),
            Some(b"final".to_vec())
        ]
    );
    storage.atomic_write_batch(&[
        put(b"data", b"new value"),
        StorageWrite::Delete {
            key: b"empty".to_vec(),
        },
    ])?;
    assert_eq!(storage.get(b"data")?, Some(b"new value".to_vec()));
    assert_eq!(storage.get(b"empty")?, None);
    drop(storage);
    let mut reopened = StorageSession::open(&library, &database, small_config())?;
    assert_eq!(reopened.get(b"data")?, Some(b"new value".to_vec()));
    assert_eq!(
        reopened.get(b"marker")?,
        Some(b"same atomic batch".to_vec())
    );
    assert_eq!(reopened.get(b"empty")?, None);
    // This is a cold session, NOT an independent process or power-loss test.
    Ok(())
}

#[test]
#[ignore = "requires explicit packaged AOEM library; native handle is created on its owner thread"]
fn real_storage_owner_threads_have_separate_databases_and_same_path_cannot_reopen() -> Result<()> {
    let library = library_path();
    let database = temporary_database("first-owner");
    let other_database = temporary_database("second-owner");
    let mut first = StorageSession::open(&library, &database, small_config())?;
    first.atomic_write_batch(&[put(b"key", b"first")])?;
    assert!(StorageSession::open(&library, &database.join("."), small_config()).is_err());
    let worker = std::thread::spawn(move || -> Result<_> {
        let mut second = StorageSession::open(&library, &other_database, small_config())?;
        second.atomic_write_batch(&[put(b"key", b"second")])?;
        second.get(b"key")
    });
    assert_eq!(worker.join().unwrap()?, Some(b"second".to_vec()));
    assert_eq!(first.get(b"key")?, Some(b"first".to_vec()));
    Ok(())
}

static REAL_WIRE: OnceLock<StorageWire> = OnceLock::new();
static REAL_FREE: OnceLock<Free> = OnceLock::new();
static FAULT_CALLS: AtomicUsize = AtomicUsize::new(0);
static RESPONSE_FREES: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn committed_but_bad_ack(
    handle: *mut c_void,
    request: *const u8,
    length: usize,
    out: *mut *mut u8,
    out_length: *mut usize,
) -> i32 {
    FAULT_CALLS.fetch_add(1, Ordering::SeqCst);
    let Some(call) = REAL_WIRE.get() else {
        return -1;
    };
    // SAFETY: forward the genuine ABI arguments without changing ownership.
    let status = unsafe { call(handle, request, length, out, out_length) };
    if status == 0 && !out.is_null() && !out_length.is_null() {
        // SAFETY: the real provider has returned an owned response allocation;
        // alter only the four-byte ack after the complete native write.
        unsafe {
            if !(*out).is_null() && *out_length == 20 {
                std::slice::from_raw_parts_mut(*out, *out_length)[16..20]
                    .copy_from_slice(&0u32.to_le_bytes());
            }
        }
    }
    status
}

unsafe extern "C" fn count_native_free(pointer: *mut u8, length: usize) {
    RESPONSE_FREES.fetch_add(1, Ordering::SeqCst);
    if let Some(free) = REAL_FREE.get() {
        // SAFETY: use the same allocating module's genuine free function.
        unsafe { free(pointer, length) };
    }
}

#[test]
#[ignore = "requires explicit packaged AOEM library; fault is injected only AFTER a genuine native write"]
fn real_write_bad_ack_is_unknown_poisoned_freed_and_never_automatically_retried() -> Result<()> {
    let database = temporary_database("unknown-write");
    let library = library_path();
    let mut storage = StorageSession::open(&library, &database, small_config())?;
    REAL_WIRE
        .set(storage.native.library.storage)
        .map_err(|_| anyhow::anyhow!("fixture initialized twice"))?;
    REAL_FREE
        .set(storage.native.library.free)
        .map_err(|_| anyhow::anyhow!("fixture initialized twice"))?;
    FAULT_CALLS.store(0, Ordering::SeqCst);
    RESPONSE_FREES.store(0, Ordering::SeqCst);
    storage.native.library.storage = committed_but_bad_ack;
    storage.native.library.free = count_native_free;
    let error = storage
        .atomic_write_batch(&[
            put(b"data", b"committed"),
            put(b"marker", b"committed together"),
        ])
        .unwrap_err();
    assert!(error.to_string().contains("acknowledgement count"));
    assert!(storage.is_poisoned());
    assert_eq!(FAULT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(RESPONSE_FREES.load(Ordering::SeqCst), 1);
    assert!(storage.get(b"data").is_err());
    assert!(storage.multi_get(&[]).is_err());
    assert!(storage
        .atomic_write_batch(&[put(b"data", b"must not write")])
        .is_err());
    assert_eq!(FAULT_CALLS.load(Ordering::SeqCst), 1);
    drop(storage);
    // Explicit recovery of a new owner, never an implicit retry. Both values
    // actually landed despite the error: UNKNOWN must not be called rollback.
    let mut recovered = StorageSession::open(&library, &database, small_config())?;
    assert_eq!(recovered.get(b"data")?, Some(b"committed".to_vec()));
    assert_eq!(
        recovered.get(b"marker")?,
        Some(b"committed together".to_vec())
    );
    Ok(())
}
