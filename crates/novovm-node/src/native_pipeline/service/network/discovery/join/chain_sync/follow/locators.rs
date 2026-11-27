use super::*;
use novovm_network::{
    discovery_control::{
        decode_node_locator_v1, decode_public_read_locator_v1, encode_node_locator_v1,
        encode_public_read_locator_v1, native_locator_id_v1, public_read_locator_id_v1,
    },
    product_service_lease::MAX_SERVICE_LEASE_MS,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(in super::super::super) use tests::check_locator_role_transition;

const MAX_PUBLIC_READERS: usize = 8;
const MAX_FIXED_READERS: usize = 28;

type Records = BTreeMap<Vec<u8>, PeerSignedRelayRecordV1>;

fn keeps_public_floor(record: &PeerSignedRelayRecordV1, now: u64) -> bool {
    // An older record may have a longer signed lifetime than its successor.
    now < record.issued_at_ms.saturating_add(MAX_SERVICE_LEASE_MS)
}

/// Retain independent signed sequence floors for each authorized purpose.
/// Public floors become dialable only when returned by a current discovery round.
pub(super) struct Locators {
    path: PathBuf,
    fixed: Records,
    public: Records,
    observations: BTreeMap<Vec<u8>, Observation>,
}

#[derive(Clone, Copy)]
enum Observation {
    Advertised,
    Conflict(u64),
}

pub(super) struct AcceptedLocator {
    pub(super) peer_transport_sha256: String,
    pub(super) sequence: u64,
    pub(super) issued_at_ms: u64,
    pub(super) expires_at_ms: u64,
    pub(super) returned_by: Vec<String>,
}

impl Locators {
    pub(super) fn open(
        path: PathBuf,
        profile: &Profile,
        identity: &TransportIdentity,
    ) -> Result<Self> {
        let entries: Vec<PeerSignedRelayRecordV1> = if path.try_exists()? {
            read_document(&path)?
        } else {
            Vec::new()
        };
        ensure!(
            entries.len() <= MAX_FIXED_READERS + MAX_PUBLIC_READERS,
            "installed locator checkpoint inventory"
        );
        let now = wall_ms()?;
        let mut fixed = Records::new();
        let mut public = Records::new();
        let public_id = public_read_locator_id_v1(profile.pins());
        for record in entries {
            ensure!(
                record.issued_at_ms <= now,
                "future installed locator checkpoint"
            );
            let target = if record.record_id == public_id {
                let wire = encode_public_read_locator_v1(&record, profile.pins())?;
                decode_public_read_locator_v1(
                    &wire,
                    &record.relay_public_key,
                    profile.pins(),
                    record.issued_at_ms,
                    identity,
                )
                .context("authenticating public read locator checkpoint")?;
                if !keeps_public_floor(&record, now) {
                    continue;
                }
                ensure!(
                    public.len() < MAX_PUBLIC_READERS,
                    "public reader checkpoint inventory"
                );
                &mut public
            } else {
                let wire = encode_node_locator_v1(&record, profile.pins())?;
                decode_node_locator_v1(
                    &wire,
                    &record.relay_public_key,
                    profile.pins(),
                    record.issued_at_ms,
                    identity,
                )
                .context("authenticating installed locator checkpoint")?;
                ensure!(
                    fixed.len() < MAX_FIXED_READERS,
                    "fixed reader checkpoint inventory"
                );
                &mut fixed
            };
            ensure!(
                target
                    .insert(record.relay_public_key.clone(), record)
                    .is_none(),
                "duplicate installed locator purpose and key"
            );
        }
        Ok(Self {
            path,
            fixed,
            public,
            observations: BTreeMap::new(),
        })
    }

    pub(super) fn fixed<'a>(
        &'a self,
        profile: &'a Profile,
    ) -> impl Iterator<Item = &'a PeerSignedRelayRecordV1> {
        profile
            .lookup_transport_keys
            .iter()
            .filter_map(|key| self.fixed.get(key))
    }

    pub(super) fn candidates<'a>(
        &'a self,
        profile: &'a Profile,
        now: u64,
    ) -> impl Iterator<Item = &'a PeerSignedRelayRecordV1> {
        self.fixed(profile)
            .chain(self.observations.iter().filter_map(|(key, observed)| {
                matches!(observed, Observation::Advertised)
                    .then(|| self.public.get(key))
                    .flatten()
            }))
            .filter(move |record| record.expires_at_ms > now)
    }

    pub(super) fn observe(
        &mut self,
        found: DiscoveryReport,
        profile: &Profile,
        local_key: &[u8],
    ) -> Result<Vec<AcceptedLocator>> {
        let now = wall_ms()?;
        let public_id = public_read_locator_id_v1(profile.pins());
        let fixed_id = native_locator_id_v1(profile.pins());
        let plan = InstalledLookups::new(profile);
        let mut public = self.public.clone();
        public.retain(|_, r| keeps_public_floor(r, now));
        let pruned = public.len() != self.public.len();
        let mut fixed = self.fixed.clone();
        // Sampling returns at most one public reader per source. Omissions carry no
        // withdrawal information; each verified advertisement lasts its signed lease.
        let mut observations = self.observations.clone();
        observations.retain(|key, _| public.contains_key(key));
        for (key, sequence) in found.conflicting_locator_keys {
            if public.get(&key).is_some_and(|r| r.sequence <= sequence) {
                let status = observations
                    .entry(key)
                    .or_insert(Observation::Conflict(sequence));
                if !matches!(status, Observation::Conflict(previous) if *previous >= sequence) {
                    *status = Observation::Conflict(sequence);
                }
            }
        }
        let mut accepted = Vec::new();
        for (key, candidate) in found.records {
            if candidate.issued_at_ms > now
                || candidate.expires_at_ms <= now
                || key != candidate.relay_public_key
            {
                continue;
            }
            let is_public = candidate.record_id == public_id;
            let target = if is_public {
                if matches!(observations.get(&key), Some(Observation::Conflict(sequence)) if candidate.sequence <= *sequence)
                {
                    continue;
                }
                if !profile.public_read_referrals()
                    || !plan.accepts_public_reader(&candidate, local_key)
                {
                    continue;
                }
                if public.len() >= MAX_PUBLIC_READERS && !public.contains_key(&key) {
                    continue;
                }
                &mut public
            } else {
                if candidate.record_id != fixed_id || !profile.lookup_transport_keys.contains(&key)
                {
                    continue;
                }
                if fixed.len() >= MAX_FIXED_READERS && !fixed.contains_key(&key) {
                    // An updated release may replace its entire fixed inventory.
                    let retired = fixed
                        .iter()
                        .filter(|(key, _)| !profile.lookup_transport_keys.contains(key))
                        .min_by_key(|(_, r)| r.issued_at_ms)
                        .map(|(key, _)| key.clone())
                        .context("installed fixed locator inventory")?;
                    fixed.remove(&retired);
                }
                &mut fixed
            };
            if let Some(current) = target.get(&key) {
                if candidate.sequence < current.sequence
                    || candidate.issued_at_ms < current.issued_at_ms
                {
                    continue;
                }
                if candidate.sequence == current.sequence {
                    if candidate != *current {
                        // A conflicting record cannot replace an accepted fixed pin.
                        // An unpinned reader needs a later unambiguous advertisement.
                        if is_public {
                            observations.insert(key, Observation::Conflict(candidate.sequence));
                        }
                    } else if is_public {
                        observations.insert(key, Observation::Advertised);
                    }
                    continue;
                }
            }
            // A bootstrap that returned an earlier sequence did not supply the
            // winning record, even if it authenticated in this round.
            let summary = LocatorSummary::new(&candidate, &public_id);
            let returned_by = found
                .locators_by_source
                .iter()
                .filter(|(_, records)| records.contains(&summary))
                .map(|(source, _)| source.clone())
                .collect();
            accepted.push(AcceptedLocator {
                peer_transport_sha256: summary.transport_identity_sha256,
                sequence: candidate.sequence,
                issued_at_ms: candidate.issued_at_ms,
                expires_at_ms: candidate.expires_at_ms,
                returned_by,
            });
            if is_public {
                observations.insert(key.clone(), Observation::Advertised);
            }
            target.insert(key, candidate);
        }
        if pruned || !accepted.is_empty() {
            let entries: Vec<_> = fixed.values().chain(public.values()).collect();
            let bytes = serde_json::to_vec(&entries)?;
            ensure!(
                bytes.len() <= MAX_CONFIG,
                "installed locator checkpoint size"
            );
            store_checkpoint(&self.path, &bytes)?;
            self.public = public;
            self.fixed = fixed;
        }
        self.observations = observations;
        Ok(accepted)
    }
}
