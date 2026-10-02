//! Bounded diagnostic image, not the production NOV execution relation.
//! It proves the result of the unchanged public V3 authentication entry point.

#![no_main]

use novovm_host::ingress::authentication::authenticate_transfer_v3;
use risc0_zkvm::{
    guest::env,
    sha::{Impl, Sha256},
};

risc0_zkvm::guest::entry!(main);

const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_CASES: usize = 32;
const MAX_RAW_BYTES: usize = 4096;
const INPUT_MAGIC: &[u8; 8] = b"NVAUTHI1";
const JOURNAL_MAGIC: &[u8; 8] = b"NVAUTHJ1";

fn take<'a>(input: &'a [u8], cursor: &mut usize, length: usize) -> &'a [u8] {
    let end = cursor.checked_add(length).expect("record length overflow");
    let bytes = input.get(*cursor..end).expect("truncated auth input");
    *cursor = end;
    bytes
}

fn main() {
    let mut length = [0; 4];
    env::read_slice(&mut length);
    let length = u32::from_le_bytes(length) as usize;
    // The external length word is part of the total diagnostic input budget.
    assert!(length <= MAX_INPUT_BYTES - 4, "auth input exceeds bound");
    let mut input = vec![0; length];
    env::read_slice(&mut input);

    let mut cursor = 0;
    assert_eq!(take(&input, &mut cursor, 8), INPUT_MAGIC);
    let chain_id = u64::from_le_bytes(take(&input, &mut cursor, 8).try_into().unwrap());
    assert_ne!(chain_id, 0, "zero authentication domain");
    let count = u16::from_le_bytes(take(&input, &mut cursor, 2).try_into().unwrap()) as usize;
    assert!((1..=MAX_CASES).contains(&count), "invalid case count");

    // Preflight the entire carrier before authenticating any case. Records
    // borrow the bounded input; malformed framing cannot yield a partial journal.
    let mut records = [(0, 0); MAX_CASES];
    for record in &mut records[..count] {
        let length = u32::from_le_bytes(take(&input, &mut cursor, 4).try_into().unwrap()) as usize;
        assert!((1..=MAX_RAW_BYTES).contains(&length), "invalid raw length");
        let start = cursor;
        take(&input, &mut cursor, length);
        *record = (start, cursor);
    }
    assert_eq!(cursor, input.len(), "trailing auth input");

    // Bind the exact ordered public inputs, not merely a replaceable vector of
    // true/false claims. There are no expected outcomes in the guest input.
    let digest = Impl::hash_bytes(&input);
    let mut journal = Vec::with_capacity(42 + count);
    journal.extend_from_slice(JOURNAL_MAGIC);
    journal.extend_from_slice(digest.as_bytes());
    journal.extend_from_slice(&(count as u16).to_le_bytes());
    for &(start, end) in &records[..count] {
        journal.push(u8::from(
            authenticate_transfer_v3(&input[start..end], chain_id, MAX_RAW_BYTES).is_ok(),
        ));
    }
    env::commit_slice(&journal);
}
