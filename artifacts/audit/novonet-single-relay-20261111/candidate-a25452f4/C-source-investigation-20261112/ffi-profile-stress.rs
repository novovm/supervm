#![deny(unsafe_op_in_unsafe_fn)]
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
    let gate = Arc::new(Barrier::new(10));
    let began = std::time::Instant::now();
    let checked: usize = std::thread::scope(|scope| {
        for worker in 0..4u8 {
            let gate = gate.clone();
            scope.spawn(move || {
                let path = std::env::var("C_DIAGNOSTIC_AOEM_DLL").unwrap();
                // Only the pinned packaged runtime is loaded; no node handle or database.
                let runtime = unsafe { aoem_bindings::AoemDyn::load(path) }.unwrap();
                gate.wait();
                let (pk, sk) = runtime.mldsa_keygen_v1(65).unwrap();
                let sk = zeroize::Zeroizing::new(sk);
                assert_eq!(pk.len(), 1952);
                for n in 0..256u64 {
                    let mut canary = [worker.wrapping_add(37); 4096];
                    std::hint::black_box(&mut canary);
                    let message = format!("C offline ABI isolation {worker}:{n}").into_bytes();
                    let expected_message = message.clone();
                    let sig = runtime.mldsa_sign_v1(65, &sk, &message).unwrap();
                    assert_eq!(sig.len(), 3309);
                    assert!(runtime.mldsa_verify_v1(65, &pk, &message, &sig).unwrap());
                    let mut changed = message.clone(); changed.push(1);
                    assert!(!runtime.mldsa_verify_v1(65, &pk, &changed, &sig).unwrap());
                    assert_eq!(message, expected_message);
                    assert!(std::hint::black_box(&canary).iter().all(|v| *v == worker.wrapping_add(37)));
                    check_snapshot(profile::snapshot());
                }
            });
        }
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
        "scope":"original profile plus pinned AOEM ML-DSA65 keygen/sign/verify, no node/network/database", "crypto_workers":4, "signature_pairs":1024, "tampered_message_rejections":1024, "additional_crypto_snapshots":1024,
        "root_cause_reproduced":false, "complete_node_qualified":false
    }));
}
