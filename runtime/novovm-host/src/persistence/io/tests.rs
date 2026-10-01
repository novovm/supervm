//! Admission tests use only channels/permits, never pretend to write durable
//! content. The explicit real-library test exercises the actual I/O owner.

use super::*;
use crate::persistence::{PacketBudget, StorageDomain};
use crate::state::tree::{empty_root, stage_state_update, StateChange, StateNodeReader};
use novovm_aoem::StorageConfig;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn admission_service(budget: IoBudget, capacity: usize) -> (IoService, mpsc::Receiver<Command>) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let service = IoService {
        sender,
        worker: thread::spawn(|| {}),
        usage: Arc::new(Mutex::new(Usage::default())),
        budget,
    };
    (service, receiver)
}

fn usage(usage: &Arc<Mutex<Usage>>) -> (usize, usize) {
    let current = usage.lock().unwrap_or_else(|error| error.into_inner());
    (current.requests, current.bytes)
}

#[test]
fn permit_count_and_bytes_remain_reserved_until_last_owner_releases() {
    let (service, _receiver) = admission_service(
        IoBudget {
            requests: 2,
            bytes: 100,
        },
        2,
    );
    let first = service.reserve(60).unwrap().unwrap();
    let retained = first.clone();
    assert!(service.reserve(41).unwrap().is_none());
    let second = service.reserve(40).unwrap().unwrap();
    assert_eq!(usage(&service.usage), (2, 100));
    assert!(service.reserve(1).unwrap().is_none());
    assert!(service.reserve(101).is_err());
    drop(first);
    assert_eq!(usage(&service.usage), (2, 100));
    drop(retained);
    assert_eq!(usage(&service.usage), (1, 40));
    let third = service.reserve(60).unwrap().unwrap();
    drop(second);
    drop(third);
    assert_eq!(usage(&service.usage), (0, 0));
}

#[test]
fn completed_unconsumed_reply_still_applies_count_and_byte_backpressure() {
    let bytes = 32 + 291 + 1;
    let (service, receiver) = admission_service(IoBudget { requests: 1, bytes }, 1);
    let mut ticket = service.try_read_nodes(vec![[1; 32]]).unwrap().unwrap();
    assert!(ticket.try_take().unwrap().is_none());
    let command = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    let Command { operation, permit } = command;
    let Operation::Nodes { hashes, reply } = operation else {
        panic!("wrong operation")
    };
    assert_eq!(hashes, vec![[1; 32]]);
    // Only a typed channel reply: this test makes no disk-read/write assertion.
    reply.send(Ok(vec![None])).unwrap();
    drop(reply);
    drop(permit);
    assert_eq!(usage(&service.usage), (1, bytes));
    assert!(service.try_read_nodes(vec![[2; 32]]).unwrap().is_none());
    assert_eq!(ticket.try_take().unwrap(), Some(vec![None]));
    assert_eq!(usage(&service.usage), (0, 0));
    assert!(ticket.try_take().is_err());
}

#[test]
fn dropping_ticket_never_removes_accepted_command_or_its_live_permit() {
    let bytes = 32 + 291 + 1;
    let (service, receiver) = admission_service(IoBudget { requests: 1, bytes }, 1);
    let ticket = service.try_read_nodes(vec![[3; 32]]).unwrap().unwrap();
    drop(ticket);
    assert_eq!(usage(&service.usage), (1, bytes));
    assert!(service.try_read_nodes(vec![[4; 32]]).unwrap().is_none());
    let command = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    let Command { operation, permit } = command;
    let Operation::Nodes { hashes, reply } = operation else {
        panic!("accepted command was changed")
    };
    assert_eq!(hashes, vec![[3; 32]]);
    assert!(
        reply.send(Ok(vec![None])).is_err(),
        "only notification was abandoned"
    );
    assert_eq!(usage(&service.usage), (1, bytes));
    drop(permit);
    assert_eq!(usage(&service.usage), (0, 0));
}

#[test]
fn full_or_disconnected_command_queue_returns_its_admission_budget() {
    let bytes = 32 + 291 + 1;
    let (service, receiver) = admission_service(
        IoBudget {
            requests: 3,
            bytes: bytes * 3,
        },
        1,
    );
    let first = service.try_read_nodes(vec![[1; 32]]).unwrap().unwrap();
    assert!(service.try_read_nodes(vec![[2; 32]]).unwrap().is_none());
    assert_eq!(usage(&service.usage), (1, bytes));
    drop(receiver);
    assert!(service.try_read_nodes(vec![[3; 32]]).is_err());
    assert_eq!(usage(&service.usage), (1, bytes));
    drop(first);
    assert_eq!(usage(&service.usage), (0, 0));
}

#[test]
fn error_and_disconnected_replies_release_ticket_without_inventing_success() {
    let (service, receiver) = admission_service(
        IoBudget {
            requests: 2,
            bytes: 2048,
        },
        2,
    );
    let mut failed = service.try_read_nodes(vec![[1; 32]]).unwrap().unwrap();
    let Command { operation, permit } = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    let Operation::Nodes { reply, .. } = operation else {
        panic!("wrong operation")
    };
    reply
        .send(Err(anyhow::anyhow!("injected reply error")))
        .unwrap();
    drop(reply);
    drop(permit);
    assert!(failed
        .try_take()
        .unwrap_err()
        .to_string()
        .contains("injected reply error"));
    assert_eq!(usage(&service.usage), (0, 0));

    let mut disconnected = service.try_read_nodes(vec![[2; 32]]).unwrap().unwrap();
    drop(receiver.recv_timeout(Duration::from_secs(1)).unwrap());
    let error = disconnected.try_take().unwrap_err();
    assert!(error.to_string().contains("outcome unknown"));
    assert_eq!(usage(&service.usage), (0, 0));
    assert!(disconnected.try_take().is_err());
}

