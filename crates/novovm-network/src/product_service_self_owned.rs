//! Local, explicit key trust; not a self-issued publisher endorsement. The
//! existing V2 offer, service handshake, online lease and durable CAS are reused.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfOwnedTrustV1 {
    pub version: u8,
    /// Confirmed out of band by the receiving owner. Never inferred from an
    /// unsolicited record or copied automatically from an old endorsement.
    pub node_pins: BTreeSet<[u8; 32]>,
}
impl SelfOwnedTrustV1 {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && (1..=2).contains(&self.node_pins.len()),
            "self-owned trust bounds"
        );
        for pin in &self.node_pins {
            strict_key(pin)?;
        }
        Ok(())
    }
}

pub struct ServiceBootstrapCandidateV1 {
    pub public_key: [u8; 32],
    pub endpoint: IrohServiceEndpointV1,
    /// Local permission to attempt a fresh authenticated offer. Not the expiry
    /// of the historical signed record, nor permission for application IO.
    pub authenticate_before_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u8,
    trust: SelfOwnedTrustV1,
    grants: Vec<PeerSignedRelayRecordV1>,
    current: Vec<PeerSignedRelayRecordV1>,
    tombstones: BTreeSet<[u8; 32]>,
    clock: u64,
    /// Retain the original signed source/revocation history on explicit migration.
    prior_endorsement: Option<String>,
}

