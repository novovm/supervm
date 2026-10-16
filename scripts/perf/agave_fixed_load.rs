// Reference load instrumentation only. No validator or product changes.
use std::{collections::HashSet, fs::OpenOptions, io::{BufWriter, Write}, sync::Mutex,
          time::{Instant, SystemTime, UNIX_EPOCH}};

// Same Linux CLOCK_MONOTONIC epoch as Python time.monotonic_ns(). Wall time is
// diagnostic only: WSL clock adjustment must not become transaction latency.
fn monotonic_ns() -> u128 {
    #[repr(C)]
    struct Timespec { seconds: std::ffi::c_long, nanos: std::ffi::c_long }
    unsafe extern "C" {
        fn clock_gettime(clock: std::ffi::c_int, output: *mut Timespec) -> std::ffi::c_int;
    }
    let mut value = Timespec { seconds: 0, nanos: 0 };
    assert_eq!(unsafe { clock_gettime(1, &mut value) }, 0, "CLOCK_MONOTONIC unavailable");
    assert!(value.seconds >= 0 && (0..1_000_000_000).contains(&value.nanos));
    value.seconds as u128 * 1_000_000_000 + value.nanos as u128
}

#[cfg(test)]
mod reference_manifest_tests {
    use super::*;
    use solana_signer::Signer;

    #[test]
    fn buffered_manifest_preserves_signed_workload_and_completion() {
        let path = std::env::temp_dir().join(format!("agave-manifest-{}-{}.jsonl",
            std::process::id(), monotonic_ns()));
        let file = OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        let count = 65_536;
        let load = FixedLoad {
            count, state: Mutex::new((BufWriter::with_capacity(64 * 1024, file), HashSet::new())),
            start: Instant::now(), retry_bytes: Mutex::new((Vec::new(), 0)),
        };
        let payer = solana_keypair::Keypair::new();
        let recipient = solana_keypair::Keypair::new();
        let signed: Vec<_> = (1..=count).map(|amount| {
            crate::generator::transaction_builder::create_serialized_signed_transaction(
                &payer, solana_hash::Hash::default(),
                vec![solana_system_interface::instruction::transfer(
                    &payer.pubkey(), &recipient.pubkey(), amount as u64)], vec![], 600)
        }).collect();
        let start = Instant::now();
        // Match the earlier per-transaction recording call, without network or chain.
        for row in &signed { load.record(std::slice::from_ref(row)); }
        assert!(std::fs::metadata(&path).unwrap().len() > 0, "streaming visibility");
        load.finish();
        eprintln!("manifest_only count={count} seconds={:.6}; NOT chain TPS", start.elapsed().as_secs_f64());
        let data = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<serde_json::Value> = data.lines().map(|s| serde_json::from_str(s).unwrap()).collect();
        assert_eq!(rows.len(), count + 1);
        for (row, bytes) in rows.iter().zip(&signed) {
            let tx: solana_transaction::versioned::VersionedTransaction = wincode::deserialize(bytes).unwrap();
            assert_eq!(row["signature"], tx.signatures[0].to_string());
            assert_eq!(row["boundary"], "before_client_queue");
        }
        assert_eq!(rows[count]["generated_complete"], count);
        drop(load);
        std::fs::remove_file(path).unwrap();
    }
}

pub struct FixedLoad {
    pub count: usize,
    state: Mutex<(BufWriter<std::fs::File>, HashSet<String>)>,
    start: Instant,
    retry_bytes: Mutex<(Vec<Vec<u8>>, usize)>,
}

impl FixedLoad {
    pub fn from_env() -> Option<std::sync::Arc<Self>> {
        let count = std::env::var("AGAVE_REFERENCE_COUNT").ok();
        let path = std::env::var("AGAVE_REFERENCE_MANIFEST").ok();
        match (count, path) {
            (None, None) => None,
            (Some(count), Some(path)) => {
                let count: usize = count.parse().expect("positive fixed count");
                assert!((1..=1_000_000).contains(&count));
                let file = OpenOptions::new().write(true).create_new(true).open(path)
                    .expect("new manifest path required; never overwrite evidence");
                Some(std::sync::Arc::new(Self {
                    count, state: Mutex::new((BufWriter::with_capacity(64 * 1024, file), HashSet::new())), start: Instant::now(),
                    retry_bytes: Mutex::new((Vec::new(), 0)),
                }))
            }
            _ => panic!("COUNT and MANIFEST must be supplied together"),
        }
    }

