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
    pub(super) slots: Arc<tokio::sync::Semaphore>,
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
    let (mut channel, _) = tokio::time::timeout(Duration::from_secs(8), async {
        authenticate_structured_service_client_observed(
            &mut stream,
            &identity,
            ServicePeerPolicyV1::AnyAuthenticated,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await
    })
    .await??;
    let _admitted = ingress
        .slots
        .try_acquire()
        .context("carrier query slots at capacity")?;
    tokio::time::timeout(
        Duration::from_secs(5),
        select_chain_service(&mut stream, &mut channel, SELECTOR, pins, false),
    )
    .await??;
    let (endpoint, mut pump) =
        NativeCarrierPump::attach_continuous_v7(&mut stream, channel, vec![wire::STREAM])?;
    let domain = fragment_domain(pins.0, pins.1, pins.2);
    let mut link = NativeObjectLink::new(endpoint, vec![query_lane(domain)], policy())?;
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
            let answer = match ingress.requests.try_send(Read {
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
        if let Err(error) = tick(&mut pump).await {
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
    pump: NativeCarrierPump<'a>,
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
        Ok(Self { client, pump })
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
            tick(&mut self.pump).await?;
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
