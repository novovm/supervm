#![forbid(unsafe_code)]
#![allow(dead_code)]
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
    assert!(profile::enabled());profile::enable_native_delivery_observation();
    let start=Instant::now();
    for n in 0..4096u64 {
        let h=hash_input(n.wrapping_mul(0x9e3779b97f4a7c15));
        profile::critical_event(profile::CriticalEvent::ReceiptProjectionVisible,n,1,Some(h),Some(h));
        profile::delivery_event("bounded_isolation_fixture",n,h,h,1);
        profile::work_interval(profile::Phase::Projection,Some(start),Some(start+Duration::from_millis(2)),2,Some(h),Some(h));
    }
    let deadline=Instant::now()+Duration::from_secs(180);
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
    println!("{}",serde_json::json!({"status":"PASS","worker_results":outputs,"elapsed_ms":start.elapsed().as_millis(),
      "scope":"original bounded profile source with populated buffers and JSON roundtrip; no AOEM, node, network, database or original identities",
      "root_cause_reproduced":false}));
}
