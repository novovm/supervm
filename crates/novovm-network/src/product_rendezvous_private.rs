//! Private APFL admission and announcements for the original service owner.
#[cfg(feature = "private-apfl-listener")]
use crate::{
    bound_session::ServicePeerPolicyV1,
    carrier_stream::CarrierStreamV1,
    iroh_transport::IrohScopeV1,
    novorudp::wire_budget::NativeWireBudget,
    product_overlay::{TransportIdentity, MLDSA65_PUBLIC_KEY_BYTES},
    relay::hop::{
        private::{OpenedPrivateHop, PrivateHopKey, PrivateHopLink},
        HopBudget,
    },
};
use crate::{
    iroh_transport::IrohAddressV1,
    product_directory::{validate_relay_record_v1, PeerSignedRelayRecordV1},
    product_overlay::TransportVerifier,
    product_service_lease::NativeCarrierMailboxEndpointV8,
    relay::hop::private::{PrivateHopDeclaration, PrivateHopRole, VerifiedPrivateHop},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(feature = "private-apfl-listener")]
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateApflAnnouncement {
    pub record: PeerSignedRelayRecordV1,
    pub declaration: Vec<u8>,
}

#[path = "product_rendezvous_private_renewal.rs"]
mod renewal;
pub use renewal::renew_private_apfl_path;
#[cfg(feature = "private-apfl-listener")]
pub(super) use renewal::RENEW;

impl PrivateApflAnnouncement {
    /// Continuity between two statements from an independently pinned owner.
    /// Call verify() separately on the new statement at the current clock.
    /// Locator and hop epoch have separate signing domains and lifetimes.
    pub fn check_renewal(&self, next: &Self) -> Result<()> {
        ensure!(
            self.record.relay_public_key == next.record.relay_public_key
                && next.record.issued_at_ms >= self.record.issued_at_ms
                && (next.record.sequence > self.record.sequence
                    || (next.record == self.record && next.declaration == self.declaration)),
            "private APFL locator rollback or equivocation"
        );
        let old = PrivateHopDeclaration::decode_wire(&self.declaration)?;
        let new = PrivateHopDeclaration::decode_wire(&next.declaration)?;
        ensure!(
            old.scope == new.scope
                && old.role == new.role
                && (new.issued_ms > old.issued_ms || next.declaration == self.declaration),
            "private APFL hop epoch rollback or equivocation"
        );
        Ok(())
    }

    /// Authentic historical address for a fresh pinned metadata handshake only.
    /// Business traffic still requires verify() at the current clock.
    pub fn renewal_address(
        &self,
        pin: &[u8],
        verifier: &dyn TransportVerifier,
    ) -> Result<IrohAddressV1> {
        ensure!(
            self.record.relay_public_key == pin,
            "private APFL renewal pin mismatch"
        );
        crate::product_directory::verify_relay_record_authenticity_v1(&self.record, verifier)?;
        Ok(NativeCarrierMailboxEndpointV8::from_record(&self.record)?
            .context("private APFL renewal needs a signed V8 locator")?
            .iroh
            .address)
    }

    pub fn verify(
        &self,
        identity: &[u8],
        scope: [u8; 32],
        now: u64,
        verifier: &dyn TransportVerifier,
    ) -> Result<(IrohAddressV1, VerifiedPrivateHop)> {
        ensure!(
            self.record.relay_public_key == identity,
            "private APFL identity mismatch"
        );
        validate_relay_record_v1(&self.record, now, verifier)?;
        let endpoint = NativeCarrierMailboxEndpointV8::from_record(&self.record)?
            .context("private APFL requires a signed V8 locator")?;
        let hop =
            PrivateHopDeclaration::verify_wire(&self.declaration, identity, scope, now, verifier)?;
        Ok((endpoint.iroh.address, hop))
    }

    /// Authenticate an APK-approved discovery hint, including its original
    /// role. Even an expired hint can locate the same peer for fresh signed
    /// metadata. This API intentionally cannot yield a VerifiedPrivateHop.
    pub fn verify_bootstrap_hint(
        &self,
        identity: &[u8],
        scope: [u8; 32],
        role: PrivateHopRole,
        now: u64,
        verifier: &dyn TransportVerifier,
    ) -> Result<IrohAddressV1> {
        let hint = self.renewal_address(identity, verifier)?;
        let declaration = PrivateHopDeclaration::decode_wire(&self.declaration)?;
        ensure!(
            self.record.issued_at_ms <= now && declaration.issued_ms <= now,
            "private APFL bootstrap hint issued in the future"
        );
        let hop = declaration.verify(identity, scope, declaration.issued_ms, verifier)?;
        ensure!(hop.role() == role, "private APFL bootstrap role differs");
        Ok(hint)
    }
}

