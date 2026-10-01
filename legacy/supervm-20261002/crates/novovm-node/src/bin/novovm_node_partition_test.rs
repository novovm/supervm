use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
};

static WORKER: AtomicBool = AtomicBool::new(false);

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Control {
    phase: String,
    allowed: BTreeMap<String, Vec<u8>>,
}

impl Control {
    fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 4096, "partition control too large");
        let control: Self = serde_json::from_slice(bytes)?;
        ensure!(
            ["seed", "partition", "heal-prepare", "heal-decision"]
                .contains(&control.phase.as_str())
                && control.allowed.len() <= 4
                && control.allowed.iter().all(|(peer, kinds)| !peer.is_empty()
                    && peer.len() <= 128
                    && kinds.len() <= 10
                    && kinds.iter().all(|kind| (1..=10).contains(kind))),
            "invalid partition control"
        );
        Ok(control)
    }

    fn allows(&self, peer: &str, payload: &[u8]) -> bool {
        payload.starts_with(b"NOVSRW01")
            && payload.get(10).is_some_and(|kind| {
                self.allowed
                    .get(peer)
                    .is_some_and(|kinds| kinds.contains(kind))
            })
    }
}

pub(super) struct Controller {
    control: Control,
    counts: BTreeMap<String, u64>,
}

impl Controller {
    pub(super) fn open() -> Result<Option<Self>> {
        Ok(WORKER.load(Ordering::Acquire).then_some(Self {
            control: Control::default(),
            counts: BTreeMap::new(),
        }))
    }

    pub(super) fn refresh(&mut self) -> Result<()> {
        let mut bytes = Vec::new();
        fs::File::open("partition-control.json")?
            .take(4097)
            .read_to_end(&mut bytes)?;
        let control = Control::parse(&bytes)?;
        if control.phase != self.control.phase {
            self.counts.clear();
        }
        self.control = control;
        Ok(())
    }

    pub(super) fn allow(&mut self, peer: &str, payload: &[u8]) -> bool {
        let allowed = self.control.allows(peer, payload);
        let kind = payload.get(10).copied().unwrap_or_default();
        let key = format!("{}:{kind}", if allowed { "allowed" } else { "dropped" });
        *self.counts.entry(key).or_default() += 1;
        allowed
    }

    pub(super) fn record(&self, status: &Value) -> Result<()> {
        let report = json!({
            "instrumentation":"cfg_test_main_entry_ingress_drop_only",
            "pid":std::process::id(), "phase":self.control.phase,
            "counts":self.counts, "status":status,
        });
        fs::write("partition-status.next", serde_json::to_vec(&report)?)?;
        fs::rename("partition-status.next", "partition-status.json")?;
        Ok(())
    }
}

#[test]
#[ignore = "child-only real main entry; requires disposable loopback fixture"]
fn main_entry_worker() -> Result<()> {
    ensure!(
        std::env::var("NOVOVM_TEST_PARTITION_WORKER").as_deref() == Ok("1")
            && std::env::var("NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID").as_deref() == Ok("98919601")
            && std::env::var("NOVOVM_NODE_MODE").as_deref() == Ok("native_execution_tick"),
        "partition worker requires explicit disposable fixture"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|parent| parent.parent())
        .context("repository root")?
        .join("artifacts/audit/candidate-node-processes")
        .canonicalize()?;
    ensure!(
        std::env::current_dir()?.starts_with(root),
        "private test cwd required"
    );
    let overlay = fs::read_to_string("overlay.json")?;
    ensure!(
        overlay.contains("wss://127.0.0.2:443/novovm"),
        "loopback fixture required"
    );
    WORKER.store(true, Ordering::Release);
    super::main()
}

#[test]
fn controller_rejects_invalid_control_and_filters_only_allowlisted_frames() {
    assert!(Control::parse(&vec![b' '; 4097]).is_err());
    assert!(Control::parse(br#"{"phase":"typo","allowed":{}}"#).is_err());
    assert!(Control::parse(br#"{"phase":"seed","allowed":{"peer":[11]}}"#).is_err());
    assert!(Control::parse(br#"{"phase":"seed","allowed":{},"extra":true}"#).is_err());
    let control = Control::parse(br#"{"phase":"seed","allowed":{"peer":[4,6]}}"#).unwrap();
    let mut payload = b"NOVSRW01\0\0\x04".to_vec();
    assert!(control.allows("peer", &payload));
    assert!(!control.allows("other", &payload));
    payload[10] = 9;
    assert!(!control.allows("peer", &payload));
    payload[0] ^= 1;
    assert!(!control.allows("peer", &payload));
    assert!(!control.allows("peer", b"NOVSRW01"));
    assert!(Controller::open().unwrap().is_none());
}
