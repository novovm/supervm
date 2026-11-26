//! Read-only native query stream over the original Finished-bound NOVONET carrier.
use super::*;
use crate::native_pipeline::{
    consensus::{
        native::{query_lane, MessageDomain},
        transport::fragment_domain,
    },
    service::client::{
        operator::receipt_proof::{Download, ProofTarget},
        NativeApplicationClient,
    },
};
use novovm_network::{
    iroh_transport::{
        iroh_stream_failure_kind_v1, IrohConnectionFailureKindV1, IrohStreamFailureKindV1,
    },
    novorudp::{
        fragments::{FragmentProfile, OutgoingMessage},
        object_link::{NativeObjectLink, NativeSendPolicy},
    },
};
use novovm_protocol::native_query::{self as wire, Answer, Frame as QueryFrame, Request, Response};

pub(super) const ENTRY: &[u8; 8] = b"NVNETQR1";
const SELECTOR: &[u8; 8] = b"NVQUERY1";
const IDLE: Duration = Duration::from_secs(30);

pub(super) struct Read {
    pub(super) request: Request,
    pub(super) answer: oneshot::Sender<Answer>,
}

#[derive(Clone)]
pub(super) struct Ingress {
    pub(super) requests: mpsc::SyncSender<Read>,
    pub(super) slot: Option<Arc<QuerySlot>>,
    pub(super) peers: Option<Arc<[Vec<u8>]>>,
}

/// The single long-lived read stream may be resumed by its authenticated
/// transport identity even when the previous QUIC path has not closed yet.
#[derive(Default)]
pub(super) struct QuerySlot {
    occupant: Mutex<Option<Occupant>>,
}

struct Occupant {
    peer: Vec<u8>,
    generation: Arc<()>,
    _lease: oneshot::Sender<()>,
}

struct Admission<'a> {
    slot: &'a QuerySlot,
    generation: Arc<()>,
    revoked: oneshot::Receiver<()>,
}

impl QuerySlot {
    fn enter(&self, peer: Vec<u8>) -> Result<Admission<'_>> {
        let mut active = self
            .occupant
            .lock()
            .map_err(|_| anyhow::anyhow!("carrier query slot poisoned"))?;
        ensure!(
            active.as_ref().is_none_or(|current| current.peer == peer),
            "carrier query slots at capacity"
        );
        let (lease, revoked) = oneshot::channel();
        let generation = Arc::new(());
        *active = Some(Occupant {
            peer,
            generation: generation.clone(),
            _lease: lease,
        });
        Ok(Admission {
            slot: self,
            generation,
            revoked,
        })
    }
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.slot.occupant.lock() {
            if active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(&current.generation, &self.generation))
            {
                active.take();
            }
        }
    }
}

fn policy() -> NativeSendPolicy {
    NativeSendPolicy {
        window: novovm_network::novorudp::NovoRudpWindowConfig::default(),
        pacing: novovm_network::novorudp::NovoRudpPacingProfile::default(),
        ttl: Duration::from_secs(10),
    }
}

pub(super) async fn accept(
    mut stream: CarrierStreamV1<'_>,
    identity: TransportIdentity,
    pins: (u64, [u8; 32], [u8; 32]),
    ingress: Ingress,
) -> Result<()> {
    let slot = ingress.slot.context("carrier queries disabled")?;
    let peer_policy = ingress.peers.as_deref().map_or(
        ServicePeerPolicyV1::AnyAuthenticated,
        ServicePeerPolicyV1::Pinned,
    );
    let (mut channel, remote) = tokio::time::timeout(Duration::from_secs(8), async {
        authenticate_structured_service_client_observed(
            &mut stream,
            &identity,
            peer_policy,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await
    })
    .await??;
    tokio::time::timeout(
        Duration::from_secs(5),
        select_chain_service(&mut stream, &mut channel, SELECTOR, pins, false),
    )
    .await??;
    let (endpoint, mut pump) =
        NativeCarrierPump::attach_continuous_v7(&mut stream, channel, vec![wire::STREAM])?;
    let domain = fragment_domain(pins.0, pins.1, pins.2);
    let mut link = NativeObjectLink::new(endpoint, vec![query_lane(domain)], policy())?;
    let mut admission = slot.enter(remote)?;
    // Observe revocation even during an outstanding read and release the old
    // receiver before serving more of that stream. The RPC queue stays bounded.
    tokio::select! {
        biased;
        _ = &mut admission.revoked => Ok(()),
        result = serve(&mut link, &mut pump, domain, &ingress.requests) => result,
    }
}

async fn serve(
    link: &mut NativeObjectLink,
    pump: &mut NativeCarrierPump<'_>,
    domain: [u8; 32],
    requests: &mpsc::SyncSender<Read>,
) -> Result<()> {
    let mut expires = Instant::now() + IDLE;
    let mut reply = None;
    loop {
        let now = Instant::now();
        if now >= expires {
            return Ok(());
        }
        let _ = link.poll(wall_ms()?, now, 32)?;
        if let Some(object) = &reply {
            if link.try_send(0, wire::STREAM, object, now)? {
                reply = None;
            }
        } else if let Some(complete) = link.poll_complete(wire::STREAM, now, 8)? {
            let mut bytes = Vec::new();
            for chunk in complete.chunks {
                ensure!(
                    chunk.len() <= wire::MAX_BYTES.saturating_sub(bytes.len()),
                    "carrier query size"
                );
                bytes.extend_from_slice(&chunk);
            }
            let QueryFrame::Request(request) = wire::decode(&bytes).map_err(anyhow::Error::msg)?
            else {
                anyhow::bail!("carrier accepts only native query requests");
            };
            expires = now + IDLE;
            let (answer, waiting) = oneshot::channel();
            let answer = match requests.try_send(Read {
                request: request.clone(),
                answer,
            }) {
                Ok(()) => tokio::time::timeout(Duration::from_secs(2), waiting).await??,
                Err(mpsc::TrySendError::Full(_)) => Answer::Error(wire::QueryError::Busy),
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    anyhow::bail!("carrier query owner stopped")
                }
            };
            reply = Some(Arc::new(OutgoingMessage::with_profile(
                domain,
                wire::encode(QueryFrame::Response(Response { request, answer }))
                    .map_err(anyhow::Error::msg)?,
                wire::MAX_BYTES,
                FragmentProfile::NativeUdpV2,
            )?));
        }
        if let Err(error) = tick(pump).await {
            if iroh_stream_failure_kind_v1(&error)
                == Some(IrohStreamFailureKindV1::Connection(
                    IrohConnectionFailureKindV1::RemoteGracefulClose,
                ))
            {
                return Ok(());
            }
            return Err(error);
        }
    }
}

