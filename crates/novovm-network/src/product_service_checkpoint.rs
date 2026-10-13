//! Local recovery format, not a network protocol. Authenticated encryption and
//! atomic compare-and-swap are supplied by the platform's single storage owner.
//! An older complete valid backup still needs an external freshness anchor.
use super::*;

pub const MAX_SERVICE_CHECKPOINT_BYTES: usize = 128 * 1024;

pub trait ServiceLeasePersistenceV1: Send + Sync {
    /// Must durably compare the exact prior plaintext before publishing `next`.
    /// Any ambiguous outcome is an error and permanently stops this book.
    fn commit(&self, expected: &[u8], next: &[u8]) -> Result<()>;
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    version: u8,
    signers: BTreeSet<String>,
    minimum_signatures: usize,
    sources: Vec<BootstrapSourceV1>,
    current: Vec<PeerSignedRelayRecordV1>,
    tombstones: BTreeSet<[u8; 32]>,
    clock: u64,
}

impl ServiceLeaseBookV1 {
    /// Contains node addresses/history. Keep it encrypted and backup-excluded.
    pub fn checkpoint(&self) -> Result<Vec<u8>> {
        ensure!(!self.failed, "service lease persistence unavailable");
        if matches!(self.policy, ServiceTrustV1::SelfOwned(_)) {
            return self.self_owned_checkpoint();
        }
        encode(
            self.policy.endorsed()?,
            &self.sources,
            &self.entries,
            &self.tombstones,
            self.clock,
        )
    }

    /// Restore expired history too: it prevents a restart from resetting sequence
    /// floors or revocations. routes/check_route still enforce actual wall time.
    /// The trust policy comes from the protected install owner, not this blob.
    pub fn restore(policy: BootstrapTrustPolicyV1, bytes: &[u8], now: u64) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
            "checkpoint size"
        );
        let state: Checkpoint = serde_json::from_slice(bytes)?;
        ensure!(
            state.version == 1
                && state.signers == policy.allowed_signer_peer_ids
                && state.minimum_signatures == policy.minimum_valid_signatures
                && now >= state.clock
                && state.tombstones.len() <= 32
                && !state.sources.is_empty()
                && state.sources.len() <= 4
                && state.current.len() <= 2,
            "checkpoint policy/version/bounds/clock"
        );
        let mut entries = BTreeMap::<[u8; 32], Entry>::new();
        let mut ids = BTreeSet::new();
        for source in &state.sources {
            ensure!(ids.insert(&source.manifest.manifest_id), "duplicate source");
            let at = source
                .manifest
                .relay_records
                .iter()
                .map(|r| r.issued_at_ms)
                .fold(source.manifest.issued_at_ms, u64::max);
            ensure!(at <= state.clock, "future checkpoint source");
            // Reuse all current validation, at its historical admission time.
            let validated = Self::new(policy.clone(), vec![source.clone()], at)?;
            for (pin, entry) in validated.entries {
                ensure!(!state.tombstones.contains(&pin), "revoked checkpoint node");
                if let Some(old) = entries.get_mut(&pin) {
                    ensure!(old.grant == entry.grant, "conflicting checkpoint sources");
                    old.expires = old.expires.max(entry.expires);
                } else {
                    entries.insert(pin, entry);
                }
            }
        }
        ensure!(
            entries.len() <= 2 && entries.len() == state.current.len(),
            "checkpoint inventory"
        );
        let mut seen = BTreeSet::new();
        for record in &state.current {
            ensure!(
                seen.insert(record.relay_public_key),
                "duplicate checkpoint record"
            );
            let entry = entries
                .get_mut(&record.relay_public_key)
                .context("unknown checkpoint node")?;
            ensure!(
                record.issued_at_ms <= state.clock,
                "future checkpoint lease"
            );
            if *record != entry.grant {
                verify(record, record.issued_at_ms, MAX_SERVICE_LEASE_MS)?;
                newer(&entry.grant, record)?;
                ensure!(
                    record.expires_at_ms <= entry.expires,
                    "checkpoint lease exceeds source"
                );
            }
            entry.current = record.clone();
        }
        let result = Self {
            policy: ServiceTrustV1::Endorsed(policy),
            sources: state.sources,
            entries,
            tombstones: state.tombstones,
            clock: state.clock,
            persistence: None,
            persisted: Vec::new(),
            failed: false,
            prior_endorsement: None,
        };
        ensure!(
            result.checkpoint()? == bytes,
            "noncanonical service checkpoint"
        );
        Ok(result)
    }

    /// Attach only after the install owner has durably stored this exact state.
    pub fn attach_persistence(
        &mut self,
        owner: Arc<dyn ServiceLeasePersistenceV1>,
        expected: &[u8],
    ) -> Result<()> {
        ensure!(
            self.persistence.is_none() && self.checkpoint()? == expected,
            "checkpoint attachment differs"
        );
        // Confirms storage exists/authenticates and is still the expected state.
        owner.commit(expected, expected)?;
        self.persisted = expected.to_vec();
        self.persistence = Some(owner);
        Ok(())
    }

    pub(super) fn publish(
        &mut self,
        sources: Vec<BootstrapSourceV1>,
        entries: BTreeMap<[u8; 32], Entry>,
        tombstones: BTreeSet<[u8; 32]>,
    ) -> Result<()> {
        if let Some(owner) = &self.persistence {
            let next = match &self.policy {
                ServiceTrustV1::Endorsed(policy) => {
                    encode(policy, &sources, &entries, &tombstones, self.clock)?
                }
                ServiceTrustV1::SelfOwned(policy) => self_owned::encode_self_owned(
                    policy,
                    &entries,
                    &tombstones,
                    self.clock,
                    &self.prior_endorsement,
                )?,
            };
            if let Err(error) = owner.commit(&self.persisted, &next) {
                self.failed = true;
                return Err(error);
            }
            self.persisted = next;
        }
        self.sources = sources;
        self.entries = entries;
        self.tombstones = tombstones;
        Ok(())
    }
}

fn encode(
    policy: &BootstrapTrustPolicyV1,
    sources: &[BootstrapSourceV1],
    entries: &BTreeMap<[u8; 32], Entry>,
    tombstones: &BTreeSet<[u8; 32]>,
    clock: u64,
) -> Result<Vec<u8>> {
    let result = serde_json::to_vec(&Checkpoint {
        version: 1,
        signers: policy.allowed_signer_peer_ids.clone(),
        minimum_signatures: policy.minimum_valid_signatures,
        sources: sources.to_vec(),
        current: entries.values().map(|e| e.current.clone()).collect(),
        tombstones: tombstones.clone(),
        clock,
    })?;
    ensure!(
        result.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
        "service checkpoint budget"
    );
    Ok(result)
}
