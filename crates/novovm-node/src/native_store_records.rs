//! Lossless physical storage layout for the existing native store.
//!
//! This root is NOT a consensus state/receipt root or an authority pointer.
//! It includes local diagnostics and exists only to recover the exact typed
//! image required by the current candidate loading/validation paths.
//! Maps grow by independent immutable records, not by one accumulated blob.

use super::NovNativeExecutionStoreV1;
use crate::native_state_records::{read_record, RecordChange, RecordOverlayV1, RecordStatsDeltaV1};
use anyhow::{bail, Context, Result};
#[cfg(test)]
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::value::RawValue;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Records = BTreeMap<Vec<u8>, Vec<u8>>;
type Object = BTreeMap<String, Box<RawValue>>;
pub(super) const OBJECT: &[u8] = b"\0";

pub(super) trait NativeRecordAccessV1 {
    /// Returns a raw typed JSON token; structural object markers are exposed as
    /// `{}`. A missing record is distinct from an existing empty object.
    fn read_path(&self, path: &[&str]) -> Result<Option<Vec<u8>>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RawPathChangeV1 {
    Put { path: Vec<String>, value: Vec<u8> },
    Delete { path: Vec<String> },
}

/// Serialize from the concrete Rust type, never through Value/f64. This is the
/// physical layout's scalar/struct spelling; consensus canonicalization is a
/// separate projection and must not change the recoverable physical image.
pub(super) fn typed_raw_v1<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

#[cfg(test)]
pub(super) fn read_typed_path_v1<T: DeserializeOwned>(
    reader: &dyn NativeRecordAccessV1,
    path: &[&str],
) -> Result<Option<T>> {
    reader
        .read_path(path)?
        .map(|bytes| Ok(serde_json::from_slice(&bytes)?))
        .transpose()
}

pub(super) fn key(path: &[String]) -> Result<Vec<u8>> {
    let encoded = serde_json::to_vec(path)?;
    let mut bytes = b"NDS1".to_vec();
    bytes.extend_from_slice(&super::sha256_bytes_v1(&[
        b"novovm-native-store-record-path-v1\0",
        &encoded,
    ]));
    Ok(bytes)
}

pub(super) fn value(parts: &[String], raw: &[u8]) -> Result<Vec<u8>> {
    let encoded = serde_json::to_vec(parts)?;
    let mut bytes = b"NSV1".to_vec();
    bytes.extend_from_slice(&u32::try_from(encoded.len())?.to_be_bytes());
    bytes.extend_from_slice(&encoded);
    bytes.extend_from_slice(raw);
    Ok(bytes)
}

pub(super) fn unpack<'a>(record_key: &[u8], bytes: &'a [u8]) -> Result<(Vec<String>, &'a [u8])> {
    if bytes.len() < 8 || &bytes[..4] != b"NSV1" {
        bail!("native store record value codec mismatch");
    }
    let len = usize::try_from(u32::from_be_bytes(bytes[4..8].try_into()?))?;
    let end = 8usize
        .checked_add(len)
        .context("record path length overflow")?;
    let encoded = bytes.get(8..end).context("truncated record path")?;
    let parts: Vec<String> = serde_json::from_slice(encoded)?;
    if parts.len() > 4 || serde_json::to_vec(&parts)? != encoded || key(&parts)? != record_key {
        bail!("native store record path hash/canonical encoding mismatch");
    }
    Ok((parts, &bytes[end..]))
}

fn module_maps() -> Result<&'static BTreeSet<String>> {
    static MAPS: std::sync::OnceLock<std::result::Result<BTreeSet<String>, String>> =
        std::sync::OnceLock::new();
    MAPS.get_or_init(|| {
        let defaults = super::NovNativeExecutionModuleStateV1::default();
        let fields: Object = serde_json::to_vec(&defaults)
            .and_then(|bytes| serde_json::from_slice(&bytes))
            .map_err(|error| error.to_string())?;
        Ok(fields
            .into_iter()
            .filter_map(|(name, value)| (value.get() == "{}").then_some(name))
            .collect())
    })
    .as_ref()
    .map_err(|error| anyhow::anyhow!("native module map schema: {error}"))
}

