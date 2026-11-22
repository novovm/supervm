//! Private APFL admission and announcements for the original service owner.
use crate::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{IrohAddressV1, IrohScopeV1},
    novorudp::wire_budget::NativeWireBudget,
    product_directory::{validate_relay_record_v1, PeerSignedRelayRecordV1},
    product_overlay::{TransportIdentity, TransportVerifier, MLDSA65_PUBLIC_KEY_BYTES},
    product_service_lease::NativeCarrierMailboxEndpointV8,
    relay::hop::{
        private::{
            OpenedPrivateHop, PrivateHopDeclaration, PrivateHopKey, PrivateHopLink, PrivateHopRole,
            VerifiedPrivateHop,
        },
        HopBudget,
    },
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
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

impl PrivateApflAnnouncement {
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
}

pub struct PrivateApflSuccessor {
    pub identity: Vec<u8>,
    pub declaration: Box<dyn Fn() -> Result<PrivateApflAnnouncement> + Send + Sync>,
}

pub struct PrivateApflIngress {
    scope: [u8; 32],
    predecessors: Vec<Vec<u8>>,
    successor: Option<PrivateApflSuccessor>,
    budget: HopBudget,
    epochs: Mutex<Option<(Arc<PrivateHopKey>, Option<Arc<PrivateHopKey>>)>>,
    publish: Box<dyn Fn(&PrivateApflAnnouncement) -> Result<()> + Send + Sync>,
}

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
                && (1..=16).contains(&predecessors.len())
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
            budget: Arc::new(Mutex::new(NativeWireBudget::new(
                max_wire_bytes_per_minute,
                Instant::now(),
            )?)),
            epochs: Mutex::new(None),
            publish: Box::new(publish),
        })
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
        (self.publish)(&PrivateApflAnnouncement {
            record: record.clone(),
            declaration: current.declaration().encode()?,
        })
    }

    pub(super) fn epochs(&self) -> Result<(Arc<PrivateHopKey>, Option<Arc<PrivateHopKey>>)> {
        self.epochs
            .lock()
            .map_err(|_| anyhow::anyhow!("private APFL key owner unavailable"))?
            .clone()
            .context("private APFL not announced")
    }

    pub(super) async fn accept<'a>(
        &self,
        stream: CarrierStreamV1<'a>,
        key: &TransportIdentity,
        epochs: &(Arc<PrivateHopKey>, Option<Arc<PrivateHopKey>>),
    ) -> Result<(PrivateHopLink<'a>, OpenedPrivateHop)> {
        PrivateHopLink::accept_rotating(
            stream,
            key,
            &epochs.0,
            epochs.1.as_deref(),
            &self.predecessors,
            self.budget.clone(),
        )
        .await
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
