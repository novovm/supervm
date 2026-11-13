// Standalone unique marker exports for debugger breakpoints; no unsafe runtime operations.
#![deny(unsafe_op_in_unsafe_fn)]
use std::hint::black_box;
#[no_mangle]
#[inline(never)]
pub extern "C" fn phase3_begin(value:u8)->u8 { black_box(value) }
#[no_mangle]
#[inline(never)]
pub extern "C" fn phase3_end(length:usize)->usize { black_box(length) }
fn main() {
    for value in [0xb9u8,0x56,0x10,0x7f,0x80,0xff] {
        let v=phase3_begin(black_box(value));
        let result=format!("{v:02x}");
        let length=phase3_end(result.len());
        assert_eq!(length,2);
        let digits=b"0123456789abcdef";
        assert_eq!(result.as_bytes(),[digits[(v>>4) as usize],digits[(v&15) as usize]]);
    }
    println!("PHASE3_FIXTURE_PASS cases=6 original_node_reproduced=false");
}