impl ServiceLeaseBookV1 {
    /// NVMBOX04 live connection policy. This checks LOCAL pin ownership and
    /// revocation, not the freshness of an opaque rendezvous locator. Callers
    /// must still verify fresh Finished on the actual pinned carrier; this
    /// method alone authenticates no peer and grants no mailbox capability.
    /// Never available to the lease-bound, Publisher-endorsed policy.
    pub fn check_continuous_peer_v4(
        &mut self,
        pin: [u8; 32],
        endpoint: [u8; 32],
        now: u64,
    ) -> Result<()> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "continuous peer requires local trust");
        let entry = self.entries.get_mut(&pin).context("service revoked")?;
        ensure!(
            entry.decoded_current()?.iroh.address.endpoint_key == endpoint,
            "self-owned transport changed"
        );
        Ok(())
    }

    /// Check the stable locator for a NEW handshake. Store instances may rotate
    /// behind that same pinned transport. Only accept_renewal authorizes the
    /// offered instance, after signature, freshness and sequence validation.
    pub fn check_bootstrap_route(
        &mut self,
        pin: [u8; 32],
        endpoint: [u8; 32],
        instance: [u8; 32],
        now: u64,
    ) -> Result<()> {
        if !self.is_self_owned() {
            return self.check_route(pin, endpoint, instance, now);
        }
        self.observe(now)?;
        let entry = self.entries.get(&pin).context("service revoked")?;
        ensure!(
            IrohServiceEndpointV1::from_record(&entry.current)?
                .address
                .endpoint_key
                == endpoint,
            "self-owned transport changed"
        );
        Ok(())
    }
    pub fn durable_checkpoint(&self) -> Result<Vec<u8>> {
        ensure!(
            !self.failed && self.persistence.is_some(),
            "durable lease owner required"
        );
        Ok(self.persisted.clone())
    }

    /// Retire the old policy owner after an attempted atomic policy change.
    /// Any uncertain commit requires a protected reopen, never old-policy IO.
    pub fn retire_for_policy_change(&mut self) {
        self.failed = true;
    }

    pub fn is_self_owned(&self) -> bool {
        matches!(self.policy, ServiceTrustV1::SelfOwned(_))
    }

    pub fn self_owned_records(&self) -> Result<Vec<PeerSignedRelayRecordV1>> {
        ensure!(
            !self.failed && self.is_self_owned(),
            "self-owned policy required"
        );
        Ok(self.entries.values().map(|e| e.current.clone()).collect())
    }

    /// Fresh node-signed locator update under the SAME local pins. No online
    /// source can insert a key or undo a tombstone by omission/reintroduction.
    pub fn replace_self_owned_records(
        &mut self,
        records: Vec<PeerSignedRelayRecordV1>,
        now: u64,
    ) -> Result<()> {
        self.observe(now)?;
        ensure!(
            self.is_self_owned() && records.len() == self.entries.len(),
            "self-owned update inventory"
        );
        let mut entries = self.entries.clone();
        let mut seen = BTreeSet::new();
        for record in records {
            verify(&record, now, MAX_BOOTSTRAP_LEASE_MS)?;
            ensure!(
                seen.insert(record.relay_public_key),
                "duplicate self-owned update"
            );
            let entry = entries
                .get_mut(&record.relay_public_key)
                .context("untrusted or revoked self-owned node")?;
            newer(&entry.current, &record)?;
            entry.grant = record.clone();
            entry.current = record;
        }
        self.publish(Vec::new(), entries, self.tombstones.clone())
    }

    pub fn new_self_owned(
        trust: SelfOwnedTrustV1,
        records: Vec<PeerSignedRelayRecordV1>,
        now: u64,
    ) -> Result<Self> {
        trust.validate()?;
        ensure!(
            records.len() == trust.node_pins.len(),
            "self-owned inventory"
        );
        let mut entries = BTreeMap::new();
        for record in records {
            verify(&record, now, MAX_BOOTSTRAP_LEASE_MS)?;
            ensure!(
                trust.node_pins.contains(&record.relay_public_key),
                "unconfirmed self-owned node"
            );
            let pin = record.relay_public_key;
            ensure!(!entries.contains_key(&pin), "duplicate self-owned node");
            entries.insert(
                pin,
                Entry {
                    grant: record.clone(),
                    current: record,
                    expires: u64::MAX,
                    decoded: None,
                },
            );
        }
        Ok(Self {
            policy: ServiceTrustV1::SelfOwned(trust),
            sources: Vec::new(),
            entries,
            tombstones: BTreeSet::new(),
            clock: now,
            persistence: None,
            persisted: Vec::new(),
            failed: false,
            prior_endorsement: None,
        })
    }

    /// These are only dial candidates. A historical locator can locate the same
    /// pinned transport key after its short claim expires, but cannot authorize
    /// service: every new connection must verify a NEW V2 offer and node handshake.
    /// This does not mutate/re-sign an expired record or extend an old stream.
    pub fn bootstrap_candidates(&mut self, now: u64) -> Result<Vec<ServiceBootstrapCandidateV1>> {
        self.observe(now)?;
        if self.is_self_owned() {
            return self
                .entries
                .values()
                .map(|entry| {
                    Ok(ServiceBootstrapCandidateV1 {
                        public_key: entry.current.relay_public_key,
                        endpoint: IrohServiceEndpointV1::from_record(&entry.current)?,
                        authenticate_before_ms: now
                            .checked_add(MAX_SERVICE_LEASE_MS)
                            .context("local clock exhausted")?,
                    })
                })
                .collect();
        }
        self.bootstrap_routes(now)?
            .iter()
            .map(|record| {
                Ok(ServiceBootstrapCandidateV1 {
                    public_key: record.relay_public_key,
                    endpoint: IrohServiceEndpointV1::from_record(record)?,
                    authenticate_before_ms: record.expires_at_ms,
                })
            })
            .collect()
    }

    /// Operator/user removes only this local trust. It cannot revoke the node
    /// for other owners. A revoked key cannot resurrect by replay or restart.
    pub fn revoke_self_owned(&mut self, pin: [u8; 32], now: u64) -> Result<()> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "self-owned policy required");
        ensure!(
            self.entries.contains_key(&pin) || self.tombstones.contains(&pin),
            "unknown self-owned node"
        );
        let mut entries = self.entries.clone();
        entries.remove(&pin);
        let mut tombstones = self.tombstones.clone();
        tombstones.insert(pin);
        ensure!(tombstones.len() <= 32, "revocation budget");
        self.publish(Vec::new(), entries, tombstones)
    }

    /// Explicit local policy migration, not online fallback. Produces an unattached
    /// candidate; the platform atomically commits BOTH profile and checkpoint.
    /// Existing pins, sequence floors and tombstones are preserved. No chat data.
    pub fn prepare_self_owned_migration(
        &self,
        trust: SelfOwnedTrustV1,
        records: Vec<PeerSignedRelayRecordV1>,
        now: u64,
    ) -> Result<Self> {
        ensure!(
            !self.failed && !self.is_self_owned() && now >= self.clock,
            "invalid trust migration"
        );
        let mut next = Self::new_self_owned(trust, records, now)?;
        for (pin, entry) in &next.entries {
            ensure!(!self.tombstones.contains(pin), "revoked service identity");
            let previous = self
                .entries
                .get(pin)
                .context("migration cannot replace node identity")?;
            newer(&previous.current, &entry.grant)?;
        }
        next.tombstones = self.tombstones.clone();
        next.prior_endorsement = Some(String::from_utf8(self.checkpoint()?)?);
        next.checkpoint()?; // Fail bounds before any storage mutation.
        Ok(next)
    }

    pub(super) fn self_owned_checkpoint(&self) -> Result<Vec<u8>> {
        let ServiceTrustV1::SelfOwned(trust) = &self.policy else {
            anyhow::bail!("self-owned policy required")
        };
        encode_self_owned(
            trust,
            &self.entries,
            &self.tombstones,
            self.clock,
            &self.prior_endorsement,
        )
    }

    pub fn restore_self_owned(trust: SelfOwnedTrustV1, bytes: &[u8], now: u64) -> Result<Self> {
        trust.validate()?;
        ensure!(
            bytes.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
            "checkpoint size"
        );
        let state: State = serde_json::from_slice(bytes)?;
        ensure!(
            state.version == 2
                && state.trust == trust
                && now >= state.clock
                && state.tombstones.len() <= 32
                && state.grants.len() <= 2
                && state.grants.len() == state.current.len(),
            "self-owned checkpoint policy/bounds/clock"
        );
        let mut entries = BTreeMap::new();
        for (grant, current) in state.grants.iter().zip(&state.current) {
            ensure!(
                trust.node_pins.contains(&grant.relay_public_key)
                    && !state.tombstones.contains(&grant.relay_public_key)
                    && current.relay_public_key == grant.relay_public_key
                    && grant.issued_at_ms <= state.clock
                    && current.issued_at_ms <= state.clock,
                "self-owned checkpoint identity/clock"
            );
            let old = verify(grant, grant.issued_at_ms, MAX_BOOTSTRAP_LEASE_MS)?;
            if current != grant {
                let live = verify(current, current.issued_at_ms, MAX_SERVICE_LEASE_MS)?;
                ensure!(
                    old.address.endpoint_key == live.address.endpoint_key
                        && old.instance == live.instance,
                    "self-owned checkpoint route changed"
                );
                newer(grant, current)?;
            }
            ensure!(
                entries
                    .insert(
                        grant.relay_public_key,
                        Entry {
                            grant: grant.clone(),
                            current: current.clone(),
                            expires: u64::MAX,
                            decoded: None,
                        }
                    )
                    .is_none(),
                "duplicate self-owned checkpoint"
            );
        }
        ensure!(
            trust
                .node_pins
                .iter()
                .all(|p| entries.contains_key(p) || state.tombstones.contains(p)),
            "missing self-owned history"
        );
        let book = Self {
            policy: ServiceTrustV1::SelfOwned(trust),
            sources: Vec::new(),
            entries,
            tombstones: state.tombstones,
            clock: state.clock,
            persistence: None,
            persisted: Vec::new(),
            failed: false,
            prior_endorsement: state.prior_endorsement,
        };
        ensure!(
            book.checkpoint()? == bytes,
            "noncanonical self-owned checkpoint"
        );
        Ok(book)
    }
}

pub(super) fn encode_self_owned(
    trust: &SelfOwnedTrustV1,
    entries: &BTreeMap<[u8; 32], Entry>,
    tombstones: &BTreeSet<[u8; 32]>,
    clock: u64,
    prior_endorsement: &Option<String>,
) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(&State {
        version: 2,
        trust: trust.clone(),
        grants: entries.values().map(|e| e.grant.clone()).collect(),
        current: entries.values().map(|e| e.current.clone()).collect(),
        tombstones: tombstones.clone(),
        clock,
        prior_endorsement: prior_endorsement.clone(),
    })?;
    ensure!(
        bytes.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
        "service checkpoint budget"
    );
    Ok(bytes)
}
