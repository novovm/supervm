//! Provider-local authority for autonomous V8 mailbox sessions.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
    time::{Duration, Instant},
};

const WINDOW: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateMailboxAdmissionConfig {
    pub session_seconds: u16,
    pub requests_per_minute: u16,
    pub tracked_devices: u16,
    #[serde(default)]
    pub denied_clients: BTreeSet<Vec<u8>>,
}

pub struct PrivateMailboxAdmission {
    config: PrivateMailboxAdmissionConfig,
    devices: Mutex<BTreeMap<[u8; 32], Device>>,
}

struct Device {
    until: Instant,
    remaining: usize,
    active: bool,
}

impl PrivateMailboxAdmission {
    pub fn new(config: PrivateMailboxAdmissionConfig) -> Result<Self> {
        ensure!(
            (1..=60).contains(&config.session_seconds)
                && (1..=1024).contains(&config.requests_per_minute)
                && (1..=1024).contains(&config.tracked_devices)
                && config.denied_clients.len() <= 1024
                && config.denied_clients.iter().all(|pin| {
                    pin.len() == crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES
                        && pin.iter().any(|byte| *byte != 0)
                }),
            "invalid autonomous mailbox admission limits"
        );
        Ok(Self {
            config,
            devices: Mutex::new(BTreeMap::new()),
        })
    }

    pub(super) fn grant_at(
        &self,
        authenticated: &[u8],
        now: Instant,
    ) -> Result<PrivateMailboxGrant<'_>> {
        ensure!(
            authenticated.len() == crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES
                && !self.config.denied_clients.contains(authenticated),
            "mailbox device denied by provider"
        );
        let slot = Sha256::digest(authenticated).into();
        let mut devices = self
            .devices
            .lock()
            .map_err(|_| anyhow::anyhow!("mailbox admission unavailable"))?;
        devices.retain(|_, device| device.active || device.until > now);
        ensure!(
            devices.contains_key(&slot) || devices.len() < usize::from(self.config.tracked_devices),
            "autonomous mailbox admission capacity exhausted"
        );
        let device = devices.entry(slot).or_insert(Device {
            until: now + WINDOW,
            remaining: usize::from(self.config.requests_per_minute),
            active: false,
        });
        ensure!(
            !device.active && device.until > now && device.remaining > 0,
            "device already active or its mailbox quota is exhausted"
        );
        device.active = true;
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
        ensure!(now < self.expires, "autonomous mailbox grant expired");
        Ok(())
    }

    pub(super) fn charge_at(&self, rows: usize, now: Instant) -> Result<()> {
        self.check_at(now)?;
        let mut devices = self
            .owner
            .devices
            .lock()
            .map_err(|_| anyhow::anyhow!("mailbox admission unavailable"))?;
        let device = devices
            .get_mut(&self.slot)
            .context("mailbox grant no longer exists")?;
        device.remaining = device
            .remaining
            .checked_sub(rows)
            .context("autonomous mailbox request quota exhausted")?;
        Ok(())
    }
}

impl Drop for PrivateMailboxGrant<'_> {
    fn drop(&mut self) {
        if let Ok(mut devices) = self.owner.devices.lock() {
            if let Some(device) = devices.get_mut(&self.slot) {
                device.active = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES;

    #[test]
    fn autonomous_mailbox_grant_limits_reconnects_and_excludes_denied_keys() -> Result<()> {
        let first = vec![1; MLDSA65_PUBLIC_KEY_BYTES];
        let second = vec![2; MLDSA65_PUBLIC_KEY_BYTES];
        let denied = vec![3; MLDSA65_PUBLIC_KEY_BYTES];
        let owner = PrivateMailboxAdmission::new(PrivateMailboxAdmissionConfig {
            session_seconds: 20,
            requests_per_minute: 5,
            tracked_devices: 1,
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
