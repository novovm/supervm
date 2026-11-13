// Isolated diagnostic control. Never accesses node state or the network.
use std::ffi::c_void;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetErrorMode(mode: u32) -> u32;
    fn SetUnhandledExceptionFilter(filter: unsafe extern "system" fn(*mut c_void) -> i32) -> *mut c_void;
    fn ExitProcess(code: u32) -> !;
}
static mut POINTER_SLOT: *mut u64 = std::ptr::null_mut();
unsafe extern "system" fn exception_exit(_: *mut c_void) -> i32 { ExitProcess(0xc0000005) }
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn first_bad_writer() {
    std::ptr::addr_of_mut!(POINTER_SLOT).write_volatile(0xb8usize as *mut u64);
}
#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn later_consumer() {
    let p = std::ptr::addr_of!(POINTER_SLOT).read_volatile();
    p.write_volatile(0x1122334455667788);
}
fn main() {
    unsafe {
        SetErrorMode(0x8003);
        SetUnhandledExceptionFilter(exception_exit);
        let good = Box::into_raw(Box::new(0u64));
        std::ptr::addr_of_mut!(POINTER_SLOT).write_volatile(good);
        println!("CONTROL_SLOT={:#x} WRITER={:#x} CONSUMER={:#x}",
            std::ptr::addr_of!(POINTER_SLOT) as usize, first_bad_writer as *const () as usize,
            later_consumer as *const () as usize);
        if std::env::args().nth(1).as_deref() == Some("bad") { first_bad_writer(); }
        later_consumer();
        assert_eq!(good.read_volatile(), 0x1122334455667788);
        drop(Box::from_raw(good));
        println!("CONTROL_CLEAN_OK");
    }
}
