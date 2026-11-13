
#![allow(dead_code)]

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

mod profile;
use std::time::{Duration, Instant};
fn hash_input(n:u64)->[u8;32]{
    let mut bytes=[0u8;32];
    for i in 0..4 {bytes[i*8..i*8+8].copy_from_slice(&n.to_le_bytes());}
    bytes
}
fn check(v:serde_json::Value)->usize {
    let mut hashes=0;
    for name in ["native_delivery","critical_path","control_work"] {
        if let Some(rows)=v[name]["events"].as_array(){
            assert!(rows.len()<=2048);
            for row in rows {for key in ["value","body","fragment","peer"] {
                if let Some(s)=row[key].as_str(){
                    assert_eq!(s.len(),64);
                    assert!(s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)));
                    assert!(s.as_bytes().chunks_exact(16).all(|c|c==&s.as_bytes()[..16]));
                    hashes+=1;
                }
            }}
        }
    }
    let raw=serde_json::to_vec(&v).unwrap();
    let decoded:serde_json::Value=serde_json::from_slice(&raw).unwrap();
    assert_eq!(v,decoded);
    hashes
}
fn main(){
    let tracing=std::env::args().nth(1).as_deref()==Some("trace");
    TRACE.store(tracing,Ordering::Relaxed);
    assert!(profile::enabled());profile::enable_native_delivery_observation();
    let start=Instant::now();
    for n in 0..4096u64 {
        let h=hash_input(n.wrapping_mul(0x9e3779b97f4a7c15));
        profile::critical_event(profile::CriticalEvent::ReceiptProjectionVisible,n,1,Some(h),Some(h));
        profile::delivery_event("bounded_isolation_fixture",n,h,h,1);
        profile::work_interval(profile::Phase::Projection,Some(start),Some(start+Duration::from_millis(2)),2,Some(h),Some(h));
    }
    let deadline=Instant::now()+Duration::from_secs(if tracing{30}else{5});
    let outputs=std::thread::scope(|scope|{
        let mut handles=Vec::new();
        for worker in 0..2 {handles.push(scope.spawn(move||{
            let mut snapshots=0u64;let mut checked=0usize;
            while Instant::now()<deadline && snapshots<10000 {
                checked+=check(profile::snapshot());snapshots+=1;
                if snapshots%500==0 {println!("worker={worker} snapshots={snapshots} checked_hashes={checked}");}
            }
            (snapshots,checked)
        }));}
        handles.into_iter().map(|h|h.join().unwrap()).collect::<Vec<_>>()
    });
    TRACE.store(false,Ordering::Relaxed);
    println!("STACK_CAPTURE calls={} empty={} enabled={}",CAPTURES.load(Ordering::Relaxed),EMPTY.load(Ordering::Relaxed),tracing);
    println!("{}",serde_json::json!({"status":"PASS","worker_results":outputs,"elapsed_ms":start.elapsed().as_millis(),
      "scope":"original bounded profile source with populated buffers and JSON roundtrip; no AOEM, node, network, database or original identities",
      "root_cause_reproduced":false}));
}