fn split_object(parts: &[String], maps: &BTreeSet<String>) -> bool {
    match parts {
        [] => true,
        [field] => field == "module_state" || field == "receipts",
        [module, field] => module == "module_state" && maps.contains(field),
        [module, field, _] => module == "module_state" && field == "account_asset_balances",
        _ => false,
    }
}

pub(super) fn is_object_path_v1(parts: &[String]) -> Result<bool> {
    Ok(split_object(parts, module_maps()?))
}

/// Convert an API JSON token to the exact physical marker/value spelling.
/// Putting `{}` at a structural path creates/preserves the marker, not a
/// subtree replacement. Removing a subtree requires explicit child deletes.
pub(super) fn physical_path_value_v1(parts: &[String], raw: &[u8]) -> Result<Vec<u8>> {
    if parts.len() > 4 {
        bail!("native store record path exceeds layout depth");
    }
    if is_object_path_v1(parts)? {
        let object: Object = serde_json::from_slice(raw)?;
        if !object.is_empty() {
            bail!("structural record put must contain an empty object marker");
        }
        return Ok(OBJECT.to_vec());
    }
    let parsed: Box<RawValue> = serde_json::from_slice(raw)?;
    Ok(parsed.get().as_bytes().to_vec())
}

impl NativeRecordAccessV1 for RecordOverlayV1<'_> {
    fn read_path(&self, path: &[&str]) -> Result<Option<Vec<u8>>> {
        let parts: Vec<String> = path.iter().map(|part| (*part).to_owned()).collect();
        let record_key = key(&parts)?;
        let Some(bytes) = read_record(self, self.root(), &record_key)? else {
            return Ok(None);
        };
        let (stored, raw) = unpack(&record_key, &bytes)?;
        if stored != parts {
            bail!("native record lookup path differs from requested path");
        }
        if is_object_path_v1(&parts)? {
            if raw != OBJECT {
                bail!("native record structural marker missing");
            }
            return Ok(Some(b"{}".to_vec()));
        }
        let parsed: Box<RawValue> = serde_json::from_slice(raw)?;
        Ok(Some(parsed.get().as_bytes().to_vec()))
    }
}

#[cfg(test)]
pub(super) fn apply_raw_path_changes_v1(
    overlay: &mut RecordOverlayV1<'_>,
    changes: &[RawPathChangeV1],
) -> Result<()> {
    apply_raw_changes_to_overlay_v1(overlay, changes).map(|_| ())
}

