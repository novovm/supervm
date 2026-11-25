//! Provider-local authority for autonomous V8 mailbox sessions.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

const WINDOW: Duration = Duration::from_secs(60);
const MAX_DEVICE_SESSIONS: u16 = 16;

fn default_device_sessions() -> u16 {
    4
}

const REVOCATION_POLL: Duration = Duration::from_millis(500);
type DeviceId = [u8; 32];
type RevocationSource = Box<dyn Fn() -> Result<BTreeSet<Vec<u8>>> + Send + Sync>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateMailboxAdmissionConfig {
    pub session_seconds: u16,
    pub requests_per_minute: u16,
    pub tracked_devices: u16,
    #[serde(default = "default_device_sessions")]
    pub max_device_sessions: u16,
    #[serde(default)]
    pub denied_clients: BTreeSet<Vec<u8>>,
}

pub struct PrivateMailboxAdmission {
    config: PrivateMailboxAdmissionConfig,
    state: Mutex<State>,
    read_revocations: Option<RevocationSource>,
}

struct Device {
    until: Instant,
    until_ms: i64,
    remaining: usize,
    active_sessions: u16,
}

struct State {
    devices: BTreeMap<DeviceId, Device>,
    denied: BTreeSet<DeviceId>,
    checked_revocations: Option<Instant>,
    attempts: Option<Device>,
    db: Option<Connection>,
    clock: (Instant, i64),
}

fn valid_key(key: &[u8]) -> bool {
    key.len() == crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES
        && key.iter().any(|byte| *byte != 0)
}

fn device_id(key: &[u8]) -> DeviceId {
    Sha256::digest(key).into()
}

impl State {
    fn now_ms(&self, now: Instant) -> Result<i64> {
        let elapsed = i64::try_from(now.saturating_duration_since(self.clock.0).as_millis())?;
        self.clock
            .1
            .checked_add(elapsed)
            .context("admission clock overflow")
    }

    fn save(&self, slot: &DeviceId, device: &Device, now: Instant) -> Result<()> {
        if let Some(db) = &self.db {
            db.execute(
                "INSERT INTO apfl_admission_v1 VALUES (?1,?2,?3,?4)
                 ON CONFLICT(device) DO UPDATE SET
                   until_ms=excluded.until_ms, remaining=excluded.remaining,
                   observed_ms=excluded.observed_ms",
                params![
                    slot,
                    device.until_ms,
                    i64::try_from(device.remaining)?,
                    self.now_ms(now)?
                ],
            )?;
        }
        Ok(())
    }
}

impl PrivateMailboxAdmission {
    pub fn new(config: PrivateMailboxAdmissionConfig) -> Result<Self> {
        ensure!(
            (1..=60).contains(&config.session_seconds)
                && (1..=1024).contains(&config.requests_per_minute)
                && (1..=1024).contains(&config.tracked_devices)
                && (1..=MAX_DEVICE_SESSIONS).contains(&config.max_device_sessions)
                && config.denied_clients.len() <= 1024
                && config.denied_clients.iter().all(|pin| valid_key(pin)),
            "invalid autonomous mailbox admission limits"
        );
        let denied = config
            .denied_clients
            .iter()
            .map(|pin| device_id(pin))
            .collect();
        Ok(Self {
            config,
            read_revocations: None,
            state: Mutex::new(State {
                devices: BTreeMap::new(),
                denied,
                attempts: None,
                checked_revocations: None,
                db: None,
                clock: (
                    Instant::now(),
                    i64::try_from(crate::product_rendezvous_daemon::now_ms()?)?,
                ),
            }),
        })
    }

