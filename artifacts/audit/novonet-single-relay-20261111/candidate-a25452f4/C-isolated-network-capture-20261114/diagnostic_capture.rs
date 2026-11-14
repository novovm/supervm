//! ISOLATED DIAGNOSTIC COPY ONLY. Binary trace, never a production feature.
//! No payload, pointer or key data from this private file may be published.
use std::{collections::BTreeMap, fs::{File,OpenOptions}, io::Write,
    sync::{Mutex,OnceLock,atomic::{AtomicU64,Ordering}}, time::Instant};
static TRACE: OnceLock<Mutex<Trace>> = OnceLock::new();
static THREADS: AtomicU64 = AtomicU64::new(1);
thread_local! { static THREAD: u64 = THREADS.fetch_add(1,Ordering::Relaxed); }
struct Trace { file:File, origin:Instant, seq:u64, bytes:u64, generation:u64, live:BTreeMap<usize,u64> }
fn trace()-> &'static Mutex<Trace> {
    TRACE.get_or_init(|| {
        let root=std::path::PathBuf::from(std::env::var_os("JOINT_ROOT").expect("isolated root")).canonicalize().expect("root exists");
        assert!(root.join("ISOLATED_TEST_ONLY").is_file());
        let index:usize=std::env::var("JOINT_INDEX").expect("index").parse().expect("integer");
        assert!(index<4);
        let mut file=OpenOptions::new().write(true).create_new(true).open(root.join(format!("trace-{index}-private.bin"))).expect("new trace only");
        file.write_all(b"NCAP0001").expect("trace header");
        Mutex::new(Trace{file,origin:Instant::now(),seq:0,bytes:8,generation:0,live:BTreeMap::new()})
    })
}
// Types:1 packet receive;2 Arc object first observed;3 last Arc object Drop;
// 4 receive completed chunks;5 query begin;6 query end;7 orderly owner stop.
// first observed is NOT allocator entry; generation is process-local, not a network identity.
pub fn record(kind:u64,address:usize,count:u64,payload:&[u8]) {
    let mut t=trace().lock().expect("trace lock");
    let generation=if kind==2 {
        assert!(!t.live.contains_key(&address),"duplicate live address");
        t.generation+=1;let g=t.generation;t.live.insert(address,g);g
    } else if kind==3 { t.live.remove(&address).unwrap_or(0) } else {0};
    t.seq+=1;
    let values=[t.seq,kind,THREAD.with(|x|*x),t.origin.elapsed().as_nanos().min(u64::MAX as u128) as u64,address as u64,generation,count,payload.len() as u64];
    let needed=64+payload.len() as u64;
    if t.bytes+needed>128*1024*1024 { std::process::exit(84); }
    let mut header=[0u8;64];
    for (part,value) in header.chunks_exact_mut(8).zip(values) {part.copy_from_slice(&value.to_le_bytes());}
    if t.file.write_all(&header).and_then(|_|t.file.write_all(payload)).is_err(){std::process::exit(85);}
    t.bytes+=needed;
}
#[inline(never)]
pub extern "C" fn joint_hash_begin()->usize {std::hint::black_box(17)}
#[inline(never)]
pub extern "C" fn joint_hash_end()->usize {std::hint::black_box(19)}
