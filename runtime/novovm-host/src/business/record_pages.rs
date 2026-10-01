//! Bounded record pages for the replacement NOV business-state layout.
//!
//! Tree values remain at most 256 bytes. The record is a four-byte little-endian
//! length and a fixed declared range of 256-byte pages. A profile's prefix and
//! maximum size must be bound to its effect/layout contract: callers must not
//! reinterpret the same prefix under a smaller maximum to hide old tail pages.
//! This is a codec, not storage, authentication or permission to publish.

use crate::ingress::batch::SignatureCheckedInput;
use crate::state::frontier::DeclaredAccess;
use crate::state::tree::StateChange;
use anyhow::{ensure, Context, Result};

const PAGE_BYTES: usize = 256;
const MAX_RECORD_BYTES: usize = 8192;
const MAX_PREFIX_BYTES: usize = 256 - b"/page/".len() - 2;

fn page_count(max_bytes: usize) -> Result<usize> {
    ensure!(
        (1..=MAX_RECORD_BYTES).contains(&max_bytes),
        "record page profile must contain 1..=8192 bytes"
    );
    Ok(max_bytes
        .checked_add(PAGE_BYTES - 1)
        .context("record page count overflow")?
        / PAGE_BYTES)
}

fn keys(prefix: &[u8], max_bytes: usize) -> Result<(Vec<u8>, Vec<Vec<u8>>)> {
    let count = page_count(max_bytes)?;
    ensure!(
        !prefix.is_empty() && prefix.len() <= MAX_PREFIX_BYTES,
        "record prefix must leave room for bounded tree keys"
    );
    let mut header = prefix.to_vec();
    header.extend_from_slice(b"/len");
    let mut pages = Vec::with_capacity(count);
    for index in 0..count {
        let mut key = prefix.to_vec();
        key.extend_from_slice(b"/page/");
        key.extend_from_slice(&u16::try_from(index)?.to_be_bytes());
        pages.push(key);
    }
    Ok((header, pages))
}

/// Declare the entire bounded range, including absent tail pages. Writable
/// pages permit deletion so shrinking a record removes its obsolete contents.
pub(crate) fn declarations(
    prefix: &[u8],
    max_bytes: usize,
    writable: bool,
) -> Result<Vec<DeclaredAccess>> {
    let (header, pages) = keys(prefix, max_bytes)?;
    let mut access = Vec::with_capacity(1 + pages.len());
    access.push(DeclaredAccess {
        key: header,
        may_put: writable,
        may_delete: false,
    });
    access.extend(pages.into_iter().map(|key| DeclaredAccess {
        key,
        may_put: writable,
        may_delete: writable,
    }));
    Ok(access)
}

pub(crate) fn read(
    input: &SignatureCheckedInput,
    prefix: &[u8],
    max_bytes: usize,
) -> Result<Vec<u8>> {
    read_with(|key| input.read(key), prefix, max_bytes)
}

fn read_with(
    mut read_value: impl FnMut(&[u8]) -> Result<Option<Vec<u8>>>,
    prefix: &[u8],
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let (header_key, page_keys) = keys(prefix, max_bytes)?;
    let header = read_value(&header_key)?.context("record length header missing")?;
    let header: [u8; 4] = header
        .as_slice()
        .try_into()
        .context("record length header must contain exactly four bytes")?;
    let length = usize::try_from(u32::from_le_bytes(header))?;
    ensure!(
        length > 0 && length <= max_bytes,
        "record length outside bound"
    );
    let mut bytes = Vec::with_capacity(length);
    for (index, key) in page_keys.into_iter().enumerate() {
        let page = read_value(&key)?;
        let offset = index * PAGE_BYTES;
        if offset < length {
            let page = page.context("record data page missing")?;
            let expected = (length - offset).min(PAGE_BYTES);
            ensure!(page.len() == expected, "record data page is noncanonical");
            bytes.extend_from_slice(&page);
        } else {
            ensure!(
                page.is_none(),
                "record has an undeclared trailing data page"
            );
        }
    }
    // No prefix is returned if any page read or canonicality check failed.
    Ok(bytes)
}

