//! Local, explicit key trust; not a self-issued publisher endorsement. The
//! existing V2 offer, service handshake, online lease and durable CAS are reused.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfOwnedTrustV1 {
    pub version: u8,
    /// Confirmed out of band by the receiving owner. Never inferred from an
    /// unsolicited record or copied automatically from an old endorsement.
    pub node_pins: BTreeSet<Vec<u8>>,
}
impl SelfOwnedTrustV1 {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 2 && (1..=2).contains(&self.node_pins.len()),
            "self-owned trust bounds"
        );
        for pin in &self.node_pins {
            ensure!(
                pin.len() == MLDSA65_PUBLIC_KEY_BYTES && pin.iter().any(|b| *b != 0),
                "invalid native service key"
            );
        }
        Ok(())
    }
}

pub struct ServiceBootstrapCandidateV1 {
    pub public_key: Vec<u8>,
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
    tombstones: BTreeSet<Vec<u8>>,
    clock: u64,
    /// Retain the original signed source/revocation history on explicit migration.
    prior_endorsement: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    retired: Vec<RetiredServiceRecords>,
}

/// Historical signed records only. Routing and service authorization do not
/// consult this archive.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetiredServiceRecords {
    grant: PeerSignedRelayRecordV1,
    current: PeerSignedRelayRecordV1,
}

