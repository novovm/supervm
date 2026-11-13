from pathlib import Path
import shutil
b=Path(__file__).parent;r=b/'unwind-isolation-01';r.mkdir(exist_ok=False)
src=Path('D:/WEB3_AI/SUPERVM/target/c-aoem-node-test-20261113/profile-bounded-37796')
shutil.copytree(src/'src',r/'src')
s=(src/'Cargo.toml').read_text().replace('c-profile-source-isolation','c-stackcapture-profile')
(r/'Cargo.toml').write_text(s)
main=(r/'src/main.rs').read_text().replace('#![forbid(unsafe_code)]','')
allocator='''
use std::alloc::{GlobalAlloc,Layout,System};
use std::sync::atomic::{AtomicBool,AtomicU64,Ordering};
static TRACE:AtomicBool=AtomicBool::new(false);
static CAPTURES:AtomicU64=AtomicU64::new(0);
static EMPTY:AtomicU64=AtomicU64::new(0);
#[link(name="kernel32")]
unsafe extern "system" {fn RtlCaptureStackBackTrace(skip:u32,count:u32,frames:*mut *mut std::ffi::c_void,hash:*mut u32)->u16;}
thread_local! {static INSIDE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
#[inline(never)] fn capture(){
 if !TRACE.load(Ordering::Relaxed){return}
 let _=INSIDE.try_with(|inside|{
  if inside.replace(true){return}
  let mut frames=[std::ptr::null_mut();64];
  let n=unsafe{RtlCaptureStackBackTrace(0,64,frames.as_mut_ptr(),std::ptr::null_mut())};
  CAPTURES.fetch_add(1,Ordering::Relaxed);if n==0{EMPTY.fetch_add(1,Ordering::Relaxed);}
  inside.set(false);
 });
}
struct TracedSystem;
unsafe impl GlobalAlloc for TracedSystem {
 unsafe fn alloc(&self,l:Layout)->*mut u8{capture();unsafe{System.alloc(l)}}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8{capture();unsafe{System.alloc_zeroed(l)}}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8{capture();unsafe{System.realloc(p,l,n)}}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){unsafe{System.dealloc(p,l)}}
}
#[global_allocator] static ALLOC:TracedSystem=TracedSystem;
'''
main=main.replace('mod profile;',allocator+'\nmod profile;')
main=main.replace('fn main(){','fn main(){\n    let tracing=std::env::args().nth(1).as_deref()==Some("trace");\n    TRACE.store(tracing,Ordering::Relaxed);')
main=main.replace('Duration::from_secs(180)','Duration::from_secs(if tracing{30}else{5})')
main=main.replace('    println!("{}",serde_json::json!', '    TRACE.store(false,Ordering::Relaxed);\n    println!("STACK_CAPTURE calls={} empty={} enabled={}",CAPTURES.load(Ordering::Relaxed),EMPTY.load(Ordering::Relaxed),tracing);\n    println!("{}",serde_json::json!')
(r/'src/main.rs').write_text(main)
print(r)
