//! Diagnostic export only. No unsafe code, locks, allocation, formatting or IO in record().
use std::{cell::{Cell,RefCell},time::Instant,sync::atomic::{AtomicU64,Ordering},io::Write};
const CAPACITY:usize=65536;
const EMPTY:[u64;16]=[0;16];
static UNREGISTERED:AtomicU64=AtomicU64::new(0);
static GENERATION:AtomicU64=AtomicU64::new(1);
struct Ring { events:Box<[[u64;16]]>, seq:u64, overwritten:u64, origin:Instant, tid:u64 }
thread_local! {
 static RING:RefCell<Option<Ring>>=const {RefCell::new(None)};
 static LOST:Cell<u64>=const {Cell::new(0)};
}
pub fn enabled()->bool {cfg!(feature="lowtrace")}
/// Must run before the owner workload. Unregistered threads drop rather than allocate.
pub fn prime(tid:u64) {
 if !enabled(){return;}
 RING.with(|r| {assert!(r.borrow().is_none());
  let events=vec![EMPTY;CAPACITY].into_boxed_slice();
  println!("LOWTRACE_BUFFER tid={tid} address={:p} capacity={CAPACITY} stride=128",events.as_ptr());
  *r.borrow_mut()=Some(Ring{events,seq:0,overwritten:0,origin:Instant::now(),tid});
 });
}
pub fn generation()->u64 {if enabled(){GENERATION.fetch_add(1,Ordering::Relaxed)}else{0}}
/// seq, monotonic ns, kind, site, OS tid, call, object, generation, len, capacity,
/// digest[4], argument0, argument1. Payload bytes are NOT recorded.
#[inline]
pub fn record(kind:u64,site:u64,call:u64,object:usize,generation:u64,len:usize,capacity:usize,id:&[u8],a:u64,b:u64) {
 if !enabled(){return;}
 let _=RING.try_with(|r| {
  let Ok(mut guard)=r.try_borrow_mut() else {LOST.with(|n|n.set(n.get().saturating_add(1)));return;};
  let Some(r)=guard.as_mut() else {UNREGISTERED.fetch_add(1,Ordering::Relaxed);return;};
  r.seq=r.seq.saturating_add(1);
  let mut e=[r.seq,r.origin.elapsed().as_nanos().min(u64::MAX as u128) as u64,kind,site,r.tid,call,object as u64,generation,len as u64,capacity as u64,0,0,0,0,a,b];
  for (i,c) in id.chunks_exact(8).take(4).enumerate(){e[10+i]=u64::from_le_bytes(c.try_into().expect("8 bytes"));}
  let slot=(r.seq.saturating_sub(1)%CAPACITY as u64) as usize;
  if r.seq>CAPACITY as u64{r.overwritten=r.overwritten.saturating_add(1);}
  r.events[slot]=e;
 });
}
/// Outside workload only. Full-memory exception dumps retain the in-memory ring.
pub fn finish(path:&std::path::Path)->std::io::Result<()> {
 if !enabled(){return Ok(());}
 RING.with(|slot| {
  let r=slot.borrow();let Some(r)=r.as_ref() else{return Ok(());};
  let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
  f.write_all(b"NCLTLS01")?;
  let retained=r.seq.min(CAPACITY as u64);
  for v in [r.tid,r.seq,retained,r.overwritten,LOST.with(Cell::get),UNREGISTERED.load(Ordering::Relaxed),CAPACITY as u64,128]{f.write_all(&v.to_le_bytes())?;}
  for seq in r.seq-retained+1..=r.seq {
   let e=&r.events[((seq-1)%CAPACITY as u64) as usize];
   for v in e{f.write_all(&v.to_le_bytes())?;}
  }
  f.flush()
 })
}