    /// Uses the active owner's existing mailbox database and transaction journal.
    /// An unusable database or revocation source prevents autonomous admission.
    pub fn with_storage(
        mut self,
        path: &Path,
        read_revocations: impl Fn() -> Result<BTreeSet<Vec<u8>>> + Send + Sync + 'static,
    ) -> Result<Self> {
        let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        db.busy_timeout(Duration::from_secs(2))?;
        db.execute_batch(
            "PRAGMA synchronous=EXTRA; PRAGMA trusted_schema=OFF;
             CREATE TABLE IF NOT EXISTS apfl_admission_v1 (
               device BLOB PRIMARY KEY CHECK(length(device)=32),
               until_ms INTEGER NOT NULL, remaining INTEGER NOT NULL CHECK(remaining BETWEEN 0 AND 1024),
               observed_ms INTEGER NOT NULL CHECK(observed_ms>0 AND until_ms>observed_ms
                   AND until_ms-observed_ms<=60000)
             );
             CREATE TABLE IF NOT EXISTS apfl_revoked_v1 (
               device BLOB PRIMARY KEY CHECK(length(device)=32)
             );",
        )?;
        let state = self
            .state
            .get_mut()
            .map_err(|_| anyhow::anyhow!("admission state unavailable"))?;
        let observed: i64 = db.query_row(
            "SELECT coalesce(max(observed_ms),0) FROM apfl_admission_v1",
            [],
            |r| r.get(0),
        )?;
        state.clock.1 = state.clock.1.max(observed);
        db.execute(
            "DELETE FROM apfl_admission_v1 WHERE until_ms<=?1",
            [state.clock.1],
        )?;
        {
            let mut query =
                db.prepare("SELECT device,until_ms,remaining FROM apfl_admission_v1 LIMIT 1026")?;
            let rows = query.query_map([], |r| {
                Ok((
                    r.get::<_, DeviceId>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, u16>(2)?,
                ))
            })?;
            for row in rows {
                let (slot, until_ms, remaining) = row?;
                let duration = Duration::from_millis(u64::try_from(until_ms - state.clock.1)?);
                ensure!(
                    duration <= WINDOW && remaining <= 1024,
                    "invalid durable APFL admission window"
                );
                let device = Device {
                    until_ms,
                    until: state.clock.0 + duration,
                    remaining: usize::from(remaining),
                    active_sessions: 0,
                };
                if slot == [0; 32] {
                    state.attempts = Some(device);
                } else {
                    state.devices.insert(slot, device);
                }
            }
            ensure!(
                state.devices.len() <= 1024,
                "durable APFL admission capacity exceeded"
            );
        }
        {
            let mut query = db.prepare("SELECT device FROM apfl_revoked_v1 LIMIT 1025")?;
            let ids = query.query_map([], |r| r.get::<_, DeviceId>(0))?;
            for id in ids {
                state.denied.insert(id?);
            }
            ensure!(
                state.denied.len() <= 1024,
                "durable APFL revocation capacity exceeded"
            );
        }
        let transaction = db.transaction()?;
        for id in &state.denied {
            transaction.execute("INSERT OR IGNORE INTO apfl_revoked_v1 VALUES (?1)", [id])?;
        }
        transaction.commit()?;
        state.db = Some(db);
        self.read_revocations = Some(Box::new(read_revocations));
        drop(self.state(Instant::now())?);
        Ok(self)
    }

