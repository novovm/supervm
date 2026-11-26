//! An authenticated introducer may name a public reader, but cannot choose
//! the client's IP destinations. Only release-pinned peers use the usual scope.
use super::*;
use novovm_network::discovery_control::{native_locator_id_v1, public_read_locator_id_v1};

/// Build the nested carrier or query future directly across a short-lived
/// stack frame. Its caller retains only a pointer across polls. This matters
/// on the default Windows executable stack at low optimization levels.
#[inline(never)]
pub(super) fn boxed_read<F: std::future::Future>(
    create: impl FnOnce() -> F,
) -> std::pin::Pin<Box<F>> {
    Box::pin(create())
}

pub(super) struct ReadRoute {
    address: IrohAddressV1,
    relay_only: Option<IrohConfigV1>,
    expires_at_ms: u64,
}

impl ReadRoute {
    pub(super) fn new(record: &PeerSignedRelayRecordV1, profile: &Profile) -> Result<Self> {
        let mut address = IrohServiceEndpointV1::from_record(record)?.address;
        let relay_only = if profile
            .lookup_transport_keys
            .contains(&record.relay_public_key)
        {
            ensure!(
                record.record_id == native_locator_id_v1(profile.pins()),
                "fixed read locator domain differs"
            );
            None
        } else {
            ensure!(
                profile.public_read_referrals()
                    && record.record_id == public_read_locator_id_v1(profile.pins()),
                "read service lacks a signed public referral"
            );
            address.direct.clear();
            address
                .relays
                .retain(|relay| approved_read_relay(&profile.relays, relay));
            ensure!(
                !address.relays.is_empty(),
                "public read service has no release-approved relay"
            );
            Some(IrohConfigV1::new(&profile.relays, profile.nameserver)?.with_relay_only()?)
        };
        Ok(Self {
            address,
            relay_only,
            expires_at_ms: record.expires_at_ms,
        })
    }

    pub(super) async fn run<T>(
        self,
        fixed_scope: &IrohScopeV1,
        budget: Option<Duration>,
        operation: impl AsyncFnOnce(&IrohScopeV1, IrohAddressV1) -> Result<T>,
    ) -> Result<T> {
        let remaining = self.expires_at_ms.saturating_sub(wall_ms()?);
        ensure!(remaining > 0, "installed read locator expired before dial");
        let Some(config) = self.relay_only else {
            let future = boxed_read(|| operation(fixed_scope, self.address));
            return match budget {
                Some(limit) => tokio::time::timeout(limit, future).await?,
                None => future.await,
            };
        };
        // Dropping direct hints on a normal Iroh endpoint still permits a
        // hole-punch upgrade. The restricted owner has no IP transport at all.
        let lease = Duration::from_millis(remaining);
        let deadline = Instant::now() + budget.unwrap_or(lease).min(lease);
        let owner =
            tokio::time::timeout_at(deadline.into(), boxed_read(|| bind_carrier(config))).await??;
        let result = boxed_read(|| {
            owner.with_relay_only_service_v1(async |scope| {
                let query = boxed_read(|| operation(scope, self.address));
                tokio::time::timeout_at(deadline.into(), query).await?
            })
        })
        .await;
        let closed = boxed_read(|| owner.close()).await;
        let value = result?;
        closed?;
        Ok(value)
    }
}

/// Give fixed services first choice and reserve an early attempt for a public
/// reader if several fixed discovery-only services stall until their deadlines.
pub(in super::super) fn order_installed_candidates(
    candidates: impl IntoIterator<Item = PeerSignedRelayRecordV1>,
    profile: &Profile,
) -> Vec<PeerSignedRelayRecordV1> {
    use rand::seq::SliceRandom;
    let (mut fixed, mut public): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|record| {
        profile
            .lookup_transport_keys
            .contains(&record.relay_public_key)
    });
    let mut rng = rand::thread_rng();
    fixed.shuffle(&mut rng);
    public.shuffle(&mut rng);
    let mut ordered = Vec::with_capacity(fixed.len() + public.len());
    let mut fixed = fixed.into_iter();
    let mut public = public.into_iter();
    while fixed.len() + public.len() > 0 {
        ordered.extend(fixed.next());
        ordered.extend(public.next());
    }
    ordered
}

pub(in super::super) async fn read_installed_receipt(
    scope: &IrohScopeV1,
    candidate: &PeerSignedRelayRecordV1,
    identity: TransportIdentity,
    domain: MessageDomain,
    target: &ProofTarget,
    profile: &Profile,
    budget: Duration,
) -> Result<serde_json::Value> {
    ReadRoute::new(candidate, profile)?
        .run(scope, Some(budget), async |scope, route| {
            let (mut stream, channel) = connect_route(
                scope,
                &route,
                &candidate.relay_public_key,
                identity,
                &domain,
            )
            .await?;
            let mut session = QuerySession::attach(&mut stream, channel, domain)?;
            target.verify(&session.fetch(Download::new(target.height())).await?)
        })
        .await
}
