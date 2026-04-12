use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedRequest {
    pub request_id: String,
    pub idempotent_key: String,
    pub created_unix_ms: u64,
    pub payload: Vec<u8>,
}

pub trait QueueStore: Send {
    fn enqueue(&mut self, req: QueuedRequest) -> Result<(), String>;
    fn list_pending(&self) -> Vec<QueuedRequest>;
    fn remove(&mut self, request_id: &str) -> Result<(), String>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryQueueStore {
    inner: Vec<QueuedRequest>,
}

impl InMemoryQueueStore {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }
}

impl QueueStore for InMemoryQueueStore {
    fn enqueue(&mut self, req: QueuedRequest) -> Result<(), String> {
        self.inner.push(req);
        Ok(())
    }

    fn list_pending(&self) -> Vec<QueuedRequest> {
        self.inner.clone()
    }

    fn remove(&mut self, request_id: &str) -> Result<(), String> {
        self.inner.retain(|r| r.request_id != request_id);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FileQueueStore {
    dir: PathBuf,
}

impl FileQueueStore {
    pub fn new(dir: impl AsRef<Path>) -> Result<Self, String> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)
            .map_err(|e| format!("create queue dir failed: {} ({e})", dir.display()))?;
        Ok(Self { dir })
    }

    fn file_path_for_request_id(&self, request_id: &str) -> PathBuf {
        let mut encoded = String::with_capacity(request_id.len() * 2 + 5);
        for b in request_id.as_bytes() {
            encoded.push_str(&format!("{b:02x}"));
        }
        self.dir.join(format!("{encoded}.json"))
    }
}

impl QueueStore for FileQueueStore {
    fn enqueue(&mut self, req: QueuedRequest) -> Result<(), String> {
        let path = self.file_path_for_request_id(&req.request_id);
        let body = serde_json::to_vec(&req)
            .map_err(|e| format!("queue json encode failed: request_id={} ({e})", req.request_id))?;
        fs::write(&path, body)
            .map_err(|e| format!("queue write failed: {} ({e})", path.display()))?;
        Ok(())
    }

    fn list_pending(&self) -> Vec<QueuedRequest> {
        let mut files: Vec<PathBuf> = match fs::read_dir(&self.dir) {
            Ok(read_dir) => read_dir
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| {
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
                })
                .collect(),
            Err(_) => return Vec::new(),
        };
        files.sort();

        files
            .into_iter()
            .filter_map(|path| fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()))
            .collect()
    }

    fn remove(&mut self, request_id: &str) -> Result<(), String> {
        let path = self.file_path_for_request_id(request_id);
        match fs::remove_file(&path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("queue remove failed: {} ({e})", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_roundtrip() {
        let mut store = InMemoryQueueStore::new();
        store
            .enqueue(QueuedRequest {
                request_id: "req-1".to_string(),
                idempotent_key: "idem-1".to_string(),
                created_unix_ms: 1_000,
                payload: vec![1, 2, 3],
            })
            .expect("enqueue should succeed");

        let pending = store.list_pending();
        assert_eq!(pending.len(), 1);

        store
            .remove("req-1")
            .expect("remove should succeed for in-memory queue");
        assert!(store.list_pending().is_empty());
    }

    #[test]
    fn file_store_roundtrip() {
        let mut queue_dir = std::env::temp_dir();
        queue_dir.push(format!(
            "novovm_queue_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&queue_dir).expect("create temp queue dir");
        let mut store = FileQueueStore::new(&queue_dir).expect("create file queue store");
        let req = QueuedRequest {
            request_id: "req:1".to_string(),
            idempotent_key: "idem-1".to_string(),
            created_unix_ms: 1_000,
            payload: vec![7, 8, 9],
        };
        store.enqueue(req.clone()).expect("file enqueue should succeed");

        let pending = store.list_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0], req);

        store
            .remove("req:1")
            .expect("file remove should succeed");
        assert!(store.list_pending().is_empty());
        let _ = fs::remove_dir_all(&queue_dir);
    }
}