    fn state(&self, now: Instant) -> Result<MutexGuard<'_, State>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("APFL admission unavailable"))?;
        if let Some(read) = &self.read_revocations {
            if state
                .checked_revocations
                .is_none_or(|checked| now.saturating_duration_since(checked) >= REVOCATION_POLL)
            {
                let keys = read()?;
                ensure!(
                    keys.len() <= 1024 && keys.iter().all(|key| valid_key(key)),
                    "invalid APFL revocation source"
                );
                let mut denied = state.denied.clone();
                denied.extend(keys.iter().map(|key| device_id(key)));
                ensure!(denied.len() <= 1024, "APFL revocation capacity exhausted");
                if denied != state.denied {
                    let db = state
                        .db
                        .as_mut()
                        .context("missing durable APFL revocation owner")?;
                    let transaction = db.transaction()?;
                    for id in &denied {
                        transaction
                            .execute("INSERT OR IGNORE INTO apfl_revoked_v1 VALUES (?1)", [id])?;
                    }
                    transaction.commit()?;
                    state.denied = denied;
                }
                state.checked_revocations = Some(now);
            }
        }
        Ok(state)
    }

    /// Bound cryptographic work before reading peer proof. Failed and repeated
    /// attempts consume this provider budget independently of any device grant.
    pub(super) fn authenticate_at(&self, now: Instant) -> Result<()> {
        let mut state = self.state(now)?;
        let maximum = usize::from(self.config.requests_per_minute);
        let mut attempts = match state.attempts.take() {
            Some(device) if device.until > now => device,
            _ => Device {
                until: now + WINDOW,
                until_ms: state
                    .now_ms(now)?
                    .checked_add(60_000)
                    .context("admission window overflow")?,
                remaining: maximum,
                active_sessions: 0,
            },
        };
        let result = attempts
            .remaining
            .min(maximum)
            .checked_sub(1)
            .context("APFL authentication budget exhausted")
            .and_then(|remaining| {
                attempts.remaining = remaining;
                state.save(&[0; 32], &attempts, now)
            });
        state.attempts = Some(attempts);
        result
    }

    #[cfg(feature = "private-apfl-listener")]
    pub(super) fn check_authenticated_at(&self, key: &[u8], now: Instant) -> Result<()> {
        ensure!(
            valid_key(key) && !self.state(now)?.denied.contains(&device_id(key)),
            "APFL device revoked or key invalid"
        );
        Ok(())
    }

    pub(super) fn grant_at(
        &self,
        authenticated: &[u8],
        now: Instant,
    ) -> Result<PrivateMailboxGrant<'_>> {
        ensure!(valid_key(authenticated), "invalid APFL device key");
        let slot = device_id(authenticated);
        ensure!(slot != [0; 32], "invalid APFL device commitment");
        let mut state = self.state(now)?;
        ensure!(!state.denied.contains(&slot), "APFL device revoked");
        let now_ms = state.now_ms(now)?;
        let expired = state
            .devices
            .values()
            .any(|device| device.active_sessions == 0 && device.until <= now);
        if expired {
            if let Some(db) = &state.db {
                db.execute("DELETE FROM apfl_admission_v1 WHERE until_ms<=?1", [now_ms])?;
            }
        }
        let devices = &mut state.devices;
        devices.retain(|_, device| device.active_sessions > 0 || device.until > now);
        ensure!(
            devices.contains_key(&slot) || devices.len() < usize::from(self.config.tracked_devices),
            "autonomous mailbox admission capacity exhausted"
        );
        let device = devices.entry(slot).or_insert(Device {
            until: now + WINDOW,
            until_ms: now_ms
                .checked_add(60_000)
                .context("admission window overflow")?,
            remaining: usize::from(self.config.requests_per_minute),
            active_sessions: 0,
        });
        ensure!(
            device.active_sessions < self.config.max_device_sessions
                && device.until > now
                && device.remaining > 0,
            "device mailbox session or request quota exhausted"
        );
        device.active_sessions += 1;
        Ok(PrivateMailboxGrant {
            owner: self,
            slot,
            expires: device
                .until
                .min(now + Duration::from_secs(u64::from(self.config.session_seconds))),
        })
    }
}

/// Issued only after the existing V8 Finished proof. This grant lives inside
/// that exact server session. It cannot be exported, copied or presented to a
/// different carrier. Mailbox read/write capabilities remain separate.
pub(super) struct PrivateMailboxGrant<'a> {
    owner: &'a PrivateMailboxAdmission,
    slot: [u8; 32],
    expires: Instant,
}

impl PrivateMailboxGrant<'_> {
    pub(super) fn check_at(&self, now: Instant) -> Result<()> {
        ensure!(now < self.expires, "autonomous APFL grant expired");
        ensure!(
            !self.owner.state(now)?.denied.contains(&self.slot),
            "APFL device revoked"
        );
        Ok(())
    }

    pub(super) fn charge_at(&self, rows: usize, now: Instant) -> Result<()> {
        ensure!(now < self.expires, "autonomous APFL grant expired");
        let mut state = self.owner.state(now)?;
        ensure!(!state.denied.contains(&self.slot), "APFL device revoked");
        let device = state
            .devices
            .get(&self.slot)
            .context("mailbox grant no longer exists")?;
        let remaining = device
            .remaining
            .min(usize::from(self.owner.config.requests_per_minute))
            .checked_sub(rows)
            .context("autonomous mailbox request quota exhausted")?;
        let charged = Device {
            remaining,
            ..*device
        };
        state.save(&self.slot, &charged, now)?;
        state.devices.insert(self.slot, charged);
        Ok(())
    }
}