    // Captured immediately before existing queue submission, NOT network delivery.
    pub fn record(&self, batch: &[Vec<u8>]) {
        if std::env::var_os("AGAVE_REFERENCE_RETRY").is_some() {
            let mut cache = self.retry_bytes.lock().expect("retry cache");
            let bytes = batch.iter().try_fold(cache.1, |sum, row|
                sum.checked_add(row.len())).expect("retry byte count overflow");
            assert!(bytes <= 64 * 1024 * 1024, "reference retry cache exceeds 64 MiB");
            cache.0.extend_from_slice(batch);
            cache.1 = bytes;
        }
        let mut guard = self.state.lock().expect("manifest lock");
        for bytes in batch {
            let tx: solana_transaction::versioned::VersionedTransaction =
                wincode::deserialize(bytes).expect("decode generated transaction");
            let signature = tx.signatures.first().expect("signed transaction").to_string();
            assert!(guard.1.insert(signature.clone()), "duplicate generated signature");
            assert!(guard.1.len() <= self.count, "fixed count exceeded");
            let row = serde_json::json!({"signature": signature,
                "queue_attempt_monotonic_ns": monotonic_ns(),
                "queue_attempt_elapsed_ns": self.start.elapsed().as_nanos(),
                "queue_attempt_unix_ns": SystemTime::now().duration_since(UNIX_EPOCH)
                    .expect("clock").as_nanos(), "boundary": "before_client_queue"});
            writeln!(guard.0, "{row}").expect("manifest write");
        }
    }

    pub fn finish(&self) {
        let mut guard = self.state.lock().expect("manifest lock");
        assert_eq!(guard.1.len(), self.count, "incomplete generated workload");
        writeln!(guard.0, "{}", serde_json::json!({"generated_complete": self.count}))
            .expect("completion marker");
        guard.0.flush().expect("manifest flush");
        guard.0.get_ref().sync_all().expect("manifest sync");
    }

    pub async fn settle(&self, senders: &[tokio::sync::mpsc::Sender<
        solana_tpu_client_next::transaction_batch::TransactionBatch>]) {
        if std::env::var_os("AGAVE_REFERENCE_RETRY").is_none() { return; }
        let bytes = self.retry_bytes.lock().expect("retry cache").0.clone();
        let signatures: Vec<_> = bytes.iter().map(|bytes| {
            let tx: solana_transaction::versioned::VersionedTransaction =
                wincode::deserialize(bytes).expect("retry decode");
            tx.signatures[0]
        }).collect();
        let clients: Vec<_> = [18899, 18999, 19099, 19199].into_iter().map(|port|
            solana_rpc_client::nonblocking::rpc_client::RpcClient::new_with_timeout(
                format!("http://127.0.0.1:{port}"), std::time::Duration::from_secs(2))).collect();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut retries = vec![0u8; bytes.len()];
        // Original sender handles stay alive while this bounded operation runs.
        let operation = async {
            loop {
                let mut all_final = true;
                let mut missing = vec![false; bytes.len()];
                for client in &clients {
                    for (chunk_index, chunk) in signatures.chunks(256).enumerate() {
                        match client.get_signature_statuses(chunk).await {
                            Ok(response) => {
                                assert_eq!(response.value.len(), chunk.len());
                                for (index, status) in response.value.iter().enumerate() {
                                    match status {
                                        None => { missing[chunk_index * 256 + index] = true; all_final = false; }
                                        Some(status) => all_final &= status.satisfies_commitment(
                                            solana_commitment_config::CommitmentConfig::finalized()),
                                    }
                                }
                            }
                            Err(error) => { all_final = false; log::warn!("reference retry observation: {error}"); }
                        }
                    }
                }
                if all_final { break; }
                for (index, absent) in missing.into_iter().enumerate() {
                    if absent && retries[index] < 8 {
                        senders[index % senders.len()].send(
                            solana_tpu_client_next::transaction_batch::TransactionBatch::new(
                                vec![bytes[index].clone()])).await.expect("retry queue closed");
                        retries[index] += 1;
                        log::info!("reference_retry signature={} attempt={}", signatures[index], retries[index]);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        };
        assert!(tokio::time::timeout_at(deadline, operation).await.is_ok(),
                "reference completion deadline; keep original manifest as incomplete evidence");
        log::info!("reference_retry completed unique={} retransmissions={}",
            signatures.len(), retries.iter().map(|v| usize::from(*v)).sum::<usize>());
    }
}
