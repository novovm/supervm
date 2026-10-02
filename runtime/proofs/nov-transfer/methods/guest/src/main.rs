#![no_main]

risc0_zkvm::guest::entry!(main);

fn main() {
    // Match AOEM's byte-oriented portable guest input, not serde word framing.
    let mut length = [0; 4];
    risc0_zkvm::guest::env::read_slice(&mut length);
    let length = u32::from_le_bytes(length) as usize;
    assert!(
        length <= novovm_host::proof::MAX_INPUT_BYTES,
        "proof input too large"
    );
    let mut input = vec![0; length];
    risc0_zkvm::guest::env::read_slice(&mut input);
    let journal = novovm_host::proof::execute_to_journal(&input)
        .expect("complete NOV transition relation rejected");
    risc0_zkvm::guest::env::commit_slice(&journal.encode());
}
