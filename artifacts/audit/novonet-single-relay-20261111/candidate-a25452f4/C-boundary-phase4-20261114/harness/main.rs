#![deny(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]
mod profile;
#[no_mangle]
#[inline(never)]
pub extern "C" fn phase4_window_begin(sequence: usize) -> usize {
    std::hint::black_box(sequence)
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn phase4_window_end(sequence: usize) -> usize {
    std::hint::black_box(sequence)
}
fn main() {
    // One original snapshot, using test-only values. No node, DB, FFI or network.
    profile::enable_native_delivery_observation();
    let peer = std::array::from_fn(|n| n as u8);
    let fragment = std::array::from_fn(|n| (255 - n) as u8);
    profile::delivery_event("phase4_boundary_test", 0, peer, fragment, 1);
    phase4_window_begin(1);
    let snapshot = std::hint::black_box(profile::snapshot());
    phase4_window_end(1);
    let events = snapshot["native_delivery"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    let digits=b"0123456789abcdef";
    for (field, input) in [("peer",peer),("fragment",fragment)] {
        let bytes=events[0][field].as_str().unwrap().as_bytes();
        assert_eq!(bytes.len(),64);
        for (n,value) in input.into_iter().enumerate() {
            assert_eq!(bytes[2*n],digits[(value>>4) as usize]);
            assert_eq!(bytes[2*n+1],digits[(value&15) as usize]);
        }
    }
    println!("PHASE4_SNAPSHOT_PASS events=1 hash_bytes=64 original_failure_reproduced=false");
}