impl ServiceLeaseBookV1 {
    /// NVMBOX04 live connection policy. This checks LOCAL pin ownership and
    /// revocation, not the freshness of an opaque rendezvous locator. Callers
    /// must still verify fresh Finished on the actual pinned carrier; this
    /// method alone authenticates no peer and grants no mailbox capability.
    /// Never available to the lease-bound, Publisher-endorsed policy.
    pub fn check_continuous_peer_v4(
        &mut self,
        pin: &[u8],
        endpoint: [u8; 32],
        now: u64,
    ) -> Result<()> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "continuous peer requires local trust");
        let entry = self.entries.get_mut(pin).context("service revoked")?;
        ensure!(
            entry
                .decoded_current()?
                .routes
                .iroh
                .as_ref()
                .context("Iroh service endpoint required")?
                .address
                .endpoint_key
                == endpoint,
            "self-owned transport changed"
        );
        Ok(())
    }

    /// Check the stable locator for a NEW handshake. Store instances may rotate
    /// behind that same pinned transport. Only accept_renewal authorizes the
    /// offered instance, after signature, freshness and sequence validation.
    pub fn check_bootstrap_route(
        &mut self,
        pin: &[u8],
        endpoint: [u8; 32],
        instance: [u8; 32],
        now: u64,
    ) -> Result<()> {
        if !self.is_self_owned() {
            return self.check_route(pin, endpoint, instance, now);
        }
        self.observe(now)?;
        let entry = self.entries.get_mut(pin).context("service revoked")?;
        ensure!(
            entry
                .decoded_current()?
                .routes
                .iroh
                .as_ref()
                .context("Iroh service endpoint required")?
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

    /// The local policy owner is live. This does not require a fresh remote
    /// service declaration or extend any service/capability lifetime.
    pub fn check_self_owned_owner(&self) -> Result<()> {
        ensure!(
            !self.failed && self.is_self_owned(),
            "self-owned policy owner retired or unavailable"
        );
        Ok(())
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
            verify_service_record(&record, now, MAX_BOOTSTRAP_LEASE_MS, &self.verifier)?;
            ensure!(
                seen.insert(record.relay_public_key.clone()),
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
        verifier: impl TransportVerifier + 'static,
    ) -> Result<Self> {
        Self::initialize_self_owned(trust, records, now, verifier, false)
    }

    /// Only for a trusted distribution's initial dial hints. Historic inputs
    /// authenticate a new service offer, never extend an old lease.
    pub fn new_self_owned_bootstrap(
        trust: SelfOwnedTrustV1,
        records: Vec<PeerSignedRelayRecordV1>,
        now: u64,
        verifier: impl TransportVerifier + 'static,
    ) -> Result<Self> {
        Self::initialize_self_owned(trust, records, now, verifier, true)
    }

    fn initialize_self_owned(
        trust: SelfOwnedTrustV1,
        records: Vec<PeerSignedRelayRecordV1>,
        now: u64,
        verifier: impl TransportVerifier + 'static,
        historical: bool,
    ) -> Result<Self> {
        trust.validate()?;
        ensure!(
            records.len() == trust.node_pins.len(),
            "self-owned inventory"
        );
        let mut entries = BTreeMap::new();
        for record in records {
            if historical {
                ensure!(
                    record.issued_at_ms <= now,
                    "self-owned bootstrap from the future"
                );
            }
            let verify_at = if historical { record.issued_at_ms } else { now };
            verify_service_record(&record, verify_at, MAX_BOOTSTRAP_LEASE_MS, &verifier)?;
            ensure!(
                trust.node_pins.contains(&record.relay_public_key),
                "unconfirmed self-owned node"
            );
            let pin = record.relay_public_key.clone();
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
            verifier: Arc::new(verifier),
            policy: ServiceTrustV1::SelfOwned(trust),
            sources: Vec::new(),
            entries,
            tombstones: BTreeSet::new(),
            clock: now,
            persistence: None,
            persisted: Vec::new(),
            failed: false,
            prior_endorsement: None,
            retired: Vec::new(),
        })
    }

    /// These are only dial candidates. A historical locator can locate the same
    /// pinned transport key after its short claim expires, but cannot authorize
    /// service: every new connection must verify a NEW V2 offer and node handshake.
    /// This does not mutate/re-sign an expired record or extend an old stream.
    pub fn bootstrap_candidates(&mut self, now: u64) -> Result<Vec<ServiceBootstrapCandidateV1>> {
        self.observe(now)?;
        if self.is_self_owned() {
            let mut candidates = Vec::new();
            for entry in self.entries.values_mut() {
                let pin = entry.current.relay_public_key.clone();
                if let Some(endpoint) = &entry.decoded_current()?.routes.iroh {
                    candidates.push(ServiceBootstrapCandidateV1 {
                        public_key: pin,
                        endpoint: endpoint.clone(),
                        authenticate_before_ms: now
                            .checked_add(MAX_SERVICE_LEASE_MS)
                            .context("local clock exhausted")?,
                    });
                }
            }
            return Ok(candidates);
        }
        self.bootstrap_routes(now)?
            .iter()
            .map(|record| {
                Ok(ServiceBootstrapCandidateV1 {
                    public_key: record.relay_public_key.clone(),
                    endpoint: IrohServiceEndpointV1::from_record(record)?,
                    authenticate_before_ms: record.expires_at_ms,
                })
            })
            .collect()
    }

    /// Operator/user removes only this local trust. It cannot revoke the node
    /// for other owners. A revoked key cannot resurrect by replay or restart.
    pub fn revoke_self_owned(&mut self, pin: &[u8], now: u64) -> Result<()> {
        self.observe(now)?;
        ensure!(self.is_self_owned(), "self-owned policy required");
        ensure!(
            self.entries.contains_key(pin) || self.tombstones.contains(pin),
            "unknown self-owned node"
        );
        let mut entries = self.entries.clone();
        entries.remove(pin);
        let mut tombstones = self.tombstones.clone();
        tombstones.insert(pin.to_vec());
        ensure!(tombstones.len() <= 32, "revocation budget");
        self.publish(Vec::new(), entries, tombstones)
    }

    /// Explicit local policy migration, not online fallback. Produces an unattached
    /// candidate for an already self-owned book. Existing trust and history are
    /// never removed; transport preferences live in the platform's same atomic
    /// policy/checkpoint commit. Only an explicit local owner action may call it.
    pub fn prepare_self_owned_policy_update(
        &self,
        trust: SelfOwnedTrustV1,
        records: Vec<PeerSignedRelayRecordV1>,
        now: u64,
    ) -> Result<Self> {
        self.prepare_self_owned_policy_change(trust, records, &[], now)
    }

    /// Explicit local owner action to replace pinned keys. Archive the old
    /// signed records and tombstone those keys in the same checkpoint.
    pub fn prepare_self_owned_policy_rotation(
        &self,
        trust: SelfOwnedTrustV1,
        records: Vec<PeerSignedRelayRecordV1>,
        retiring: &[Vec<u8>],
        now: u64,
    ) -> Result<Self> {
        ensure!(
            !retiring.is_empty(),
            "rotation requires explicit retirement"
        );
        self.prepare_self_owned_policy_change(trust, records, retiring, now)
    }

    fn prepare_self_owned_policy_change(
        &self,
        trust: SelfOwnedTrustV1,
        records: Vec<PeerSignedRelayRecordV1>,
        retiring: &[Vec<u8>],
        now: u64,
    ) -> Result<Self> {
        ensure!(!self.failed && now >= self.clock, "invalid policy update");
        let ServiceTrustV1::SelfOwned(previous) = &self.policy else {
            anyhow::bail!("self-owned policy required")
        };
        trust.validate()?;
        let removed: BTreeSet<_> = previous.node_pins.difference(&trust.node_pins).collect();
        let requested: BTreeSet<_> = retiring.iter().collect();
        ensure!(
            removed == requested && retiring.len() == requested.len(),
            "policy update must explicitly name every retired key"
        );
        let mut added = trust.node_pins.difference(&previous.node_pins).peekable();
        ensure!(
            removed.is_empty() || added.peek().is_some(),
            "rotation requires a replacement key"
        );
        ensure!(
            added.all(|pin| !self.tombstones.contains(pin)),
            "retired or revoked key cannot be installed again"
        );
        let mut entries = self.entries.clone();
        let mut tombstones = self.tombstones.clone();
        let mut retired = self.retired.clone();
        for pin in removed {
            if let Some(entry) = entries.remove(pin) {
                retired.push(RetiredServiceRecords {
                    grant: entry.grant,
                    current: entry.current,
                });
            }
            tombstones.insert(pin.clone());
        }
        ensure!(tombstones.len() <= 32, "revocation budget");
        let mut seen = BTreeSet::new();
        for record in records {
            let pin = record.relay_public_key.clone();
            ensure!(
                trust.node_pins.contains(&pin)
                    && !self.tombstones.contains(&pin)
                    && seen.insert(pin.clone()),
                "untrusted, revoked or duplicate policy record"
            );
            verify_service_record(&record, now, MAX_BOOTSTRAP_LEASE_MS, &self.verifier)?;
            if let Some(previous) = entries.get(&pin) {
                newer(&previous.current, &record)?;
            }
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
        ensure!(
            trust
                .node_pins
                .iter()
                .all(|pin| entries.contains_key(pin) || self.tombstones.contains(pin)),
            "new node requires a signed record"
        );
        let next = Self {
            verifier: self.verifier.clone(),
            policy: ServiceTrustV1::SelfOwned(trust),
            sources: Vec::new(),
            entries,
            tombstones,
            clock: now,
            persistence: None,
            persisted: Vec::new(),
            failed: false,
            prior_endorsement: self.prior_endorsement.clone(),
            retired,
        };
        next.checkpoint()?;
        Ok(next)
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
        let mut next = Self::new_self_owned(trust, records, now, self.verifier.clone())?;
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
            &self.retired,
        )
    }

    pub fn restore_self_owned(
        trust: SelfOwnedTrustV1,
        bytes: &[u8],
        now: u64,
        verifier: impl TransportVerifier + 'static,
    ) -> Result<Self> {
        trust.validate()?;
        ensure!(
            bytes.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
            "checkpoint size"
        );
        let state: State = serde_json::from_slice(bytes)?;
        ensure!(
            state.version == 3
                && state.trust == trust
                && now >= state.clock
                && state.tombstones.len() <= 32
                && state.retired.len() <= state.tombstones.len()
                && state.grants.len() <= 2
                && state.grants.len() == state.current.len(),
            "self-owned checkpoint policy/bounds/clock"
        );
        let mut entries = BTreeMap::new();
        for (grant, current) in state.grants.iter().zip(&state.current) {
            ensure!(
                trust.node_pins.contains(&grant.relay_public_key)
                    && !state.tombstones.contains(&grant.relay_public_key),
                "self-owned checkpoint active identity"
            );
            verify_checkpoint_records(grant, current, state.clock, &verifier)?;
            ensure!(
                entries
                    .insert(
                        grant.relay_public_key.clone(),
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
        let mut retired_pins = BTreeSet::new();
        for record in &state.retired {
            let pin = &record.grant.relay_public_key;
            ensure!(
                state.tombstones.contains(pin)
                    && !trust.node_pins.contains(pin)
                    && retired_pins.insert(pin),
                "self-owned checkpoint retired identity"
            );
            verify_checkpoint_records(&record.grant, &record.current, state.clock, &verifier)?;
        }
        ensure!(
            trust
                .node_pins
                .iter()
                .all(|p| entries.contains_key(p) || state.tombstones.contains(p)),
            "missing self-owned history"
        );
        let book = Self {
            verifier: Arc::new(verifier),
            policy: ServiceTrustV1::SelfOwned(trust),
            sources: Vec::new(),
            entries,
            tombstones: state.tombstones,
            clock: state.clock,
            persistence: None,
            persisted: Vec::new(),
            failed: false,
            prior_endorsement: state.prior_endorsement,
            retired: state.retired,
        };
        ensure!(
            book.checkpoint()? == bytes,
            "noncanonical self-owned checkpoint"
        );
        Ok(book)
    }
}

fn verify_checkpoint_records(
    grant: &PeerSignedRelayRecordV1,
    current: &PeerSignedRelayRecordV1,
    clock: u64,
    verifier: &dyn TransportVerifier,
) -> Result<()> {
    ensure!(
        current.relay_public_key == grant.relay_public_key
            && grant.issued_at_ms <= clock
            && current.issued_at_ms <= clock,
        "self-owned checkpoint identity/clock"
    );
    let old = verify_service_record(grant, grant.issued_at_ms, MAX_BOOTSTRAP_LEASE_MS, verifier)?;
    if current != grant {
        // Different grant/current is the legacy Iroh online-renewal shape.
        let old = old.iroh.context("Iroh service endpoint required")?;
        let live = verify(
            current,
            current.issued_at_ms,
            MAX_SERVICE_LEASE_MS,
            verifier,
        )?;
        ensure!(
            old.address.endpoint_key == live.address.endpoint_key && old.instance == live.instance,
            "self-owned checkpoint route changed"
        );
        newer(grant, current)?;
    }
    Ok(())
}

pub(super) fn encode_self_owned(
    trust: &SelfOwnedTrustV1,
    entries: &BTreeMap<Vec<u8>, Entry>,
    tombstones: &BTreeSet<Vec<u8>>,
    clock: u64,
    prior_endorsement: &Option<String>,
    retired: &[RetiredServiceRecords],
) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(&State {
        version: 3,
        trust: trust.clone(),
        grants: entries.values().map(|e| e.grant.clone()).collect(),
        current: entries.values().map(|e| e.current.clone()).collect(),
        tombstones: tombstones.clone(),
        clock,
        prior_endorsement: prior_endorsement.clone(),
        retired: retired.to_vec(),
    })?;
    ensure!(
        bytes.len() <= MAX_SERVICE_CHECKPOINT_BYTES,
        "service checkpoint budget"
    );
    Ok(bytes)
}
