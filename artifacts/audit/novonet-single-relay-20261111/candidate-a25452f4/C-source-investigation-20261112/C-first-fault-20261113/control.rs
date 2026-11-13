use std::ffi::c_void;
#[link(name="kernel32")]
unsafe extern "system" {
 fn GetProcessHeap()->*mut c_void;
 fn HeapAlloc(h:*mut c_void, flags:u32, bytes:usize)->*mut u8;
 fn HeapFree(h:*mut c_void, flags:u32, p:*mut c_void)->i32;
}
#[inline(never)]
unsafe fn deliberate_heap_overrun(p:*mut u8) { p.add(16).write_volatile(0x5a); }
fn main(){unsafe{
 let heap=GetProcessHeap();let p=HeapAlloc(heap,0,16);assert!(!p.is_null());
 for i in 0..16 {p.add(i).write_volatile(i as u8);}
 if std::env::args().nth(1).as_deref()==Some("overrun") {
  println!("CONTROL_BEFORE_OVERRUN");deliberate_heap_overrun(p);
  println!("CONTROL_OVERRUN_MISSED");
 }else{assert_ne!(HeapFree(heap,0,p.cast()),0);println!("CONTROL_CLEAN_OK");}
}}
