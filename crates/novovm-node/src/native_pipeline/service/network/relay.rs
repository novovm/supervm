//! Consume original verified locators, candidate turns and native object pump.
//! No second router graph, executor, UDP socket or caller-supplied binder.
use super::*;
use novovm_network::relay::hop::{self, HopBudget, HopRoute, HopStream};
pub(super) struct Ingress<'a> {
    pub scope: &'a IrohScopeV1,
    pub routes: Arc<Mutex<Vec<Option<Candidate>>>>,
    pub budget: HopBudget,
    pub observer: Option<Arc<NativeTransferDiagnosticsV1>>,
}

fn context(pins: (u64, [u8; 32], [u8; 32])) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(novovm_network::discovery_control::native_locator_id_v1(pins).as_bytes()).into()
}
/// Bounded turns on the SAME known candidate set: direct, each single relay,
/// then ordered two-relay paths. No path is deemed delivered or reachable here.
pub(super) fn fallback_path(
    target: usize,
    turn: usize,
    candidates: &[Option<Candidate>],
    wall: u64,
) -> Vec<Candidate> {
    let relays = candidates
        .iter()
        .enumerate()
        .filter(|(i, c)| *i != target && c.as_ref().is_some_and(|c| c.expires > wall))
        .filter_map(|(_, c)| c.clone())
        .collect::<Vec<_>>();
    let n = relays.len();
    if n == 0 {
        return Vec::new();
    }
    let slot = turn % (1 + n + n * (n - 1));
    if slot == 0 {
        return Vec::new();
    }
    let mut path = if slot <= n {
        vec![relays[slot - 1].clone()]
    } else {
        let pair = slot - 1 - n;
        let first = pair / (n - 1);
        let mut second = pair % (n - 1);
        if second >= first {
            second += 1;
        }
        vec![relays[first].clone(), relays[second].clone()]
    };
    if let Some(candidate) = &candidates[target] {
        path.push(candidate.clone());
    }
    path
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn outgoing(
    scope: &IrohScopeV1,
    identity: TransportIdentity,
    key: Vec<u8>,
    peer: usize,
    pins: (u64, [u8; 32], [u8; 32]),
    views: Arc<Mutex<Vec<PeerView>>>,
    connected: mpsc::SyncSender<Connected>,
    adopted: Arc<AtomicBool>,
    path: Vec<Candidate>,
    budget: HopBudget,
    observer: Option<Arc<NativeTransferDiagnosticsV1>>,
) -> Result<()> {
    ensure!(
        path.iter()
            .all(|c| wall_ms().is_ok_and(|now| now < c.expires)),
        "relay locator expired before dial"
    );
    let route = HopRoute::new(
        identity.public_key().to_vec(),
        path.iter().map(|c| c.key.clone()).collect(),
        context(pins),
    )?;
    let mut hop = tokio::time::timeout(Duration::from_secs(24), async {
        HopStream::connect(
            scope.connect(&path[0].route).await?.into(),
            &identity,
            route,
            Some(budget),
        )
        .await
    })
    .await??;
    if let Some(observer) = observer {
        hop.observe(observer, &identity);
    }
    let proof = hop.authenticate_native(&identity, &key, true).await?;
    let (endpoint, pump) =
        NativeCarrierPump::attach_relayed_v1(&mut hop, proof, NativeChannel::carrier_streams())?;
    run_native_pump(endpoint, pump, peer, views, connected, adopted).await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn incoming(
    stream: CarrierStreamV1<'_>,
    identity: TransportIdentity,
    keys: Vec<Vec<u8>>,
    pins: (u64, [u8; 32], [u8; 32]),
    views: Arc<Mutex<Vec<PeerView>>>,
    connected: mpsc::SyncSender<Connected>,
    ingress: Ingress<'_>,
) -> Result<()> {
    let mut hop = tokio::time::timeout(
        Duration::from_secs(8),
        HopStream::accept(stream, &identity, context(pins), ingress.budget.clone()),
    )
    .await??;
    if let Some(observer) = ingress.observer {
        hop.observe(observer, &identity);
    }
    if let Some(next) = hop.route().next() {
        let next = ingress
            .routes
            .lock()
            .map_err(|_| anyhow::anyhow!("NOVONET route view poisoned"))?
            .iter()
            .flatten()
            .find(|c| c.key == next && wall_ms().is_ok_and(|now| now < c.expires))
            .cloned()
            .context("no fresh verified next-hop locator")?;
        let route = hop.route().forwarded()?;
        let mut downstream = tokio::time::timeout(Duration::from_secs(24), async {
            HopStream::connect(
                ingress.scope.connect(&next.route).await?.into(),
                &identity,
                route,
                Some(ingress.budget),
            )
            .await
        })
        .await??;
        return hop::bridge(&mut hop, &mut downstream).await;
    }
    let origin = hop.route().origin().to_vec();
    let peer = keys
        .iter()
        .position(|key| *key == origin)
        .context("relay origin has no native peer authority")?;
    ensure!(
        origin.as_slice() < identity.public_key(),
        "native relayed initiation role differs"
    );
    hop.confirm().await?;
    let proof = hop.authenticate_native(&identity, &origin, false).await?;
    let (endpoint, pump) =
        NativeCarrierPump::attach_relayed_v1(&mut hop, proof, NativeChannel::carrier_streams())?;
    run_native_pump(
        endpoint,
        pump,
        peer,
        views,
        connected,
        Arc::new(AtomicBool::new(false)),
    )
    .await
}