#[cfg(feature = "private-apfl-listener")]
pub struct PrivateApflSuccessor {
    pub identity: Vec<u8>,
    pub declaration: Box<dyn Fn() -> Result<PrivateApflAnnouncement> + Send + Sync>,
    /// Original owner atomically retains only a verified adjacent successor.
    pub persist: Box<dyn Fn(&PrivateApflAnnouncement) -> Result<()> + Send + Sync>,
}

#[cfg(feature = "private-apfl-listener")]
pub struct PrivateApflIngress {
    scope: [u8; 32],
    predecessors: Vec<Vec<u8>>,
    successor: Option<PrivateApflSuccessor>,
    mailbox_admission: Option<super::PrivateMailboxAdmission>,
    origin_admission: Option<super::PrivateMailboxAdmission>,
    budget: HopBudget,
    renewal_budget: Option<HopBudget>,
    last_renewal_failure: Mutex<Option<Instant>>,
    publication: Mutex<Option<PrivateApflAnnouncement>>,
    successor_update: Mutex<()>,
    epochs: Mutex<Option<(Arc<PrivateHopKey>, Option<Arc<PrivateHopKey>>)>>,
    publish: Box<dyn Fn(&PrivateApflAnnouncement) -> Result<()> + Send + Sync>,
}

#[cfg(feature = "private-apfl-listener")]
impl PrivateApflIngress {
    pub fn new(
        scope: [u8; 32],
        predecessors: BTreeSet<Vec<u8>>,
        successor: Option<PrivateApflSuccessor>,
        max_wire_bytes_per_minute: u64,
        publish: impl Fn(&PrivateApflAnnouncement) -> Result<()> + Send + Sync + 'static,
    ) -> Result<Self> {
        let valid_pin =
            |key: &[u8]| key.len() == MLDSA65_PUBLIC_KEY_BYTES && key.iter().any(|b| *b != 0);
        ensure!(
            scope != [0; 32]
                && predecessors.len() <= 16
                && predecessors.iter().all(|key| valid_pin(key))
                && successor.as_ref().is_none_or(|pin| {
                    valid_pin(&pin.identity) && !predecessors.contains(&pin.identity)
                }),
            "invalid private APFL ingress policy"
        );
        Ok(Self {
            scope,
            predecessors: predecessors.into_iter().collect(),
            successor,
            mailbox_admission: None,
            origin_admission: None,
            renewal_budget: None,
            last_renewal_failure: Mutex::new(None),
            budget: Arc::new(Mutex::new(NativeWireBudget::new(
                max_wire_bytes_per_minute,
                Instant::now(),
            )?)),
            epochs: Mutex::new(None),
            publication: Mutex::new(None),
            successor_update: Mutex::new(()),
            publish: Box::new(publish),
        })
    }

    pub fn with_autonomous_mailbox(
        mut self,
        admission: super::PrivateMailboxAdmission,
    ) -> Result<Self> {
        ensure!(
            self.successor.is_none(),
            "autonomous mailbox admission belongs to the terminal"
        );
        self.mailbox_admission = Some(admission);
        Ok(self)
    }

    pub fn with_autonomous_origin(
        mut self,
        admission: super::PrivateMailboxAdmission,
        renewal_bytes_per_minute: u64,
    ) -> Result<Self> {
        ensure!(
            self.successor.is_some(),
            "autonomous origin requires a forwarding owner"
        );
        let now = Instant::now();
        let renewal = NativeWireBudget::new(renewal_bytes_per_minute, now)?;
        {
            let mut budget = self
                .budget
                .lock()
                .map_err(|_| anyhow::anyhow!("APFL byte budget unavailable"))?;
            let business = budget
                .usage(now)
                .limit
                .checked_sub(renewal_bytes_per_minute)
                .context("APFL renewal share exceeds owner byte budget")?;
            budget.set_limit(business, now)?;
        }
        self.origin_admission = Some(admission);
        self.renewal_budget = Some(Arc::new(Mutex::new(renewal)));
        Ok(self)
    }

