//! Collector mechanics only, not a NOVOVM fault reproduction.
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
fn main(){
 let root=std::path::PathBuf::from(std::env::args_os().nth(1).unwrap());std::fs::create_dir(&root).unwrap();
 lowtrace::prime(42);lowtrace::query(1);
 let before=ALLOCATIONS.load(Ordering::Relaxed);
 let gen=lowtrace::generation();lowtrace::record(2,2,0,123,gen,64,64,&[5;32],1,0);
 {let _scope=lowtrace::target();for k in [101,102,103,104,105,106]{lowtrace::boundary(k,123,456,0,64,789,2,321);}}
 for i in 0..200_000 {lowtrace::record(10,10,i,0,0,0,0,&[],0,0);}
 lowtrace::record(3,3,0,123,gen,64,64,&[5;32],0,0);
 let allocations=ALLOCATIONS.load(Ordering::Relaxed)-before;assert_eq!(allocations,0);
 lowtrace::finish(&root.join("before-overflow.bin")).unwrap();
 // Same-channel overflow must be counted, without corrupting another channel.
 for i in 0..8200{lowtrace::record(104,104,i,123,1,0,0,&[],0,0);}
 lowtrace::finish(&root.join("after-overflow.bin")).unwrap();
 let other=root.join("unregistered.bin");std::thread::spawn(move||{
  for k in [2,104,1,10]{lowtrace::record(k,k,0,0,0,0,0,&[],0,0);}
  lowtrace::prime(43);lowtrace::finish(&other).unwrap();
 }).join().unwrap();
 println!("allocations_in_registered_capture={allocations}");
}
