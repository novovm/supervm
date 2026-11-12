#![forbid(unsafe_code)]
#![allow(dead_code)]
mod profile;
use std::sync::{Arc, Barrier};

fn check_snapshot(value: serde_json::Value) -> usize {
    let encoded = serde_json::to_vec(&value).unwrap();
    let decoded: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(value, decoded);
    let mut checked = 0;
    for (section, keys) in [
        ("native_delivery", ["peer", "fragment"]),
        ("critical_path", ["value", "body"]),
    ] {
        if let Some(events) = value[section]["events"].as_array() {
            assert!(events.len() <= 2048);
            for event in events {
                for key in keys {
                    let text = event[key].as_str().expect("test events carry hashes");
                    assert_eq!(text.len(), 64);
                    assert!(text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
                    // Each submitted test hash repeats one byte. Detect torn or
                    // malformed output, not merely a successful JSON serialization.
                    assert!(text.as_bytes().chunks_exact(2).all(|p| p == &text.as_bytes()[..2]));
                    checked += 1;
                }
            }
        }
    }
    checked
}

fn main() {
    assert!(profile::enabled());
    profile::enable_native_delivery_observation();
    for n in 0..256u64 {
        profile::critical_event(profile::CriticalEvent::ReceiptProjectionVisible,
            n, 1, Some([n as u8; 32]), Some([255 - n as u8; 32]));
        profile::delivery_event("isolation_fixture", n, [n as u8; 32], [255 - n as u8; 32], 1);
    }
    let gate = Arc::new(Barrier::new(6));
    let began = std::time::Instant::now();
    let checked: usize = std::thread::scope(|scope| {
        for writer in 0..4u8 {
            let gate = gate.clone();
            scope.spawn(move || {
                gate.wait();
                for n in 0..20000u64 {
                    profile::delivery_event("isolation_fixture", n,
                        [writer; 32], [n as u8; 32], 2);
                    profile::elapsed(profile::Phase::Projection, profile::mark());
                    if n % 64 == 0 { std::thread::yield_now(); }
                }
            });
        }
        let readers: Vec<_> = (0..2).map(|_| {
            let gate = gate.clone();
            scope.spawn(move || {
                gate.wait();
                (0..250).map(|_| check_snapshot(profile::snapshot())).sum::<usize>()
            })
        }).collect();
        readers.into_iter().map(|r| r.join().unwrap()).sum()
    });
    let checked = checked + check_snapshot(profile::snapshot());
    assert!(checked >= 256 * 2 * 501);
    println!("{}", serde_json::json!({
        "result":"PASS", "snapshot_count":501, "writer_attempts":80000,
        "hash_fields_validated":checked, "elapsed_ms":began.elapsed().as_millis(),
        "scope":"isolated original profile source, no FFI/node/network/database",
        "root_cause_reproduced":false, "complete_node_qualified":false
    }));
}