    fn adjacent_policy(&self) -> ServicePeerPolicyV1<'_> {
        if self.origin_admission.is_some() {
            ServicePeerPolicyV1::AnyAuthenticated
        } else {
            ServicePeerPolicyV1::Pinned(&self.predecessors)
        }
    }

    pub(super) fn mailbox_admission(&self) -> Option<&super::PrivateMailboxAdmission> {
        self.mailbox_admission.as_ref()
    }

    pub(super) fn publish_for(
        &self,
        key: &TransportIdentity,
        record: &PeerSignedRelayRecordV1,
        now: u64,
    ) -> Result<()> {
        ensure!(
            record.relay_public_key == key.public_key()
                && NativeCarrierMailboxEndpointV8::from_record(record)?.is_some()
                && (!self.predecessors.is_empty() || self.origin_admission.is_some())
                && !self.predecessors.iter().any(|pin| pin == key.public_key())
                && self
                    .successor
                    .as_ref()
                    .is_none_or(|pin| pin.identity != key.public_key()),
            "private APFL requires the original active V8 service and distinct adjacent pins"
        );
        let mut epochs = self
            .epochs
            .lock()
            .map_err(|_| anyhow::anyhow!("private APFL key owner unavailable"))?;
        if epochs.as_ref().is_none_or(|(current, _)| {
            current.declaration().expires_ms.saturating_sub(now) <= 120_000
        }) {
            let role = if self.successor.is_some() {
                PrivateHopRole::Forwarder
            } else {
                PrivateHopRole::Terminal
            };
            let current = Arc::new(PrivateHopKey::issue(key, self.scope, role, now)?);
            let previous = epochs.take().map(|(key, _)| key);
            *epochs = Some((current, previous));
        }
        let (current, _) = epochs.as_ref().context("private APFL key epoch missing")?;
        let active = PrivateApflAnnouncement {
            record: record.clone(),
            declaration: current.declaration().encode()?,
        };
        (self.publish)(&active)?;
        *self
            .publication
            .lock()
            .map_err(|_| anyhow::anyhow!("private APFL publication unavailable"))? = Some(active);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn observed_wire_budget(&self) -> HopBudget {
        self.budget.clone()
    }

    pub(super) fn epochs(&self) -> Result<(Arc<PrivateHopKey>, Option<Arc<PrivateHopKey>>)> {
        self.epochs
            .lock()
            .map_err(|_| anyhow::anyhow!("private APFL key owner unavailable"))?
            .clone()
            .context("private APFL not announced")
    }

    pub(super) async fn accept<'a, 'owner>(
        &'owner self,
        stream: CarrierStreamV1<'a>,
        key: &TransportIdentity,
        epochs: &(Arc<PrivateHopKey>, Option<Arc<PrivateHopKey>>),
    ) -> Result<(
        PrivateHopLink<'a>,
        OpenedPrivateHop,
        Option<super::admission::PrivateMailboxGrant<'owner>>,
    )> {
        if let Some(owner) = &self.origin_admission {
            owner.authenticate_at(Instant::now())?;
        }
        let (link, opened) = PrivateHopLink::accept_rotating(
            stream,
            key,
            &epochs.0,
            epochs.1.as_deref(),
            self.adjacent_policy(),
            self.budget.clone(),
        )
        .await?;
        let grant = self
            .origin_admission
            .as_ref()
            .map(|owner| {
                let now = Instant::now();
                let grant = owner.grant_at(link.authenticated_peer(), now)?;
                grant.charge_at(1, now)?;
                Ok::<_, anyhow::Error>(grant)
            })
            .transpose()?;
        Ok((link, opened, grant))
    }

    pub(super) async fn connect<'a>(
        &self,
        scope: &'a IrohScopeV1,
        key: &TransportIdentity,
        next: &[u8],
        sealed: &[u8],
        now: u64,
    ) -> Result<PrivateHopLink<'a>> {
        let successor = self
            .successor
            .as_ref()
            .context("private APFL terminal cannot forward")?;
        ensure!(
            successor.identity == next,
            "private APFL successor outside owner policy"
        );
        let (address, _) = (successor.declaration)()?.verify(next, self.scope, now, key)?;
        PrivateHopLink::connect(
            scope.connect_private_successor(&address).await?.into(),
            key,
            next,
            sealed,
            Some(self.budget.clone()),
        )
        .await
    }
}