pub(super) async fn read_receipt(
    scope: &IrohScopeV1,
    locator: &PeerSignedRelayRecordV1,
    identity: TransportIdentity,
    domain: MessageDomain,
    target: &ProofTarget,
) -> Result<serde_json::Value> {
    let (mut stream, channel) = connect(scope, locator, identity, &domain).await?;
    let mut queries = QuerySession::attach(&mut stream, channel, domain)?;
    target.verify(&queries.fetch(Download::new(target.height())).await?)
}

pub(super) async fn connect<'a>(
    scope: &'a IrohScopeV1,
    locator: &PeerSignedRelayRecordV1,
    identity: TransportIdentity,
    domain: &MessageDomain,
) -> Result<(
    CarrierStreamV1<'a>,
    novovm_network::transport_binding::BoundSecureChannelV1,
)> {
    let pins = (domain.chain_id, domain.genesis, domain.protocol);
    let route = IrohServiceEndpointV1::from_record(locator)?.address;
    let mut stream: CarrierStreamV1<'_> = scope.connect(&route).await?.into();
    stream.write_frame(ENTRY).await?;
    let mut channel = authenticate_structured_bound_session_observed(
        &mut stream,
        &identity,
        locator.relay_public_key.clone(),
        true,
        &mut HandshakeReplayCacheV1::new(8),
        &mut |_, _| Ok(()),
    )
    .await?;
    select_chain_service(&mut stream, &mut channel, SELECTOR, pins, true).await?;
    Ok((stream, channel))
}

pub(super) struct QuerySession<'a> {
    client: NativeApplicationClient,
    pump: Option<NativeCarrierPump<'a>>,
}

impl QuerySession<'static> {
    pub(super) fn connect_native(
        identity: TransportIdentity,
        node: novovm_network::novorudp::endpoint::NativePeer,
        domain: MessageDomain,
    ) -> Result<Self> {
        let client = NativeApplicationClient::connect_udp_query(
            identity,
            node,
            domain,
            policy(),
            wall_ms()?,
        )?;
        Ok(Self { client, pump: None })
    }
}

impl<'a> QuerySession<'a> {
    pub(super) fn attach(
        stream: &'a mut CarrierStreamV1<'_>,
        channel: novovm_network::transport_binding::BoundSecureChannelV1,
        domain: MessageDomain,
    ) -> Result<Self> {
        let (endpoint, pump) =
            NativeCarrierPump::attach_continuous_v7(stream, channel, vec![wire::STREAM])?;
        let client = NativeApplicationClient::from_query_endpoint(endpoint, domain, policy())?;
        Ok(Self {
            client,
            pump: Some(pump),
        })
    }