impl Drop for PrivateMailboxGrant<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.owner.state.lock() {
            if let Some(device) = state.devices.get_mut(&self.slot) {
                device.active_sessions = device.active_sessions.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES;

    #[test]
    fn quota_and_live_revocation_survive_mailbox_restart() -> Result<()> {
        let client = vec![1; MLDSA65_PUBLIC_KEY_BYTES];
        let revoked = vec![2; MLDSA65_PUBLIC_KEY_BYTES];
        let root = tempfile::tempdir()?;
        let path = root.path().join("mailbox.sqlite");
        drop(crate::mailbox_store::MailboxStore::open(
            &path,
            Default::default(),
        )?);
        let policy = std::sync::Arc::new(Mutex::new(BTreeSet::new()));
        let open = || {
            let source = policy.clone();
            PrivateMailboxAdmission::new(PrivateMailboxAdmissionConfig {
                session_seconds: 30,
                requests_per_minute: 2,
                tracked_devices: 2,
                max_device_sessions: 1,
                denied_clients: Default::default(),
            })?
            .with_storage(&path, move || Ok(source.lock().unwrap().clone()))
        };
        let owner = open()?;
        let now = Instant::now();
        owner.authenticate_at(now)?;
        owner.grant_at(&client, now)?.charge_at(1, now)?;
        let grant = owner.grant_at(&revoked, now)?;
        policy.lock().unwrap().insert(revoked.clone());
        assert!(grant.check_at(now + REVOCATION_POLL).is_err());
        drop(grant);
        drop(owner);

        policy.lock().unwrap().clear();
        let owner = open()?;
        let now = Instant::now();
        owner.authenticate_at(now)?;
        assert!(owner.authenticate_at(now).is_err());
        assert!(owner.grant_at(&revoked, now).is_err());
        let resumed = owner.grant_at(&client, now)?;
        assert!(resumed.charge_at(2, now).is_err());
        resumed.charge_at(1, now)?;
        assert!(resumed.charge_at(1, now).is_err());
        Ok(())
    }

    #[test]
    fn concurrent_circuits_share_the_device_budget_and_release_one_seat() -> Result<()> {
        let pin = vec![1; MLDSA65_PUBLIC_KEY_BYTES];
        let owner = PrivateMailboxAdmission::new(serde_json::from_str(
            r#"{"session_seconds":30,"requests_per_minute":8,"tracked_devices":1}"#,
        )?)?;
        let now = Instant::now();
        let first = owner.grant_at(&pin, now)?;
        let second = owner.grant_at(&pin, now)?;
        let third = owner.grant_at(&pin, now)?;
        let fourth = owner.grant_at(&pin, now)?;
        assert!(owner.grant_at(&pin, now).is_err());
        first.charge_at(3, now)?;
        second.charge_at(2, now)?;
        third.charge_at(1, now)?;
        drop(first);
        let replacement = owner.grant_at(&pin, now)?;
        assert!(owner.grant_at(&pin, now).is_err());
        replacement.charge_at(2, now)?;
        assert!(fourth.charge_at(1, now).is_err());
        assert!(owner.grant_at(&pin, now).is_err());
        Ok(())
    }

    #[test]
    fn autonomous_mailbox_grant_limits_reconnects_and_excludes_denied_keys() -> Result<()> {
        let first = vec![1; MLDSA65_PUBLIC_KEY_BYTES];
        let second = vec![2; MLDSA65_PUBLIC_KEY_BYTES];
        let denied = vec![3; MLDSA65_PUBLIC_KEY_BYTES];
        let owner = PrivateMailboxAdmission::new(PrivateMailboxAdmissionConfig {
            session_seconds: 20,
            requests_per_minute: 5,
            tracked_devices: 1,
            max_device_sessions: 1,
            denied_clients: [denied.clone()].into(),
        })?;
        let now = Instant::now();
        assert!(owner.grant_at(&denied, now).is_err());
        let grant = owner.grant_at(&first, now)?;
        grant.charge_at(3, now)?;
        assert!(owner.grant_at(&first, now).is_err());
        assert!(owner.grant_at(&second, now).is_err());
        assert!(grant.charge_at(1, now + Duration::from_secs(20)).is_err());
        drop(grant);
        let reopened = owner.grant_at(&first, now + Duration::from_secs(20))?;
        assert!(reopened
            .charge_at(3, now + Duration::from_secs(20))
            .is_err());
        reopened.charge_at(2, now + Duration::from_secs(20))?;
        drop(reopened);
        assert!(owner
            .grant_at(&first, now + Duration::from_secs(30))
            .is_err());
        owner
            .grant_at(&second, now + WINDOW)?
            .charge_at(5, now + WINDOW)?;
        Ok(())
    }
}