fn validate_update_path(parts: &[String]) -> Result<()> {
    static FIELDS: std::sync::OnceLock<std::result::Result<BTreeSet<Vec<String>>, String>> =
        std::sync::OnceLock::new();
    let fields = FIELDS
        .get_or_init(|| {
            encode(&NovNativeExecutionStoreV1::default())
                .and_then(|records| {
                    records
                        .iter()
                        .map(|(key, value)| unpack(key, value).map(|(path, _)| path))
                        .collect()
                })
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!("native record path schema: {error}"))?;
    if parts.len() > 4 {
        bail!("native store record path exceeds layout depth");
    }
    if parts.is_empty() || parts.len() == 1 || (parts.len() == 2 && parts[0] == "module_state") {
        if !fields.contains(parts) {
            bail!("unknown native store record field");
        }
    } else {
        let parent = &parts[..parts.len() - 1];
        validate_update_path(parent)?;
        if !is_object_path_v1(parent)? {
            bail!("native store record parent is not a structural object");
        }
    }
    Ok(())
}

fn read_checked_physical(
    overlay: &RecordOverlayV1<'_>,
    parts: &[String],
) -> Result<Option<Vec<u8>>> {
    let record_key = key(parts)?;
    let Some(bytes) = read_record(overlay, overlay.root(), &record_key)? else {
        return Ok(None);
    };
    let (stored, raw) = unpack(&record_key, &bytes)?;
    if stored != parts {
        bail!("native record hash resolves to a different original path");
    }
    if is_object_path_v1(parts)? {
        if raw != OBJECT {
            bail!("native record structural marker missing");
        }
    } else {
        let parsed: Box<RawValue> = serde_json::from_slice(raw)?;
        if parsed.get().as_bytes() != raw {
            bail!("native record value has noncanonical outer whitespace");
        }
    }
    Ok(Some(bytes))
}

/// Apply a unique raw-path patch and return exact live-record/blob byte deltas.
/// Parent markers may be created in this same patch, in any order. Existing
/// markers mean `{}` is a no-op, never replacement of their children. Structural
/// deletes are deliberately unsupported: without enumerating descendants they
/// cannot establish that the result has no orphans. Semantic field/type rules
/// and the truth of the supplied parent root remain the caller's responsibility.
pub(super) fn apply_raw_changes_to_overlay_v1(
    overlay: &mut RecordOverlayV1<'_>,
    changes: &[RawPathChangeV1],
) -> Result<RecordStatsDeltaV1> {
    use crate::native_state_records::{MAX_STAGE_BLOB_BYTES, MAX_STAGE_CHANGES};
    if changes.len() > MAX_STAGE_CHANGES {
        bail!("too many raw record changes in one stage");
    }
    let mut keys = BTreeSet::new();
    let mut markers = BTreeSet::new();
    let mut converted = Vec::with_capacity(changes.len());
    let mut byte_budget = 0usize;
    for change in changes {
        let parts = match change {
            RawPathChangeV1::Put { path, .. } | RawPathChangeV1::Delete { path } => path,
        };
        validate_update_path(parts)?;
        let record_key = key(parts)?;
        if !keys.insert(record_key.clone()) {
            bail!("duplicate native record path or path hash");
        }
        let converted_change = match change {
            RawPathChangeV1::Put { value: raw, .. } => {
                // Bound the input before parsing/copying a possibly large JSON
                // value. The generic record layer also checks the packed limit.
                byte_budget = byte_budget
                    .checked_add(raw.len())
                    .context("raw record byte budget overflow")?;
                if raw.len() > crate::native_state_records::MAX_RECORD_VALUE_BYTES_V1
                    || byte_budget > MAX_STAGE_BLOB_BYTES
                {
                    bail!("raw record patch exceeds resource budget");
                }
                let raw = physical_path_value_v1(parts, raw)?;
                if raw == OBJECT {
                    markers.insert(parts.clone());
                }
                RecordChange::Put {
                    key: record_key,
                    value: value(parts, &raw)?,
                }
            }
            RawPathChangeV1::Delete { .. } => {
                if is_object_path_v1(parts)? {
                    bail!("native record structural deletion requires explicit subtree handling");
                }
                RecordChange::Delete { key: record_key }
            }
        };
        converted.push(converted_change);
    }
    for change in changes {
        let parts = match change {
            RawPathChangeV1::Put { path, .. } | RawPathChangeV1::Delete { path } => path,
        };
        // Validate inherited values even for a replacing put or no-op delete.
        read_checked_physical(overlay, parts)?;
        for length in 0..parts.len() {
            let parent = &parts[..length];
            if read_checked_physical(overlay, parent)?.is_none() && !markers.contains(parent) {
                bail!("native record patch has a missing structural parent");
            }
        }
    }
    overlay.stage_with_stats(&converted)
}

fn visit_object(
    value: &RawValue,
    parts: &mut Vec<String>,
    maps: &BTreeSet<String>,
    out: &mut Records,
) -> Result<()> {
    let record_key = key(parts)?;
    if split_object(parts, maps) {
        out.insert(record_key, self::value(parts, OBJECT)?);
        let object: Object = serde_json::from_str(value.get())?;
        for (name, value) in object {
            parts.push(name);
            visit_object(&value, parts, maps, out)?;
            parts.pop();
        }
    } else {
        out.insert(record_key, self::value(parts, value.get().as_bytes())?);
    }
    Ok(())
}

/// Cold state-only conversion; deliberately does not serialize history receipts.
pub(super) fn encode_module_v1(state: &super::NovNativeExecutionModuleStateV1) -> Result<Records> {
    let raw: Box<RawValue> = serde_json::from_slice(&typed_raw_v1(state)?)?;
    let mut records = Records::new();
    visit_object(
        &raw,
        &mut vec!["module_state".into()],
        module_maps()?,
        &mut records,
    )?;
    Ok(records)
}

/// Cold-path conversion only. RawValue preserves every u128/i128 JSON token;
/// routing through serde_json::Value would round values above u64 through f64.
pub(super) fn encode(store: &NovNativeExecutionStoreV1) -> Result<Records> {
    let raw: Box<RawValue> = serde_json::from_slice(&typed_raw_v1(store)?)?;
    let mut records = Records::new();
    visit_object(&raw, &mut Vec::new(), module_maps()?, &mut records)?;
    Ok(records)
}

/// Reject unknown records, missing defaulted fields, noncanonical values, and
/// orphan entries rather than silently synthesizing a partly recovered store.
pub(super) fn decode(records: Records) -> Result<NovNativeExecutionStoreV1> {
    fn assemble(
        parts: &[String],
        values: &BTreeMap<Vec<String>, Vec<u8>>,
        children: &BTreeMap<Vec<String>, BTreeSet<String>>,
    ) -> Result<Box<RawValue>> {
        let value = values
            .get(parts)
            .context("native store record parent missing")?;
        if value.as_slice() == OBJECT {
            let mut object = Object::new();
            if let Some(names) = children.get(parts) {
                for name in names {
                    let mut next = parts.to_vec();
                    next.push(name.clone());
                    object.insert(name.clone(), assemble(&next, values, children)?);
                }
            }
            Ok(serde_json::value::to_raw_value(&object)?)
        } else {
            if children.contains_key(parts) {
                bail!("native store scalar record has children");
            }
            Ok(serde_json::from_slice(value)?)
        }
    }
    let mut values = BTreeMap::new();
    let mut children: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();
    for (record_key, value) in &records {
        let (parts, raw) = unpack(record_key, value)?;
        if let Some((name, parent)) = parts.split_last() {
            children
                .entry(parent.to_vec())
                .or_default()
                .insert(name.clone());
        }
        values.insert(parts, raw.to_vec());
    }
    let raw = assemble(&[], &values, &children)?;
    let store: NovNativeExecutionStoreV1 = serde_json::from_str(raw.get())?;
    if encode(&store)? != records {
        bail!("native store records do not reproduce the complete canonical image");
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_state_records::StateRecordReader;
    use crate::native_state_tree::{empty_root, NodeHash, StateNodeReader};

    struct EmptyReader;
    impl StateNodeReader for EmptyReader {
        fn read_node(&self, _: &NodeHash) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
    }
    impl StateRecordReader for EmptyReader {
        fn read_record_chunk(&self, _: NodeHash, _: u32) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
    }
    fn seeded<'a>(
        reader: &'a EmptyReader,
        store: &NovNativeExecutionStoreV1,
    ) -> RecordOverlayV1<'a> {
        let mut overlay = RecordOverlayV1::new(reader, empty_root());
        overlay
            .stage(
                &encode(store)
                    .unwrap()
                    .into_iter()
                    .map(|(key, value)| RecordChange::Put { key, value })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        overlay
    }
    fn raw_put(parts: &[&str], raw: &[u8]) -> RawPathChangeV1 {
        RawPathChangeV1::Put {
            path: parts.iter().map(|part| (*part).to_owned()).collect(),
            value: raw.to_vec(),
        }
    }
    fn totals(store: &NovNativeExecutionStoreV1) -> (usize, usize) {
        let records = encode(store).unwrap();
        (
            records.len(),
            records
                .iter()
                .map(|(key, value)| 10 + key.len() + value.len())
                .sum(),
        )
    }

    #[test]
    fn raw_patch_stats_match_cold_map_and_allow_same_patch_parent_in_any_order() {
        let reader = EmptyReader;
        let mut expected = NovNativeExecutionStoreV1::default();
        expected.module_state.account_asset_balances.insert(
            "alice".into(),
            BTreeMap::from([("NOV".into(), u128::MAX), ("USDT".into(), 3)]),
        );
        let mut overlay = seeded(&reader, &expected);
        let mut counts = totals(&expected);
        let changes = [
            raw_put(
                &["module_state", "account_asset_balances", "bob", "NOV"],
                b"1",
            ),
            raw_put(
                &["module_state", "account_asset_balances", "alice", "NOV"],
                b"42",
            ),
            raw_put(&["module_state", "account_asset_balances", "bob"], b"{ }"),
        ];
        let delta = apply_raw_changes_to_overlay_v1(&mut overlay, &changes).unwrap();
        counts = delta.checked_apply(counts.0, counts.1).unwrap();
        expected
            .module_state
            .account_asset_balances
            .insert("bob".into(), BTreeMap::from([("NOV".into(), 1)]));
        expected
            .module_state
            .account_asset_balances
            .get_mut("alice")
            .unwrap()
            .insert("NOV".into(), 42);
        assert_eq!(counts, totals(&expected));
        assert_eq!(overlay.root(), seeded(&reader, &expected).root());
        let root = overlay.root();
        assert_eq!(
            apply_raw_changes_to_overlay_v1(&mut overlay, &changes).unwrap(),
            RecordStatsDeltaV1::default()
        );
        assert_eq!(root, overlay.root());
        let changes = [
            raw_put(
                &["module_state", "account_asset_balances", "alice", "NOV"],
                u128::MAX.to_string().as_bytes(),
            ),
            RawPathChangeV1::Delete {
                path: vec![
                    "module_state".into(),
                    "account_asset_balances".into(),
                    "alice".into(),
                    "USDT".into(),
                ],
            },
            RawPathChangeV1::Delete {
                path: vec![
                    "module_state".into(),
                    "native_auth_next_nonces".into(),
                    "absent".into(),
                ],
            },
        ];
        counts = apply_raw_changes_to_overlay_v1(&mut overlay, &changes)
            .unwrap()
            .checked_apply(counts.0, counts.1)
            .unwrap();
        let balances = expected
            .module_state
            .account_asset_balances
            .get_mut("alice")
            .unwrap();
        balances.insert("NOV".into(), u128::MAX);
        balances.remove("USDT");
        assert_eq!(counts, totals(&expected));
        assert_eq!(overlay.root(), seeded(&reader, &expected).root());
        assert_eq!(
            read_typed_path_v1::<u128>(
                &overlay,
                &["module_state", "account_asset_balances", "alice", "NOV"]
            )
            .unwrap(),
            Some(u128::MAX)
        );
    }

    #[test]
    fn raw_patch_rejects_duplicates_unknown_paths_missing_parents_and_structural_deletes() {
        let reader = EmptyReader;
        let mut overlay = seeded(&reader, &NovNativeExecutionStoreV1::default());
        let initial = overlay.root();
        let normal = raw_put(&["module_state", "treasury_settled_nov_total"], b"3");
        for changes in [
            vec![normal.clone(), normal],
            vec![raw_put(&["unknown"], b"1")],
            vec![raw_put(
                &["module_state", "treasury_settled_nov_total", "child"],
                b"1",
            )],
            vec![raw_put(
                &["module_state", "account_asset_balances", "missing", "NOV"],
                b"1",
            )],
            vec![raw_put(
                &["module_state", "account_asset_balances"],
                br#"{"alice":{}}"#,
            )],
            vec![RawPathChangeV1::Delete {
                path: vec!["module_state".into(), "account_asset_balances".into()],
            }],
            vec![raw_put(
                &["module_state", "treasury_settled_nov_total"],
                b"\0",
            )],
        ] {
            assert!(apply_raw_changes_to_overlay_v1(&mut overlay, &changes).is_err());
            assert_eq!(overlay.root(), initial);
        }
        // A valid outer blob/leaf cannot hide an NSV1 original-path mismatch.
        let parts = vec!["module_state".into(), "treasury_settled_nov_total".into()];
        overlay
            .stage(&[RecordChange::Put {
                key: key(&parts).unwrap(),
                value: value(
                    &["module_state".into(), "treasury_settlements".into()],
                    b"1",
                )
                .unwrap(),
            }])
            .unwrap();
        let bad_root = overlay.root();
        assert!(apply_raw_changes_to_overlay_v1(
            &mut overlay,
            &[raw_put(
                &["module_state", "treasury_settled_nov_total"],
                b"1"
            )]
        )
        .is_err());
        assert_eq!(overlay.root(), bad_root);
    }

    #[test]
    fn store_records_losslessly_preserve_u128_and_empty_accounts() {
        let mut store = NovNativeExecutionStoreV1 {
            authority_chain_id: Some(u64::MAX),
            last_updated_unix_ms: u128::MAX,
            ..Default::default()
        };
        store.module_state.treasury_settled_nov_total = u128::MAX - 1;
        store
            .module_state
            .account_asset_balances
            .insert("empty".into(), BTreeMap::new());
        store.module_state.account_asset_balances.insert(
            "0xfull".into(),
            BTreeMap::from([("NOV".into(), u128::MAX), ("USDT".into(), u128::MAX - 3)]),
        );
        let encoded = encode(&store).unwrap();
        assert_eq!(decode(encoded.clone()).unwrap(), store);
        store
            .module_state
            .account_asset_balances
            .get_mut("0xfull")
            .unwrap()
            .insert("NOV".into(), 0);
        let changed = encode(&store).unwrap();
        assert_eq!(
            encoded
                .iter()
                .filter(|(key, value)| changed.get(*key) != Some(*value))
                .count(),
            1
        );
        assert_eq!(encoded.len(), changed.len());
    }

    #[test]
    fn store_records_missing_unknown_or_orphan_records_fail_closed() {
        let store = NovNativeExecutionStoreV1::default();
        let original = encode(&store).unwrap();
        let mut missing = original.clone();
        missing
            .remove(&key(&["module_state".into(), "treasury_settled_nov_total".into()]).unwrap());
        assert!(decode(missing).is_err());
        let mut unknown = original.clone();
        let parts = ["unrecognized".into()];
        unknown.insert(key(&parts).unwrap(), value(&parts, b"1").unwrap());
        assert!(decode(unknown).is_err());
        let mut orphan = original;
        let parts = ["not_present".into(), "child".into()];
        orphan.insert(key(&parts).unwrap(), value(&parts, b"1").unwrap());
        assert!(decode(orphan).is_err());
    }

    #[test]
    fn record_keys_bind_components_without_separator_aliases() {
        let a = vec!["a/b".into(), "c".into()];
        let b = vec!["a".into(), "b/c".into()];
        assert_ne!(key(&a).unwrap(), key(&b).unwrap());
        assert_eq!(
            unpack(&key(&a).unwrap(), &value(&a, b"1").unwrap())
                .unwrap()
                .0,
            a
        );
        let long = vec!["a".repeat(257)];
        assert_eq!(key(&long).unwrap().len(), 36);
        assert_eq!(
            unpack(&key(&long).unwrap(), &value(&long, b"1").unwrap())
                .unwrap()
                .0,
            long
        );
        assert!(unpack(&key(&b).unwrap(), &value(&a, b"1").unwrap()).is_err());
        let too_deep = vec!["component".into(); 5];
        assert!(unpack(&key(&too_deep).unwrap(), &value(&too_deep, b"1").unwrap()).is_err());
    }
}