/// Encode one nonempty record, including deletion of every unused bounded page.
/// At most 33 changes are emitted, below the tree's existing 4096-change bound.
/// The caller must use the same prefix/maximum profile for capture and reading.
pub fn record_changes(prefix: &[u8], bytes: &[u8], max_bytes: usize) -> Result<Vec<StateChange>> {
    let (header_key, page_keys) = keys(prefix, max_bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= max_bytes,
        "record content outside bound"
    );
    let mut changes = Vec::with_capacity(1 + page_keys.len());
    changes.push(StateChange::Put {
        key: header_key,
        value: u32::try_from(bytes.len())?.to_le_bytes().to_vec(),
    });
    for (index, key) in page_keys.into_iter().enumerate() {
        let offset = index * PAGE_BYTES;
        if offset < bytes.len() {
            let end = (offset + PAGE_BYTES).min(bytes.len());
            changes.push(StateChange::Put {
                key,
                value: bytes[offset..end].to_vec(),
            });
        } else {
            changes.push(StateChange::Delete { key });
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::frontier::{CaptureBudget, OwnedStateInput};
    use crate::state::tree::{empty_root, stage_state_update, NodeHash, StateNodeReader};
    use std::collections::BTreeMap;

    const PREFIX: &[u8] = b"nov/fee-state/v1";
    type Records = BTreeMap<Vec<u8>, Vec<u8>>;

    fn records(bytes: &[u8], max: usize) -> Records {
        let mut result = Records::new();
        apply(&mut result, record_changes(PREFIX, bytes, max).unwrap());
        result
    }

    fn apply(records: &mut Records, changes: Vec<StateChange>) {
        for change in changes {
            match change {
                StateChange::Put { key, value } => {
                    records.insert(key, value);
                }
                StateChange::Delete { key } => {
                    records.remove(&key);
                }
            }
        }
    }

    fn decode(records: &Records, max: usize) -> Result<Vec<u8>> {
        read_with(|key| Ok(records.get(key).cloned()), PREFIX, max)
    }

    #[test]
    fn bounded_records_roundtrip_at_page_boundaries() {
        for length in [1, 255, 256, 257, 511, 512, 2048, 8192] {
            let data: Vec<_> = (0..length).map(|index| (index % 251) as u8).collect();
            for max in [length, MAX_RECORD_BYTES] {
                let changes = record_changes(PREFIX, &data, max).unwrap();
                assert!(changes.len() <= 33);
                for change in &changes {
                    match change {
                        StateChange::Put { key, value } => {
                            assert!(key.len() <= 256);
                            assert!(!value.is_empty() && value.len() <= 256);
                        }
                        StateChange::Delete { key } => assert!(key.len() <= 256),
                    }
                }
                assert_eq!(decode(&records(&data, max), max).unwrap(), data);
            }
        }
    }

    #[test]
    fn declarations_cover_absent_tails_and_only_writable_pages_allow_delete() {
        for writable in [false, true] {
            let access = declarations(PREFIX, 8192, writable).unwrap();
            assert_eq!(access.len(), 33);
            assert_eq!(access[0].key, [PREFIX, b"/len"].concat());
            assert!(!access[0].may_delete);
            for (index, item) in access.iter().enumerate() {
                assert_eq!(item.may_put, writable);
                if index > 0 {
                    assert_eq!(item.may_delete, writable);
                    assert_eq!(
                        item.key,
                        [PREFIX, b"/page/", &(index as u16 - 1).to_be_bytes()].concat()
                    );
                }
            }
        }
    }

    #[test]
    fn missing_header_or_any_used_page_is_not_implicit_zero() {
        let original = records(&[3; 600], 1024);
        let (header, pages) = keys(PREFIX, 1024).unwrap();
        for key in std::iter::once(header).chain(pages.into_iter().take(3)) {
            let mut altered = original.clone();
            altered.remove(&key);
            assert!(decode(&altered, 1024).is_err());
        }
        assert!(decode(&Records::new(), 1024).is_err());
    }

    #[test]
    fn rejects_noncanonical_headers_pages_and_hidden_tail_even_when_empty() {
        let original = records(&[4; 257], 768);
        let (header, pages) = keys(PREFIX, 768).unwrap();
        for malformed in [
            vec![],
            vec![1; 3],
            vec![1; 5],
            0u32.to_le_bytes().to_vec(),
            769u32.to_le_bytes().to_vec(),
            u32::MAX.to_le_bytes().to_vec(),
        ] {
            let mut altered = original.clone();
            altered.insert(header.clone(), malformed);
            assert!(decode(&altered, 768).is_err());
        }
        for (index, bad_length) in [(0, 0), (0, 255), (0, 257), (1, 0), (1, 2), (1, 256)] {
            let mut altered = original.clone();
            altered.insert(pages[index].clone(), vec![4; bad_length]);
            assert!(decode(&altered, 768).is_err());
        }
        for hidden in [vec![], vec![0], vec![0; 256]] {
            let mut altered = original.clone();
            altered.insert(pages[2].clone(), hidden);
            assert!(decode(&altered, 768).is_err());
        }
    }

    #[test]
    fn shrink_explicitly_deletes_old_tail_and_exact_last_page_remains_canonical() {
        let mut stored = records(&[7; 768], 1024);
        let replacement = vec![8; 257];
        let changes = record_changes(PREFIX, &replacement, 1024).unwrap();
        assert!(matches!(&changes[3], StateChange::Delete { .. }));
        assert!(matches!(&changes[4], StateChange::Delete { .. }));
        apply(&mut stored, changes);
        assert_eq!(decode(&stored, 1024).unwrap(), replacement);
        assert_eq!(stored.len(), 3); // header + two pages, no hidden empty values.
    }

    #[test]
    fn bad_profile_prefix_and_content_fail_before_source_read() {
        for max in [0, MAX_RECORD_BYTES + 1, usize::MAX] {
            assert!(declarations(PREFIX, max, true).is_err());
            assert!(record_changes(PREFIX, &[1], max).is_err());
            assert!(read_with(|_| panic!("invalid budget must not read"), PREFIX, max).is_err());
        }
        for prefix in [vec![], vec![1; MAX_PREFIX_BYTES + 1]] {
            assert!(declarations(&prefix, 256, true).is_err());
            assert!(record_changes(&prefix, &[1], 256).is_err());
            assert!(read_with(|_| panic!("invalid prefix must not read"), &prefix, 256).is_err());
        }
        assert!(record_changes(PREFIX, &[], 256).is_err());
        assert!(record_changes(PREFIX, &[1; 257], 256).is_err());
        let access = declarations(&[1; MAX_PREFIX_BYTES], 256, true).unwrap();
        assert_eq!(access[1].key.len(), 256);
    }

    #[test]
    fn bounded_tail_reads_and_source_errors_never_return_successful_prefix() {
        let data = records(&[9], 8192);
        let (_, pages) = keys(PREFIX, 8192).unwrap();
        let final_key = pages.last().unwrap();
        let mut reads = 0;
        let result = read_with(
            |key| {
                reads += 1;
                if key == final_key.as_slice() {
                    anyhow::bail!("fixture tail read failure");
                }
                Ok(data.get(key).cloned())
            },
            PREFIX,
            8192,
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("tail read failure"));
        assert_eq!(reads, 33);
        reads = 0;
        assert_eq!(
            read_with(
                |key| {
                    reads += 1;
                    Ok(data.get(key).cloned())
                },
                PREFIX,
                8192
            )
            .unwrap(),
            [9]
        );
        assert_eq!(reads, 33);
    }

    #[derive(Default)]
    struct Memory(BTreeMap<NodeHash, Vec<u8>>);

    impl StateNodeReader for Memory {
        fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
            Ok(self.0.get(hash).cloned())
        }
    }

    #[test]
    fn paged_record_shrinks_through_real_owned_frontier_and_tree_without_live_reads() {
        let old = vec![1; 700];
        let initial = stage_state_update(
            &Memory::default(),
            empty_root(),
            &record_changes(PREFIX, &old, 1024).unwrap(),
        )
        .unwrap();
        let source = Memory(initial.nodes().clone());
        let access = declarations(PREFIX, 1024, true).unwrap();
        let budget = CaptureBudget {
            keys: 5,
            nodes: 64,
            bytes: 32768,
        };
        let owned = OwnedStateInput::capture(&source, initial.root(), &access, budget).unwrap();
        drop(source);
        assert_eq!(read_with(|key| owned.read(key), PREFIX, 1024).unwrap(), old);
        let new = vec![2; 257];
        let updated = owned
            .stage(&record_changes(PREFIX, &new, 1024).unwrap())
            .unwrap();
        let mut persisted = initial.nodes().clone();
        persisted.extend(updated.nodes().clone());
        let source = Memory(persisted);
        let final_input =
            OwnedStateInput::capture(&source, updated.root(), &access, budget).unwrap();
        drop(source);
        assert_eq!(
            read_with(|key| final_input.read(key), PREFIX, 1024).unwrap(),
            new
        );
        let (_, pages) = keys(PREFIX, 1024).unwrap();
        assert!(final_input.read(&pages[2]).unwrap().is_none());
        assert!(final_input.read(&pages[3]).unwrap().is_none());
    }
}
