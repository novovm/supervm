//! Standalone collector integrity test, never linked into a node.
mod lowtrace;
use std::alloc::{GlobalAlloc,Layout,System};
use std::sync::atomic::{AtomicU64,Ordering};
struct Counted;
static ALLOCATIONS:AtomicU64=AtomicU64::new(0);
unsafe impl GlobalAlloc for Counted {
 unsafe fn alloc(&self,l:Layout)->*mut u8{ALLOCATIONS.fetch_add(1,Ordering::Relaxed);System.alloc(l)}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8{ALLOCATIONS.fetch_add(1,Ordering::Relaxed);System.alloc_zeroed(l)}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8{ALLOCATIONS.fetch_add(1,Ordering::Relaxed);System.realloc(p,l,n)}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){System.dealloc(p,l)}
}
#[global_allocator] static ALLOC:Counted=Counted;
fn word(b:&[u8],at:usize)->u64{u64::from_le_bytes(b[at..at+8].try_into().unwrap())}
fn main(){
 let root=std::path::PathBuf::from(std::env::args_os().nth(1).expect("fresh test output"));
 std::fs::create_dir(&root).unwrap();lowtrace::prime(42);
 let before=ALLOCATIONS.load(Ordering::Relaxed);
 for i in 0..65543 {lowtrace::record(2,2,i,123,lowtrace::generation(),64,64,&[5;32],0,0);}
 let allocations=ALLOCATIONS.load(Ordering::Relaxed)-before;assert_eq!(allocations,0);
 lowtrace::finish(&root.join("ring.bin")).unwrap();
 let bytes=std::fs::read(root.join("ring.bin")).unwrap();
 assert_eq!(&bytes[..8],b"NCLTLS01");assert_eq!(word(&bytes,16),65543);assert_eq!(word(&bytes,24),65536);assert_eq!(word(&bytes,32),7);
 assert_eq!(word(&bytes,40),0);assert_eq!(word(&bytes,48),0);assert_eq!(bytes.len(),72+65536*128);
 for i in 0..65536{assert_eq!(word(&bytes,72+i*128),8+i as u64);assert_eq!(word(&bytes,72+i*128+32),42);}
 let second=root.join("second.bin");
 std::thread::spawn(move||{lowtrace::record(1,1,0,0,0,0,0,&[],0,0);lowtrace::prime(43);lowtrace::record(4,4,1,0,0,0,0,&[],0,0);lowtrace::finish(&second).unwrap();}).join().unwrap();
 let b=std::fs::read(root.join("second.bin")).unwrap();assert_eq!(word(&b,8),43);assert_eq!(word(&b,16),1);assert_eq!(word(&b,48),1);
 println!("PASS allocations_in_65543_events={allocations} retained=65536 overwritten=7 threads=2 unregistered_dropped=1");
}
