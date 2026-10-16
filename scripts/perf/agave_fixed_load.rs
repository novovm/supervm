// Reference load instrumentation only. No validator or product changes.
use std::{collections::HashSet, fs::OpenOptions, io::Write, sync::Mutex,
          time::{Instant, SystemTime, UNIX_EPOCH}};

pub struct FixedLoad {
    pub count: usize,
    state: Mutex<(std::fs::File, HashSet<String>)>,
    start: Instant,
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
                    count, state: Mutex::new((file, HashSet::new())), start: Instant::now(),
                }))
            }
            _ => panic!("COUNT and MANIFEST must be supplied together"),
        }
    }

    // Captured immediately before existing queue submission, NOT network delivery.
    pub fn record(&self, batch: &[Vec<u8>]) {
        let mut guard = self.state.lock().expect("manifest lock");
        for bytes in batch {
            let tx: solana_transaction::versioned::VersionedTransaction =
                wincode::deserialize(bytes).expect("decode generated transaction");
            let signature = tx.signatures.first().expect("signed transaction").to_string();
            assert!(guard.1.insert(signature.clone()), "duplicate generated signature");
            assert!(guard.1.len() <= self.count, "fixed count exceeded");
            let row = serde_json::json!({"signature": signature,
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
        guard.0.sync_all().expect("manifest sync");
    }
}
