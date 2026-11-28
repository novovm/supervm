#![no_main]

#[path = "../uca_authority_guest/mod.rs"]
mod native_pipeline;

use native_pipeline::uca_authority::{verify_guest_input, MAX_GUEST_INPUT_BYTES};

risc0_zkvm::guest::entry!(main);

fn main() {
    let mut size = [0; 4];
    risc0_zkvm::guest::env::read_slice(&mut size);
    let length = u32::from_le_bytes(size) as usize;
    assert!(
        length <= MAX_GUEST_INPUT_BYTES,
        "UCA guest input byte limit"
    );
    let mut input = vec![0; length];
    risc0_zkvm::guest::env::read_slice(&mut input);
    let journal = verify_guest_input(&input).expect("UCA state relation rejected");
    risc0_zkvm::guest::env::commit_slice(&journal);
}
