//! Independent diagnostic export only. No production linkage.
use std::{cell::{Cell, RefCell}, io::Write, sync::atomic::{AtomicU64, Ordering}, time::Instant};
const CAPS: [usize; 4] = [4096, 8192, 8192, 1024];
const WORDS: usize = 20;
const EMPTY: [u64; WORDS] = [0; WORDS];
static GENERATION: AtomicU64 = AtomicU64::new(1);
static UNREGISTERED: [AtomicU64; 4] = [const {AtomicU64::new(0)}; 4];
struct Channel { events: Box<[[u64; WORDS]]>, writes: u64, overwritten: u64, sampled: u64 }
struct Recorder { channels: [Channel; 4], sequence: u64, origin: Instant, tid: u64 }
#[derive(Clone, Copy, Default)]
struct Scope { call: u64, selected: bool, receiver: usize }
thread_local! {
 static REC: RefCell<Option<Recorder>> = const {RefCell::new(None)};
 static LOST: Cell<[u64;4]> = const {Cell::new([0;4])};
 static SCOPE: Cell<Scope> = const {Cell::new(Scope{call:0,selected:false,receiver:0})};
 static CALL: Cell<u64> = const {Cell::new(0)};
 static QUERY: Cell<u64> = const {Cell::new(0)};
 static MISMATCH: Cell<u64> = const {Cell::new(0)};
}
pub fn enabled() -> bool {cfg!(feature="lowtrace")}
pub fn prime(tid:u64) {
 if !enabled(){return;}
 REC.with(|cell| {assert!(cell.borrow().is_none());
  let channels=CAPS.map(|capacity|Channel{events:vec![EMPTY;capacity].into_boxed_slice(),writes:0,overwritten:0,sampled:0});
  *cell.borrow_mut()=Some(Recorder{channels,sequence:0,origin:Instant::now(),tid});
 });
}
pub fn generation()->u64 {if enabled(){GENERATION.fetch_add(1,Ordering::Relaxed)}else{0}}
fn channel(kind:u64)->usize {match kind {2..=4=>0,100..=199=>1,10|11=>3,_=>2}}
fn lost(c:usize) {let _=LOST.try_with(|x|{let mut v=x.get();v[c]=v[c].saturating_add(1);x.set(v);});}
/// Each channel owns its capacity. Dropped ordinary events never enter the B ring.
#[inline]
pub fn record(kind:u64,site:u64,call:u64,object:usize,generation:u64,len:usize,capacity:usize,id:&[u8],a:u64,b:u64) {
 record_full(kind,site,call,object,generation,len,capacity,id,a,b,0,0);
}
#[inline]
fn record_full(kind:u64,site:u64,call:u64,object:usize,generation:u64,len:usize,capacity:usize,id:&[u8],a:u64,b:u64,text_len:u64,formatter:u64) {
 if !enabled(){return;}
 let c=channel(kind);
 if REC.try_with(|cell| {
  let Ok(mut slot)=cell.try_borrow_mut() else {lost(c);return;};
  let Some(r)=slot.as_mut() else {UNREGISTERED[c].fetch_add(1,Ordering::Relaxed);return;};
  let ring=&mut r.channels[c];
  // Ordinary formatting is counted but intentionally not retained. No timestamp/copy.
  if c==3 {ring.sampled=ring.sampled.saturating_add(1);return;}
  r.sequence=r.sequence.saturating_add(1);ring.writes=ring.writes.saturating_add(1);
  let mut e=[0u64;WORDS];
  e[..10].copy_from_slice(&[r.sequence,r.origin.elapsed().as_nanos().min(u64::MAX as u128) as u64,kind,site,r.tid,call,object as u64,generation,len as u64,capacity as u64]);
  for (i,chunk) in id.chunks_exact(8).take(4).enumerate(){e[10+i]=u64::from_le_bytes(chunk.try_into().unwrap());}
  e[14]=a;e[15]=b;e[16]=QUERY.with(Cell::get);e[17]=ring.writes;e[18]=text_len;e[19]=formatter;
  let index=((ring.writes-1)%ring.events.len() as u64) as usize;
  if ring.writes>ring.events.len() as u64{ring.overwritten=ring.overwritten.saturating_add(1);}
  ring.events[index]=e;
 }).is_err(){UNREGISTERED[c].fetch_add(1,Ordering::Relaxed);}
}
pub fn query(id:u64){if enabled(){QUERY.with(|x|x.set(id));}}
pub struct TargetScope(Scope);
pub fn target()->TargetScope {
 if !enabled(){return TargetScope(Scope::default());}
 if QUERY.with(Cell::get)==0{return TargetScope(SCOPE.with(|s|s.replace(Scope::default())));}
 let call=CALL.with(|c|{let n=c.get()+1;c.set(n);n});
 // Sample entire calls, never individual stages. Every unselected group is counted.
 let selected=(call-1)%256==0;
 let previous=SCOPE.with(|s|s.replace(Scope{call,selected,receiver:0}));
 if !selected {REC.with(|r|if let Some(r)=r.borrow_mut().as_mut(){r.channels[1].sampled+=1;});}
 TargetScope(previous)
}
impl Drop for TargetScope {fn drop(&mut self){if enabled(){SCOPE.with(|s|s.set(self.0));}}}
/// Called only by the copied standard library. Arguments are source-level values,
/// NOT a claim about unmodified machine-entry registers or a pre-tailcall RSP.
pub fn boundary(kind:u64,object:usize,buffer:usize,len:usize,capacity:usize,text:usize,text_len:usize,formatter:usize) {
 if !enabled(){return;}
 let _=SCOPE.try_with(|state| {
  let mut s=state.get();if s.call==0{return;}
  if kind==101{s.receiver=object;state.set(s);}
  let mismatch=matches!(kind,102|103|104|105|106) && s.receiver!=0 && object!=s.receiver;
  if mismatch{MISMATCH.with(|m|m.set(m.get()+1));}
  if s.selected||mismatch {
   record_full(kind,kind,s.call,object,s.call,len,capacity,&[],buffer as u64,text as u64,text_len as u64,formatter as u64);
  }
 });
}
/// Normal-stop export only; unexpected-exit evidence requires full dump analysis.
/// The latest slot per channel is conservatively invalid in an exception dump.
pub fn finish(path:&std::path::Path)->std::io::Result<()> {
 if !enabled(){return Ok(());}
 REC.with(|cell|{
  let slot=cell.borrow();let Some(r)=slot.as_ref() else{return Ok(());};
  let mut out=std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
  out.write_all(b"NCPART02")?;
  for n in [r.tid,r.sequence,WORDS as u64,4,CALL.with(Cell::get),MISMATCH.with(Cell::get)]{out.write_all(&n.to_le_bytes())?;}
  for (c,ring) in r.channels.iter().enumerate(){
   let retained=ring.writes.min(ring.events.len() as u64);
   for n in [c as u64,ring.events.len() as u64,ring.writes,retained,ring.overwritten,ring.sampled,LOST.with(|x|x.get()[c]),UNREGISTERED[c].load(Ordering::Relaxed)]{out.write_all(&n.to_le_bytes())?;}
   for seq in ring.writes-retained+1..=ring.writes{for word in ring.events[((seq-1)%ring.events.len() as u64) as usize]{out.write_all(&word.to_le_bytes())?;}}
  }
  out.flush()
 })
}