#[test]
fn local_validation_and_accounting_contention_admit_nothing() {
    let (service, receiver) = admission_service(
        IoBudget {
            requests: 2,
            bytes: 32_768,
        },
        2,
    );
    assert!(service.try_read_nodes(Vec::new()).is_err());
    assert!(service.try_read_nodes(vec![[1; 32]; 65]).is_err());
    assert!(service.try_read_value(empty_root(), Vec::new()).is_err());
    assert!(service.try_read_value(empty_root(), vec![1; 257]).is_err());
    assert_eq!(usage(&service.usage), (0, 0));
    let held = service.usage.lock().unwrap();
    assert!(service.try_read_nodes(vec![[1; 32]]).unwrap().is_none());
    drop(held);
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(usage(&service.usage), (0, 0));
}

#[test]
fn poisoned_admission_accounting_rejects_without_channel_submission() {
    let (service, receiver) = admission_service(
        IoBudget {
            requests: 1,
            bytes: 1024,
        },
        1,
    );
    let accounting = service.usage.clone();
    assert!(thread::spawn(move || {
        let _held = accounting.lock().unwrap();
        panic!("intentional accounting poison");
    })
    .join()
    .is_err());
    assert!(service.try_read_nodes(vec![[1; 32]]).is_err());
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(usage(&service.usage), (0, 0));
}

struct Empty;
impl StateNodeReader for Empty {
    fn read_node(&self, _: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

// Real admission is nonblocking: brief accounting contention is legitimate
// backpressure, not a request failure. Never retry an Err or an accepted job.
fn admit_real<T>(mut submit: impl FnMut() -> Result<Option<IoTicket<T>>>) -> Result<IoTicket<T>> {
    let started = Instant::now();
    loop {
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "real I/O admission timed out"
        );
        if let Some(ticket) = submit()? {
            return Ok(ticket);
        }
        thread::sleep(
            Duration::from_millis(1).min(Duration::from_secs(5).saturating_sub(started.elapsed())),
        );
    }
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; real local I/O, not node/TPS evidence"]
fn real_io_bulk_reads_keep_order_missing_values_and_reply_budget_through_shutdown() -> Result<()> {
    let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted NOVOVM_AOEM_TEST_LIBRARY is required")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/io-tests")
        .join(format!("novovm-io-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let config = StoreConfig {
        library: library.into(),
        database: directory.join("state.rocksdb"),
        domain: StorageDomain {
            chain_id: 391,
            genesis_config_commitment: [11; 32],
            protocol_commitment: [12; 32],
        },
        storage: StorageConfig::default(),
        packet_budget: PacketBudget::default(),
    };
    let changes: Vec<_> = (0..130)
        .map(|index| StateChange::Put {
            key: format!("io/{index:04}").into_bytes(),
            value: vec![index as u8; 256],
        })
        .collect();
    let update = stage_state_update(&Empty, empty_root(), &changes)?;
    let mut small_value_config = config.clone();
    small_value_config.database = directory.join("small-value.rocksdb");
    small_value_config.packet_budget.max_value_bytes = 290;
    let small_store = CandidateStore::open(small_value_config, OpenMode::CreateNew)?;
    assert!(small_store.install_unpublished_state(&update).is_err());
    assert!(!small_store.is_write_frozen());
    assert!(small_store.read_node(&update.root())?.is_none());
    drop(small_store);
    let store = CandidateStore::open(config.clone(), OpenMode::CreateNew)?;
    store.install_unpublished_state(&update)?;
    drop(store);

    let mut hashes: Vec<_> = update.nodes().keys().copied().take(62).collect();
    assert_eq!(hashes.len(), 62);
    hashes.push(hashes[0]);
    let missing = [254; 32];
    assert!(!update.nodes().contains_key(&missing));
    hashes.push(missing);
    let expected: Vec<_> = hashes
        .iter()
        .map(|hash| update.nodes().get(hash).cloned())
        .collect();
    let key = b"io/0037".to_vec();
    let reserved_bytes = 64 * (32 + 291 + 1) + key.len() + 32 + 257;
    let service = IoService::start(
        config.clone(),
        OpenMode::Existing,
        IoBudget {
            requests: 2,
            bytes: reserved_bytes,
        },
    )?;
    let accounting = service.usage.clone();
    let nodes = admit_real(|| service.try_read_nodes(hashes.clone()))?;
    let value = admit_real(|| service.try_read_value(update.root(), key.clone()))?;
    assert!(service.try_read_nodes(vec![missing])?.is_none());
    // Both native reads really finish before shutdown returns. Their unconsumed
    // replies still retain admission permits; completion is not consumption.
    service.shutdown()?;
    assert_eq!(usage(&accounting), (2, reserved_bytes));
    assert_eq!(nodes.wait()?, expected);
    assert_eq!(usage(&accounting).0, 1);
    assert_eq!(value.wait()?, Some(vec![37; 256]));
    assert_eq!(usage(&accounting), (0, 0));

    // Missing *node* is an error, unlike an authenticated absent tree key.
    // This ordinary request failure does not manufacture a storage poison.
    let service = IoService::start(config, OpenMode::Existing, IoBudget::default())?;
    let absent = admit_real(|| service.try_read_value(update.root(), b"not-present".to_vec()))?;
    assert_eq!(absent.wait()?, None);
    let invalid_root = admit_real(|| service.try_read_value(missing, b"io/0037".to_vec()))?;
    assert!(invalid_root.wait().is_err());
    let still_live = admit_real(|| service.try_read_value(update.root(), b"io/0037".to_vec()))?;
    assert_eq!(still_live.wait()?, Some(vec![37; 256]));
    service.shutdown()?;
    Ok(())
}
