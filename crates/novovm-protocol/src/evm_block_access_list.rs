use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::collections::{BTreeMap, BTreeSet, HashSet};

const EVM_BLOCK_ACCESS_LIST_MAX_CODE_BYTES_V1: usize = 24_576;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct EvmBlockAccessListV1(pub Vec<EvmBlockAccessAccountV1>);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EvmConstructionBlockAccessListV1 {
    accounts: BTreeMap<[u8; 20], EvmConstructionAccountAccessV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct EvmConstructionAccountAccessV1 {
    storage_writes: BTreeMap<[u8; 32], BTreeMap<u32, [u8; 32]>>,
    storage_reads: BTreeSet<[u8; 32]>,
    balance_changes: BTreeMap<u32, [u8; 32]>,
    nonce_changes: BTreeMap<u32, u64>,
    code_changes: BTreeMap<u32, Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EvmBlockAccessAccountV1 {
    pub address: [u8; 20],
    pub storage_changes: Vec<EvmBlockAccessSlotChangesV1>,
    pub storage_reads: Vec<[u8; 32]>,
    pub balance_changes: Vec<EvmBlockAccessBalanceChangeV1>,
    pub nonce_changes: Vec<EvmBlockAccessNonceChangeV1>,
    pub code_changes: Vec<EvmBlockAccessCodeChangeV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmBlockAccessSlotChangesV1 {
    pub slot: [u8; 32],
    pub slot_changes: Vec<EvmBlockAccessStorageWriteV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmBlockAccessStorageWriteV1 {
    pub block_access_index: u32,
    pub post_value: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmBlockAccessBalanceChangeV1 {
    pub block_access_index: u32,
    pub post_balance: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmBlockAccessNonceChangeV1 {
    pub block_access_index: u32,
    pub post_nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmBlockAccessCodeChangeV1 {
    pub block_access_index: u32,
    pub new_code: Vec<u8>,
}

impl EvmConstructionBlockAccessListV1 {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    pub fn account_read(&mut self, address: [u8; 20]) {
        let _ = self.accounts.entry(address).or_default();
    }

    pub fn storage_read(&mut self, address: [u8; 20], slot: [u8; 32]) {
        let account = self.accounts.entry(address).or_default();
        if account.storage_writes.contains_key(&slot) {
            return;
        }
        let _ = account.storage_reads.insert(slot);
    }

    pub fn storage_write(
        &mut self,
        block_access_index: u32,
        address: [u8; 20],
        slot: [u8; 32],
        post_value: [u8; 32],
    ) {
        let account = self.accounts.entry(address).or_default();
        account
            .storage_writes
            .entry(slot)
            .or_default()
            .insert(block_access_index, post_value);
        let _ = account.storage_reads.remove(&slot);
    }

    pub fn balance_change(
        &mut self,
        block_access_index: u32,
        address: [u8; 20],
        post_balance: [u8; 32],
    ) {
        let account = self.accounts.entry(address).or_default();
        account
            .balance_changes
            .insert(block_access_index, post_balance);
    }

    pub fn nonce_change(&mut self, block_access_index: u32, address: [u8; 20], post_nonce: u64) {
        let account = self.accounts.entry(address).or_default();
        account.nonce_changes.insert(block_access_index, post_nonce);
    }

    pub fn code_change(&mut self, block_access_index: u32, address: [u8; 20], new_code: Vec<u8>) {
        let account = self.accounts.entry(address).or_default();
        account.code_changes.insert(block_access_index, new_code);
    }

    pub fn merge(&mut self, other: &Self) {
        for (address, other_account) in &other.accounts {
            let account = self.accounts.entry(*address).or_default();
            for (slot, writes) in &other_account.storage_writes {
                let existing = account.storage_writes.entry(*slot).or_default();
                for (block_access_index, post_value) in writes {
                    existing.insert(*block_access_index, *post_value);
                }
                let _ = account.storage_reads.remove(slot);
            }
            for slot in &other_account.storage_reads {
                if !account.storage_writes.contains_key(slot) {
                    let _ = account.storage_reads.insert(*slot);
                }
            }
            for (block_access_index, post_balance) in &other_account.balance_changes {
                account
                    .balance_changes
                    .insert(*block_access_index, *post_balance);
            }
            for (block_access_index, post_nonce) in &other_account.nonce_changes {
                account
                    .nonce_changes
                    .insert(*block_access_index, *post_nonce);
            }
            for (block_access_index, new_code) in &other_account.code_changes {
                account
                    .code_changes
                    .insert(*block_access_index, new_code.clone());
            }
        }
    }

    #[must_use]
    pub fn to_access_list(&self) -> EvmBlockAccessListV1 {
        EvmBlockAccessListV1(
            self.accounts
                .iter()
                .map(|(address, account)| EvmBlockAccessAccountV1 {
                    address: *address,
                    storage_changes: account
                        .storage_writes
                        .iter()
                        .map(|(slot, writes)| EvmBlockAccessSlotChangesV1 {
                            slot: *slot,
                            slot_changes: writes
                                .iter()
                                .map(|(block_access_index, post_value)| {
                                    EvmBlockAccessStorageWriteV1 {
                                        block_access_index: *block_access_index,
                                        post_value: *post_value,
                                    }
                                })
                                .collect(),
                        })
                        .collect(),
                    storage_reads: account.storage_reads.iter().copied().collect(),
                    balance_changes: account
                        .balance_changes
                        .iter()
                        .map(
                            |(block_access_index, post_balance)| EvmBlockAccessBalanceChangeV1 {
                                block_access_index: *block_access_index,
                                post_balance: *post_balance,
                            },
                        )
                        .collect(),
                    nonce_changes: account
                        .nonce_changes
                        .iter()
                        .map(
                            |(block_access_index, post_nonce)| EvmBlockAccessNonceChangeV1 {
                                block_access_index: *block_access_index,
                                post_nonce: *post_nonce,
                            },
                        )
                        .collect(),
                    code_changes: account
                        .code_changes
                        .iter()
                        .map(
                            |(block_access_index, new_code)| EvmBlockAccessCodeChangeV1 {
                                block_access_index: *block_access_index,
                                new_code: new_code.clone(),
                            },
                        )
                        .collect(),
                })
                .collect(),
        )
    }
}

#[must_use]
pub fn merge_evm_block_access_lists_v1(
    base: Option<EvmBlockAccessListV1>,
    next: Option<EvmBlockAccessListV1>,
) -> Option<EvmBlockAccessListV1> {
    match (base, next) {
        (None, None) => None,
        (Some(base), None) => Some(base),
        (None, Some(next)) => Some(next),
        (Some(base), Some(next)) => {
            let mut builder = construction_from_access_list_v1(&base);
            builder.merge(&construction_from_access_list_v1(&next));
            Some(builder.to_access_list())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvmBlockAccessListErrorV1 {
    #[error("block access list contains duplicate accounts")]
    DuplicateAccounts,
    #[error("block access list account {account_index} contains duplicate storage-write slots")]
    DuplicateStorageChangeSlots { account_index: usize },
    #[error(
        "block access list account {account_index} slot {slot_index} contains duplicate block-access indexes"
    )]
    DuplicateStorageWriteIndexes {
        account_index: usize,
        slot_index: usize,
    },
    #[error(
        "block access list account {account_index} slot {slot_index} contains no storage writes"
    )]
    EmptyStorageWriteSet {
        account_index: usize,
        slot_index: usize,
    },
    #[error("block access list account {account_index} contains duplicate storage-read slots")]
    DuplicateStorageReads { account_index: usize },
    #[error(
        "block access list account {account_index} reports slot in both storageReads and storageChanges"
    )]
    StorageReadWriteIntersection { account_index: usize },
    #[error("block access list account {account_index} contains duplicate balance-change indexes")]
    DuplicateBalanceChangeIndexes { account_index: usize },
    #[error("block access list account {account_index} contains duplicate nonce-change indexes")]
    DuplicateNonceChangeIndexes { account_index: usize },
    #[error("block access list account {account_index} contains duplicate code-change indexes")]
    DuplicateCodeChangeIndexes { account_index: usize },
    #[error(
        "block access list account {account_index} code change {change_index} exceeds Amsterdam max code size"
    )]
    OversizedCodeChange {
        account_index: usize,
        change_index: usize,
    },
}

pub fn evm_block_access_list_item_count_v1(list: &EvmBlockAccessListV1) -> u64 {
    list.0
        .iter()
        .map(|account| {
            1u64 + account.storage_changes.len() as u64 + account.storage_reads.len() as u64
        })
        .sum()
}

#[must_use]
pub fn construction_from_access_list_v1(
    list: &EvmBlockAccessListV1,
) -> EvmConstructionBlockAccessListV1 {
    let mut out = EvmConstructionBlockAccessListV1::new();
    for account in &list.0 {
        out.account_read(account.address);
        for slot in &account.storage_reads {
            out.storage_read(account.address, *slot);
        }
        for slot in &account.storage_changes {
            for change in &slot.slot_changes {
                out.storage_write(
                    change.block_access_index,
                    account.address,
                    slot.slot,
                    change.post_value,
                );
            }
        }
        for change in &account.balance_changes {
            out.balance_change(
                change.block_access_index,
                account.address,
                change.post_balance,
            );
        }
        for change in &account.nonce_changes {
            out.nonce_change(
                change.block_access_index,
                account.address,
                change.post_nonce,
            );
        }
        for change in &account.code_changes {
            out.code_change(
                change.block_access_index,
                account.address,
                change.new_code.clone(),
            );
        }
    }
    out
}

pub fn evm_block_access_list_rlp_bytes_v1(
    list: &EvmBlockAccessListV1,
) -> Result<Vec<u8>, EvmBlockAccessListErrorV1> {
    let canonical = canonicalize_evm_block_access_list_v1(list)?;
    Ok(rlp_encode_list(
        &canonical
            .0
            .iter()
            .map(rlp_encode_account_access_v1)
            .collect::<Vec<_>>(),
    ))
}

pub fn evm_block_access_list_hash_v1(
    list: &EvmBlockAccessListV1,
) -> Result<[u8; 32], EvmBlockAccessListErrorV1> {
    let rlp = evm_block_access_list_rlp_bytes_v1(list)?;
    Ok(Keccak256::digest(rlp).into())
}

pub fn canonicalize_evm_block_access_list_v1(
    list: &EvmBlockAccessListV1,
) -> Result<EvmBlockAccessListV1, EvmBlockAccessListErrorV1> {
    let mut accounts = list.0.clone();
    accounts.sort_by(|lhs, rhs| lhs.address.cmp(&rhs.address));
    for pair in accounts.windows(2) {
        if pair[0].address == pair[1].address {
            return Err(EvmBlockAccessListErrorV1::DuplicateAccounts);
        }
    }
    for (account_index, account) in accounts.iter_mut().enumerate() {
        canonicalize_account_access_v1(account_index, account)?;
    }
    Ok(EvmBlockAccessListV1(accounts))
}

fn canonicalize_account_access_v1(
    account_index: usize,
    account: &mut EvmBlockAccessAccountV1,
) -> Result<(), EvmBlockAccessListErrorV1> {
    account
        .storage_changes
        .sort_by(|lhs, rhs| lhs.slot.cmp(&rhs.slot));
    for pair in account.storage_changes.windows(2) {
        if pair[0].slot == pair[1].slot {
            return Err(EvmBlockAccessListErrorV1::DuplicateStorageChangeSlots { account_index });
        }
    }
    for (slot_index, slot_changes) in account.storage_changes.iter_mut().enumerate() {
        if slot_changes.slot_changes.is_empty() {
            return Err(EvmBlockAccessListErrorV1::EmptyStorageWriteSet {
                account_index,
                slot_index,
            });
        }
        slot_changes
            .slot_changes
            .sort_by_key(|change| change.block_access_index);
        if has_duplicate_u32_v1(
            slot_changes
                .slot_changes
                .iter()
                .map(|change| change.block_access_index),
        ) {
            return Err(EvmBlockAccessListErrorV1::DuplicateStorageWriteIndexes {
                account_index,
                slot_index,
            });
        }
    }

    account.storage_reads.sort();
    if has_duplicate_array32_v1(account.storage_reads.iter().copied()) {
        return Err(EvmBlockAccessListErrorV1::DuplicateStorageReads { account_index });
    }
    let write_slots: HashSet<[u8; 32]> = account
        .storage_changes
        .iter()
        .map(|slot| slot.slot)
        .collect();
    if account
        .storage_reads
        .iter()
        .any(|slot| write_slots.contains(slot))
    {
        return Err(EvmBlockAccessListErrorV1::StorageReadWriteIntersection { account_index });
    }

    account
        .balance_changes
        .sort_by_key(|change| change.block_access_index);
    if has_duplicate_u32_v1(
        account
            .balance_changes
            .iter()
            .map(|change| change.block_access_index),
    ) {
        return Err(EvmBlockAccessListErrorV1::DuplicateBalanceChangeIndexes { account_index });
    }

    account
        .nonce_changes
        .sort_by_key(|change| change.block_access_index);
    if has_duplicate_u32_v1(
        account
            .nonce_changes
            .iter()
            .map(|change| change.block_access_index),
    ) {
        return Err(EvmBlockAccessListErrorV1::DuplicateNonceChangeIndexes { account_index });
    }

    account
        .code_changes
        .sort_by_key(|change| change.block_access_index);
    if has_duplicate_u32_v1(
        account
            .code_changes
            .iter()
            .map(|change| change.block_access_index),
    ) {
        return Err(EvmBlockAccessListErrorV1::DuplicateCodeChangeIndexes { account_index });
    }
    for (change_index, change) in account.code_changes.iter().enumerate() {
        if change.new_code.len() > EVM_BLOCK_ACCESS_LIST_MAX_CODE_BYTES_V1 {
            return Err(EvmBlockAccessListErrorV1::OversizedCodeChange {
                account_index,
                change_index,
            });
        }
    }
    Ok(())
}

fn has_duplicate_u32_v1<I>(iter: I) -> bool
where
    I: IntoIterator<Item = u32>,
{
    let mut seen = HashSet::<u32>::new();
    for value in iter {
        if !seen.insert(value) {
            return true;
        }
    }
    false
}

fn has_duplicate_array32_v1<I>(iter: I) -> bool
where
    I: IntoIterator<Item = [u8; 32]>,
{
    let mut seen = HashSet::<[u8; 32]>::new();
    for value in iter {
        if !seen.insert(value) {
            return true;
        }
    }
    false
}

fn rlp_encode_account_access_v1(account: &EvmBlockAccessAccountV1) -> Vec<u8> {
    rlp_encode_list(&[
        rlp_encode_bytes(&account.address),
        rlp_encode_list(
            &account
                .storage_changes
                .iter()
                .map(rlp_encode_storage_changes_v1)
                .collect::<Vec<_>>(),
        ),
        rlp_encode_list(
            &account
                .storage_reads
                .iter()
                .map(|slot| rlp_encode_uint256_bytes(trim_leading_zeros_v1(slot)))
                .collect::<Vec<_>>(),
        ),
        rlp_encode_list(
            &account
                .balance_changes
                .iter()
                .map(rlp_encode_balance_change_v1)
                .collect::<Vec<_>>(),
        ),
        rlp_encode_list(
            &account
                .nonce_changes
                .iter()
                .map(rlp_encode_nonce_change_v1)
                .collect::<Vec<_>>(),
        ),
        rlp_encode_list(
            &account
                .code_changes
                .iter()
                .map(rlp_encode_code_change_v1)
                .collect::<Vec<_>>(),
        ),
    ])
}

fn rlp_encode_storage_changes_v1(slot_changes: &EvmBlockAccessSlotChangesV1) -> Vec<u8> {
    rlp_encode_list(&[
        rlp_encode_uint256_bytes(trim_leading_zeros_v1(&slot_changes.slot)),
        rlp_encode_list(
            &slot_changes
                .slot_changes
                .iter()
                .map(|change| {
                    rlp_encode_list(&[
                        rlp_encode_u64(u64::from(change.block_access_index)),
                        rlp_encode_uint256_bytes(trim_leading_zeros_v1(&change.post_value)),
                    ])
                })
                .collect::<Vec<_>>(),
        ),
    ])
}

fn rlp_encode_balance_change_v1(change: &EvmBlockAccessBalanceChangeV1) -> Vec<u8> {
    rlp_encode_list(&[
        rlp_encode_u64(u64::from(change.block_access_index)),
        rlp_encode_uint256_bytes(trim_leading_zeros_v1(&change.post_balance)),
    ])
}

fn rlp_encode_nonce_change_v1(change: &EvmBlockAccessNonceChangeV1) -> Vec<u8> {
    rlp_encode_list(&[
        rlp_encode_u64(u64::from(change.block_access_index)),
        rlp_encode_u64(change.post_nonce),
    ])
}

fn rlp_encode_code_change_v1(change: &EvmBlockAccessCodeChangeV1) -> Vec<u8> {
    rlp_encode_list(&[
        rlp_encode_u64(u64::from(change.block_access_index)),
        rlp_encode_bytes(change.new_code.as_slice()),
    ])
}

fn trim_leading_zeros_v1(bytes: &[u8]) -> &[u8] {
    let first_nonzero = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    &bytes[first_nonzero..]
}

fn rlp_encode_u64(value: u64) -> Vec<u8> {
    if value == 0 {
        return rlp_encode_bytes(&[]);
    }
    let bytes = value.to_be_bytes();
    rlp_encode_bytes(trim_leading_zeros_v1(&bytes))
}

fn rlp_encode_uint256_bytes(bytes: &[u8]) -> Vec<u8> {
    rlp_encode_bytes(bytes)
}

fn rlp_encode_bytes(bytes: &[u8]) -> Vec<u8> {
    match bytes.len() {
        1 if bytes[0] < 0x80 => vec![bytes[0]],
        len if len <= 55 => {
            let mut out = Vec::with_capacity(1 + len);
            out.push(0x80 + len as u8);
            out.extend_from_slice(bytes);
            out
        }
        len => {
            let len_bytes = usize_to_be_bytes_v1(len);
            let mut out = Vec::with_capacity(1 + len_bytes.len() + len);
            out.push(0xb7 + len_bytes.len() as u8);
            out.extend_from_slice(&len_bytes);
            out.extend_from_slice(bytes);
            out
        }
    }
}

fn rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut payload = Vec::with_capacity(payload_len);
    for item in items {
        payload.extend_from_slice(item);
    }
    if payload.len() <= 55 {
        let mut out = Vec::with_capacity(1 + payload.len());
        out.push(0xc0 + payload.len() as u8);
        out.extend_from_slice(&payload);
        return out;
    }
    let len_bytes = usize_to_be_bytes_v1(payload.len());
    let mut out = Vec::with_capacity(1 + len_bytes.len() + payload.len());
    out.push(0xf7 + len_bytes.len() as u8);
    out.extend_from_slice(&len_bytes);
    out.extend_from_slice(&payload);
    out
}

fn usize_to_be_bytes_v1(value: usize) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    trim_leading_zeros_v1(&bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_hex32(hex: &str) -> [u8; 32] {
        let trimmed = hex.strip_prefix("0x").unwrap_or(hex);
        assert_eq!(trimmed.len(), 64);
        let mut out = [0u8; 32];
        for idx in 0..32 {
            out[idx] = u8::from_str_radix(&trimmed[idx * 2..idx * 2 + 2], 16).expect("hex byte");
        }
        out
    }

    #[test]
    fn block_access_list_hash_matches_empty_rlp_list() {
        let hash = evm_block_access_list_hash_v1(&EvmBlockAccessListV1::default()).expect("hash");
        assert_eq!(
            hash,
            parse_hex32("0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347")
        );
    }

    #[test]
    fn block_access_list_rlp_encodes_single_empty_account_like_geth() {
        let rlp = evm_block_access_list_rlp_bytes_v1(&EvmBlockAccessListV1(vec![
            EvmBlockAccessAccountV1 {
                address: [0x11; 20],
                storage_changes: Vec::new(),
                storage_reads: Vec::new(),
                balance_changes: Vec::new(),
                nonce_changes: Vec::new(),
                code_changes: Vec::new(),
            },
        ]))
        .expect("rlp");
        let mut expected = vec![0xdb, 0xda, 0x94];
        expected.extend_from_slice(&[0x11; 20]);
        expected.extend_from_slice(&[0xc0, 0xc0, 0xc0, 0xc0, 0xc0]);
        assert_eq!(rlp, expected);
    }

    #[test]
    fn block_access_list_hash_is_stable_across_unsorted_input() {
        let list = EvmBlockAccessListV1(vec![
            EvmBlockAccessAccountV1 {
                address: [0x22; 20],
                storage_changes: vec![EvmBlockAccessSlotChangesV1 {
                    slot: [0x03; 32],
                    slot_changes: vec![
                        EvmBlockAccessStorageWriteV1 {
                            block_access_index: 9,
                            post_value: [0x09; 32],
                        },
                        EvmBlockAccessStorageWriteV1 {
                            block_access_index: 1,
                            post_value: [0x01; 32],
                        },
                    ],
                }],
                storage_reads: vec![[0x07; 32], [0x06; 32]],
                balance_changes: vec![
                    EvmBlockAccessBalanceChangeV1 {
                        block_access_index: 3,
                        post_balance: [0x0a; 32],
                    },
                    EvmBlockAccessBalanceChangeV1 {
                        block_access_index: 2,
                        post_balance: [0x0b; 32],
                    },
                ],
                nonce_changes: vec![EvmBlockAccessNonceChangeV1 {
                    block_access_index: 8,
                    post_nonce: 7,
                }],
                code_changes: Vec::new(),
            },
            EvmBlockAccessAccountV1 {
                address: [0x11; 20],
                storage_changes: Vec::new(),
                storage_reads: Vec::new(),
                balance_changes: Vec::new(),
                nonce_changes: Vec::new(),
                code_changes: vec![EvmBlockAccessCodeChangeV1 {
                    block_access_index: 0,
                    new_code: vec![0xde, 0xad, 0xbe, 0xef],
                }],
            },
        ]);
        let canonical = canonicalize_evm_block_access_list_v1(&list).expect("canonicalize");
        let lhs = evm_block_access_list_hash_v1(&list).expect("hash unsorted");
        let rhs = evm_block_access_list_hash_v1(&canonical).expect("hash canonical");
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn block_access_list_validation_rejects_read_write_slot_intersection() {
        let err = canonicalize_evm_block_access_list_v1(&EvmBlockAccessListV1(vec![
            EvmBlockAccessAccountV1 {
                address: [0x11; 20],
                storage_changes: vec![EvmBlockAccessSlotChangesV1 {
                    slot: [0x44; 32],
                    slot_changes: vec![EvmBlockAccessStorageWriteV1 {
                        block_access_index: 1,
                        post_value: [0x55; 32],
                    }],
                }],
                storage_reads: vec![[0x44; 32]],
                balance_changes: Vec::new(),
                nonce_changes: Vec::new(),
                code_changes: Vec::new(),
            },
        ]))
        .expect_err("intersection should fail");
        assert!(matches!(
            err,
            EvmBlockAccessListErrorV1::StorageReadWriteIntersection { account_index: 0 }
        ));
    }

    #[test]
    fn block_access_list_validation_rejects_empty_slot_changes() {
        let err = canonicalize_evm_block_access_list_v1(&EvmBlockAccessListV1(vec![
            EvmBlockAccessAccountV1 {
                address: [0x11; 20],
                storage_changes: vec![EvmBlockAccessSlotChangesV1 {
                    slot: [0x22; 32],
                    slot_changes: Vec::new(),
                }],
                storage_reads: Vec::new(),
                balance_changes: Vec::new(),
                nonce_changes: Vec::new(),
                code_changes: Vec::new(),
            },
        ]))
        .expect_err("empty slot changes must be rejected");
        assert!(matches!(
            err,
            EvmBlockAccessListErrorV1::EmptyStorageWriteSet {
                account_index: 0,
                slot_index: 0
            }
        ));
    }

    #[test]
    fn construction_block_access_list_merge_demotes_read_to_write() {
        let address = [0x11; 20];
        let slot_read = [0x22; 32];
        let slot_write = [0x33; 32];
        let mut base = EvmConstructionBlockAccessListV1::new();
        base.account_read(address);
        base.storage_read(address, slot_read);
        base.storage_write(1, address, slot_write, [0x44; 32]);

        let mut other = EvmConstructionBlockAccessListV1::new();
        other.storage_write(2, address, slot_read, [0x55; 32]);
        other.nonce_change(2, address, 9);

        base.merge(&other);
        let merged = base.to_access_list();
        assert_eq!(merged.0.len(), 1);
        assert!(merged.0[0].storage_reads.is_empty());
        assert_eq!(merged.0[0].storage_changes.len(), 2);
        assert_eq!(merged.0[0].nonce_changes[0].post_nonce, 9);
    }

    #[test]
    fn merge_block_access_lists_roundtrips_through_construction_builder() {
        let lhs = EvmBlockAccessListV1(vec![EvmBlockAccessAccountV1 {
            address: [0x11; 20],
            storage_changes: vec![EvmBlockAccessSlotChangesV1 {
                slot: [0x22; 32],
                slot_changes: vec![EvmBlockAccessStorageWriteV1 {
                    block_access_index: 1,
                    post_value: [0x33; 32],
                }],
            }],
            storage_reads: Vec::new(),
            balance_changes: Vec::new(),
            nonce_changes: Vec::new(),
            code_changes: Vec::new(),
        }]);
        let rhs = EvmBlockAccessListV1(vec![EvmBlockAccessAccountV1 {
            address: [0x11; 20],
            storage_changes: Vec::new(),
            storage_reads: vec![[0x44; 32]],
            balance_changes: vec![EvmBlockAccessBalanceChangeV1 {
                block_access_index: 2,
                post_balance: [0x55; 32],
            }],
            nonce_changes: Vec::new(),
            code_changes: Vec::new(),
        }]);
        let merged = merge_evm_block_access_lists_v1(Some(lhs), Some(rhs)).expect("merged");
        assert_eq!(merged.0.len(), 1);
        assert_eq!(merged.0[0].storage_changes.len(), 1);
        assert_eq!(merged.0[0].storage_reads.len(), 1);
        assert_eq!(merged.0[0].balance_changes.len(), 1);
    }
}
