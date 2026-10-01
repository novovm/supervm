#![no_main]
risc0_zkvm::guest::entry!(main);
fn main() {
    let mut length = [0u8; 4];
    risc0_zkvm::guest::env::read_slice(&mut length);
    let length = u32::from_le_bytes(length) as usize;
    assert!(
        length <= novovm_auth_statement::MAX_INPUT_BYTES,
        "input too large"
    );
    let mut bytes = vec![0u8; length];
    risc0_zkvm::guest::env::read_slice(&mut bytes);
    let journal =
        novovm_auth_statement::decode_and_check(&bytes).expect("authentication relation rejected");
    risc0_zkvm::guest::env::commit_slice(&journal);
}