    pub(super) async fn query(&mut self, selector: wire::Query) -> Result<Answer> {
        let request = self.client.prepare_query(selector)?;
        let mut sent = false;
        loop {
            let now = Instant::now();
            let _ = self.client.poll(wall_ms()?, now, 32)?;
            if !sent {
                sent = self.client.try_send_query(&request, now)?;
            }
            if let Some(response) = self.client.poll_query_response(&request, now)? {
                return Ok(response.answer);
            }
            if let Some(pump) = &mut self.pump {
                tick(pump).await?;
            } else {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    }

    pub(super) async fn fetch(&mut self, mut download: Download) -> Result<Vec<u8>> {
        loop {
            let answer = self.query(download.selector()?).await?;
            let busy = matches!(answer, Answer::Error(wire::QueryError::Busy));
            if let Some(bytes) = download.accept(answer)? {
                return Ok(bytes);
            }
            if busy {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
}

async fn tick(pump: &mut NativeCarrierPump<'_>) -> Result<()> {
    tokio::select! {
        result = pump.progress() => result,
        _ = tokio::time::sleep(Duration::from_millis(1)) => Ok(()),
    }
}

#[cfg(test)]
pub(in crate::native_pipeline::service) fn test_same_identity_reconnect(
    locator: PeerSignedRelayRecordV1,
    domain: MessageDomain,
    identity: TransportIdentity,
    another: TransportIdentity,
    target: ProofTarget,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let config = || IrohConfigV1::new(&[], "127.0.0.1".parse()?)?
            .with_bind_address("127.0.0.1:0".parse()?);
        let first = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        let second = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        let (ready, ready_receipt) = oneshot::channel();
        let (request_old, mut requests) = tokio::sync::mpsc::channel::<oneshot::Sender<Result<serde_json::Value>>>(1);
        let old_locator = locator.clone();
        let old_identity = identity.clone();
        let old_domain = domain.clone();
        let target = Arc::new(target);
        let old_target = target.clone();

        let old_session = first.with_continuous_service_v4(|| Ok(()), |scope| Box::pin(async move {
            let (mut stream, channel) = connect(scope, &old_locator, old_identity, &old_domain).await?;
            let mut client = QuerySession::attach(&mut stream, channel, old_domain)?;
            let original = test_receipt(&mut client, &old_target).await?;
            ready.send(original).map_err(|_| anyhow::anyhow!("reconnect fixture stopped"))?;
            while let Some(reply) = requests.recv().await {
                let _ = reply.send(test_receipt(&mut client, &old_target).await);
            }
            Ok(())
        }));
        let replacement = second.with_continuous_service_v4(|| Ok(()), |scope| Box::pin(async move {
            let original = ready_receipt.await?;
            ensure!(original["independently_verified_finality"] == true && original["success"] == true
                && original["height"] == 1 && original["nonce_after"] == 1,
                "old session did not receive the original finalized receipt");

            let foreign = tokio::time::timeout(Duration::from_secs(4), async {
                let (mut stream, channel) = connect(scope, &locator, another, &domain).await?;
                let mut client = QuerySession::attach(&mut stream, channel, domain.clone())?;
                client.query(wire::Query::Status).await
            }).await;
            ensure!(!matches!(foreign, Ok(Ok(Answer::Status(_)))), "another identity acquired the live slot");

            let mut wrong_domain = domain.clone();
            wrong_domain.genesis[0] ^= 1;
            let wrong = tokio::time::timeout(Duration::from_secs(4), connect(scope, &locator, identity.clone(), &wrong_domain)).await;
            ensure!(!matches!(wrong, Ok(Ok(_))), "wrong-chain connection was admitted");
            ensure!(test_previous_receipt(&request_old).await? == original,
                "another identity or chain displaced the original session");

            let started = Instant::now();
            let (mut stream, channel) = tokio::time::timeout(
                Duration::from_secs(4), connect(scope, &locator, identity, &domain),
            ).await??;
            let mut client = QuerySession::attach(&mut stream, channel, domain)?;
            ensure!(test_receipt(&mut client, &target).await? == original,
                "same-identity reconnect did not return the original finalized receipt");
            let elapsed = started.elapsed();
            ensure!(test_previous_receipt(&request_old).await.is_err(),
                "retired stream served another receipt");
            drop(request_old);
            ensure!(test_receipt(&mut client, &target).await? == original,
                "retiring the old stream released the new slot");
            println!("same authenticated identity recovered the original receipt on another Iroh socket in {} ms, retired stream and foreign identity refused",
                elapsed.as_millis());
            Ok(())
        }));
        let test = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::try_join!(old_session, replacement).map(|_| ())
        }).await;
        let closed_first = first.close().await;
        let closed_second = second.close().await;
        test?.and(closed_first).and(closed_second)
    })
}

#[cfg(test)]
async fn test_receipt(
    client: &mut QuerySession<'_>,
    target: &ProofTarget,
) -> Result<serde_json::Value> {
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        client.fetch(Download::new(target.height())),
    )
    .await??;
    target.verify(&bytes)
}

#[cfg(test)]
async fn test_previous_receipt(
    requests: &tokio::sync::mpsc::Sender<oneshot::Sender<Result<serde_json::Value>>>,
) -> Result<serde_json::Value> {
    let (reply, response) = oneshot::channel();
    requests.send(reply).await?;
    response.await?
}
