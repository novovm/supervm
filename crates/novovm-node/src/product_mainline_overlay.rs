//! Product Overlay ownership inside the NOVOVM main node lifecycle.
//!
//! The relay remains transport-only. Decrypted payloads are returned to the node so the normal
//! native transaction ingress can enforce chain-domain, signature, identity, and nonce policy
//! before AOEM execution.

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    peer_id_from_ed25519_public_key_v1, HandshakeReplayCacheV1, NodeHandshakeInitiatorV1,
    NodeHandshakeResponderV1, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
    RelayPeerHandshakeV1, RelayTransportV1, StrategyPathV1,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    product_node_overlay::{
        ProductNodeBootstrapStatusV1, ProductNodeOverlayConfigV1, ProductNodeOverlayRuntimeV1,
        ProductNodeRoutePlanV1,
    },
    product_relay_client::{
        ProductRelayClientConfigV1, ProductRelayClientEventV1, ProductRelayClientV1,
        ProductRelayTlsTrustV1,
    },
    tx_ingress::ingest_local_nov_raw_tx_payload_v1,
};

const PRODUCT_MAINLINE_OVERLAY_SCOPE_V1: &str = "novovm_product_mainline_overlay_runtime_v1";
const PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1: [u8; 16] = *b"NOVOVM-OVERLAY-1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductMainlineOverlayRoleV1 {
    Initiator,
    Responder,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductMainlineOverlayConfigV1 {
    pub chain_id: u64,
    pub role: ProductMainlineOverlayRoleV1,
    pub identity_key_path: PathBuf,
    pub overlay: ProductNodeOverlayConfigV1,
    #[serde(default)]
    pub target_peer_id: Option<String>,
    #[serde(default)]
    pub expected_source_peer_id: Option<String>,
    #[serde(default = "default_connect_timeout_ms_v1")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_read_timeout_ms_v1")]
    pub read_timeout_ms: u64,
    #[serde(default = "default_tls_trust_v1")]
    pub tls_trust: ProductRelayTlsTrustV1,
    #[serde(default = "default_channel_capacity_v1")]
    pub channel_capacity: usize,
    #[serde(default = "default_metric_peer_id_v1")]
    pub metric_peer_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductMainlineOverlayStartupV1 {
    pub scope: String,
    pub local_peer_id: String,
    pub remote_peer_id: String,
    pub role: ProductMainlineOverlayRoleV1,
    pub bootstrap: ProductNodeBootstrapStatusV1,
    pub route_plan: ProductNodeRoutePlanV1,
    pub payload_treated_opaque_by_relay: bool,
    pub relay_is_trusted_authority: bool,
    pub aoem_transport_policy_embedded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductMainlineOverlayInboundV1 {
    pub source_peer_id: String,
    pub frame: NovoRudpTransportFrameV0,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductMainlineOverlayDeliveryV1 {
    pub tx_hash: [u8; 32],
    pub delivered: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMainlineOverlayIngressReceiptV1 {
    pub chain_id: u64,
    pub tx_hash: [u8; 32],
    pub ingress_entry: String,
    pub pending_only: bool,
    pub execution_owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductMainlineOverlayEventV1 {
    RelayConnected { relay_peer_id: String },
    E2eSessionEstablished { remote_peer_id: String },
    Inbound(ProductMainlineOverlayInboundV1),
    Delivery(ProductMainlineOverlayDeliveryV1),
    WorkerStopped,
    WorkerFailed(String),
}

#[derive(Debug)]
struct ProductMainlineOverlayOutboundV1 {
    tx_hash: [u8; 32],
    payload: Vec<u8>,
}

struct ProductMainlineOverlayWorkerV1 {
    identity: SigningKey,
    relay_config: ProductRelayClientConfigV1,
    role: ProductMainlineOverlayRoleV1,
    remote_peer_id: String,
    expected_source_peer_id: Option<String>,
    chain_id: u64,
    outbound: Receiver<ProductMainlineOverlayOutboundV1>,
    events: SyncSender<ProductMainlineOverlayEventV1>,
    stop: Arc<AtomicBool>,
}

pub struct ProductMainlineOverlayRuntimeV1 {
    startup: ProductMainlineOverlayStartupV1,
    role: ProductMainlineOverlayRoleV1,
    metric_peer_id: u64,
    outbound: SyncSender<ProductMainlineOverlayOutboundV1>,
    events: Receiver<ProductMainlineOverlayEventV1>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ProductMainlineOverlayRuntimeV1 {
    pub fn start(config: ProductMainlineOverlayConfigV1, now_ms: u64) -> Result<Self> {
        Self::start_inner_v1(config, now_ms, None)
    }

    fn start_inner_v1(
        config: ProductMainlineOverlayConfigV1,
        now_ms: u64,
        relay_override: Option<ProductRelayClientConfigV1>,
    ) -> Result<Self> {
        validate_config_v1(&config)?;
        let identity = load_ed25519_identity_v1(&config.identity_key_path)?;
        let local_peer_id =
            peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes());
        let remote_peer_id = configured_remote_peer_id_v1(&config)?.to_string();
        if local_peer_id == remote_peer_id {
            bail!("product mainline overlay remote peer must differ from local peer");
        }

        let overlay = ProductNodeOverlayRuntimeV1::bootstrap(&config.overlay, now_ms)
            .context("bootstrap product overlay inside main node lifecycle")?;
        let route_plan = overlay.select_relay_route(
            &identity,
            format!("mainline-overlay-{now_ms}"),
            &remote_peer_id,
            now_ms,
            None,
            false,
        );
        if route_plan.selected_path != StrategyPathV1::RelayNovoRudp {
            bail!(
                "product mainline overlay requires a reachable signed relay candidate: {:?}",
                route_plan.fallback_reason
            );
        }
        let selected = route_plan
            .selected_relay
            .as_ref()
            .context("relay route selected without a signed relay record")?;
        if selected.selected_endpoint.transport != RelayTransportV1::Wss443 {
            bail!(
                "product mainline overlay selected unsupported live transport {:?}; WSS is required",
                selected.selected_endpoint.transport
            );
        }
        let relay = relay_override.unwrap_or_else(|| ProductRelayClientConfigV1 {
            endpoint: selected.selected_endpoint.uri.clone(),
            expected_relay_peer_id: selected.relay_peer_id.clone(),
            connect_timeout_ms: config.connect_timeout_ms.max(1),
            read_timeout_ms: config.read_timeout_ms.clamp(1, 1_000),
            tls_trust: config.tls_trust.clone(),
        });
        let startup = ProductMainlineOverlayStartupV1 {
            scope: PRODUCT_MAINLINE_OVERLAY_SCOPE_V1.into(),
            local_peer_id,
            remote_peer_id: remote_peer_id.clone(),
            role: config.role,
            bootstrap: overlay.bootstrap_status().clone(),
            route_plan,
            payload_treated_opaque_by_relay: true,
            relay_is_trusted_authority: false,
            aoem_transport_policy_embedded: false,
        };
        let capacity = config.channel_capacity.clamp(1, 65_536);
        let (outbound_tx, outbound_rx) = mpsc::sync_channel(capacity);
        let (event_tx, event_rx) = mpsc::sync_channel(capacity);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_role = config.role;
        let worker_remote_peer_id = remote_peer_id;
        let chain_id = config.chain_id;
        let expected_source_peer_id = config.expected_source_peer_id.clone();
        let worker = thread::Builder::new()
            .name("novovm-product-overlay".into())
            .spawn(move || {
                let result = run_worker_v1(ProductMainlineOverlayWorkerV1 {
                    identity,
                    relay_config: relay,
                    role: worker_role,
                    remote_peer_id: worker_remote_peer_id,
                    expected_source_peer_id,
                    chain_id,
                    outbound: outbound_rx,
                    events: event_tx.clone(),
                    stop: worker_stop,
                });
                if let Err(error) = result {
                    let _ = event_tx.send(ProductMainlineOverlayEventV1::WorkerFailed(
                        error.to_string(),
                    ));
                }
                let _ = event_tx.send(ProductMainlineOverlayEventV1::WorkerStopped);
            })
            .context("spawn product overlay mainline lifecycle worker")?;

        Ok(Self {
            startup,
            role: config.role,
            metric_peer_id: config.metric_peer_id,
            outbound: outbound_tx,
            events: event_rx,
            stop,
            worker: Some(worker),
        })
    }

    #[cfg(test)]
    fn start_with_relay_override_v1(
        config: ProductMainlineOverlayConfigV1,
        now_ms: u64,
        relay: ProductRelayClientConfigV1,
    ) -> Result<Self> {
        Self::start_inner_v1(config, now_ms, Some(relay))
    }

    #[must_use]
    pub fn startup(&self) -> &ProductMainlineOverlayStartupV1 {
        &self.startup
    }

    #[must_use]
    pub fn role(&self) -> ProductMainlineOverlayRoleV1 {
        self.role
    }

    #[must_use]
    pub fn metric_peer_id(&self) -> u64 {
        self.metric_peer_id
    }

    pub fn try_submit(&self, tx_hash: [u8; 32], payload: Vec<u8>) -> Result<bool> {
        if self.role != ProductMainlineOverlayRoleV1::Initiator {
            return Ok(false);
        }
        if payload.is_empty() {
            bail!("product mainline overlay refuses an empty transaction payload");
        }
        match self
            .outbound
            .try_send(ProductMainlineOverlayOutboundV1 { tx_hash, payload })
        {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => {
                bail!("product mainline overlay worker is disconnected")
            }
        }
    }

    pub fn drain_events(&self, limit: usize) -> Vec<ProductMainlineOverlayEventV1> {
        let mut events = Vec::new();
        for _ in 0..limit.max(1) {
            match self.events.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        events
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for ProductMainlineOverlayRuntimeV1 {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn load_product_mainline_overlay_config_v1(
    path: impl AsRef<Path>,
) -> Result<ProductMainlineOverlayConfigV1> {
    let path = path.as_ref();
    let bytes = fs::read(path)
        .with_context(|| format!("read product mainline overlay config: {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("decode product mainline overlay config: {}", path.display()))
}

pub fn ingest_product_mainline_overlay_payload_v1(
    chain_id: u64,
    payload: &[u8],
) -> Result<ProductMainlineOverlayIngressReceiptV1> {
    let (native_tx, _ir, tx_hash) = ingest_local_nov_raw_tx_payload_v1(
        &serde_json::json!({
            "chain_id": chain_id,
            "pipeline_only": true,
            "ingress_source": PRODUCT_MAINLINE_OVERLAY_SCOPE_V1,
        }),
        payload,
    )
    .context("product overlay payload rejected by native transaction ingress")?;
    if native_tx.chain_id != chain_id {
        bail!(
            "product overlay native tx chain_id {} does not match pipeline chain_id {}",
            native_tx.chain_id,
            chain_id
        );
    }
    Ok(ProductMainlineOverlayIngressReceiptV1 {
        chain_id,
        tx_hash,
        ingress_entry: "ingest_local_nov_raw_tx_payload_v1".into(),
        pending_only: true,
        execution_owner: "aoem_runtime".into(),
    })
}

fn run_worker_v1(worker: ProductMainlineOverlayWorkerV1) -> Result<()> {
    let mut relay = ProductRelayClientV1::connect(&worker.identity, &worker.relay_config)?;
    worker
        .events
        .send(ProductMainlineOverlayEventV1::RelayConnected {
            relay_peer_id: relay.session().relay_peer_id.clone(),
        })
        .context("publish product overlay relay-connected event")?;
    match worker.role {
        ProductMainlineOverlayRoleV1::Initiator => run_initiator_v1(
            &mut relay,
            &worker.identity,
            &worker.remote_peer_id,
            worker.chain_id,
            worker.outbound,
            &worker.events,
            &worker.stop,
        )?,
        ProductMainlineOverlayRoleV1::Responder => run_responder_v1(
            &mut relay,
            &worker.identity,
            worker.expected_source_peer_id.as_deref(),
            worker.chain_id,
            &worker.events,
            &worker.stop,
        )?,
    }
    relay.close().context("close product overlay relay session")
}

fn run_initiator_v1(
    relay: &mut ProductRelayClientV1,
    identity: &SigningKey,
    remote_peer_id: &str,
    chain_id: u64,
    outbound: Receiver<ProductMainlineOverlayOutboundV1>,
    events: &SyncSender<ProductMainlineOverlayEventV1>,
    stop: &AtomicBool,
) -> Result<()> {
    let initiator = NodeHandshakeInitiatorV1::start(identity, remote_peer_id, now_ms_v1(), 30_000)?;
    relay.send_peer_handshake(
        remote_peer_id,
        RelayPeerHandshakeV1::Offer(initiator.offer().clone()),
    )?;
    let response = loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let Some(event) = recv_relay_event_or_idle_v1(relay)? else {
            continue;
        };
        match event {
            ProductRelayClientEventV1::PeerHandshake(delivery) => match delivery.handshake {
                RelayPeerHandshakeV1::Response(response) => break response,
                RelayPeerHandshakeV1::Offer(_) => {
                    bail!("product overlay initiator received an unexpected peer offer")
                }
            },
            ProductRelayClientEventV1::HeartbeatAck => {}
            ProductRelayClientEventV1::Closed => {
                bail!("product overlay relay closed before peer response")
            }
            ProductRelayClientEventV1::Delivery(_) => {
                bail!("product overlay initiator received data before peer authentication")
            }
        }
    };
    let mut replay = HandshakeReplayCacheV1::default();
    let mut channel = initiator.complete(&response, now_ms_v1(), &mut replay)?;
    events
        .send(ProductMainlineOverlayEventV1::E2eSessionEstablished {
            remote_peer_id: remote_peer_id.into(),
        })
        .context("publish product overlay E2E-ready event")?;
    let mut sequence = 0u64;
    while !stop.load(Ordering::Acquire) {
        let outbound = match outbound.recv_timeout(Duration::from_millis(100)) {
            Ok(outbound) => outbound,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let object_id = u64::from_le_bytes(outbound.tx_hash[..8].try_into().unwrap_or_default());
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            chain_id,
            object_id,
            sequence,
            0,
            outbound.payload,
        );
        sequence = sequence.saturating_add(1);
        let delivery = match channel
            .seal_novorudp_frame(&frame)
            .map_err(anyhow::Error::from)
            .and_then(|envelope| relay.send_envelope(envelope))
        {
            Ok(()) => ProductMainlineOverlayDeliveryV1 {
                tx_hash: outbound.tx_hash,
                delivered: true,
                error: None,
            },
            Err(error) => ProductMainlineOverlayDeliveryV1 {
                tx_hash: outbound.tx_hash,
                delivered: false,
                error: Some(error.to_string()),
            },
        };
        let delivered = delivery.delivered;
        events
            .send(ProductMainlineOverlayEventV1::Delivery(delivery))
            .context("publish product overlay delivery event")?;
        if !delivered {
            bail!("product overlay relay delivery failed");
        }
    }
    Ok(())
}

fn run_responder_v1(
    relay: &mut ProductRelayClientV1,
    identity: &SigningKey,
    expected_source_peer_id: Option<&str>,
    chain_id: u64,
    events: &SyncSender<ProductMainlineOverlayEventV1>,
    stop: &AtomicBool,
) -> Result<()> {
    let offer = loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let Some(event) = recv_relay_event_or_idle_v1(relay)? else {
            continue;
        };
        match event {
            ProductRelayClientEventV1::PeerHandshake(delivery) => match delivery.handshake {
                RelayPeerHandshakeV1::Offer(offer) => {
                    if expected_source_peer_id
                        .is_some_and(|expected| expected != offer.initiator_peer_id)
                    {
                        bail!("product overlay responder rejected unexpected source peer");
                    }
                    break offer;
                }
                RelayPeerHandshakeV1::Response(_) => {
                    bail!("product overlay responder received an unexpected peer response")
                }
            },
            ProductRelayClientEventV1::HeartbeatAck => {}
            ProductRelayClientEventV1::Closed => {
                bail!("product overlay relay closed before peer offer")
            }
            ProductRelayClientEventV1::Delivery(_) => {
                bail!("product overlay responder received data before peer authentication")
            }
        }
    };
    let source_peer_id = offer.initiator_peer_id.clone();
    let mut replay = HandshakeReplayCacheV1::default();
    let responder =
        NodeHandshakeResponderV1::respond(&offer, identity, now_ms_v1(), 30_000, &mut replay)?;
    let response = responder.response().clone();
    let mut channel = responder.into_channel();
    relay.send_peer_handshake(
        source_peer_id.clone(),
        RelayPeerHandshakeV1::Response(response),
    )?;
    events
        .send(ProductMainlineOverlayEventV1::E2eSessionEstablished {
            remote_peer_id: source_peer_id.clone(),
        })
        .context("publish product overlay E2E-ready event")?;

    while !stop.load(Ordering::Acquire) {
        let Some(event) = recv_relay_event_or_idle_v1(relay)? else {
            continue;
        };
        match event {
            ProductRelayClientEventV1::Delivery(delivery) => {
                let frame = channel.open_novorudp_frame(&delivery.envelope)?;
                validate_inbound_frame_v1(&frame, chain_id)?;
                events
                    .send(ProductMainlineOverlayEventV1::Inbound(
                        ProductMainlineOverlayInboundV1 {
                            source_peer_id: source_peer_id.clone(),
                            frame,
                        },
                    ))
                    .context("publish product overlay inbound event")?;
            }
            ProductRelayClientEventV1::HeartbeatAck => {}
            ProductRelayClientEventV1::Closed => break,
            ProductRelayClientEventV1::PeerHandshake(_) => {
                bail!("product overlay responder received a second peer handshake")
            }
        }
    }
    Ok(())
}

fn validate_config_v1(config: &ProductMainlineOverlayConfigV1) -> Result<()> {
    if config.chain_id == 0 {
        bail!("product mainline overlay chain_id must be positive");
    }
    if config.channel_capacity == 0 {
        bail!("product mainline overlay channel_capacity must be positive");
    }
    match config.role {
        ProductMainlineOverlayRoleV1::Initiator => {
            if config.target_peer_id.as_deref().is_none_or(str::is_empty) {
                bail!("product mainline overlay initiator requires target_peer_id");
            }
        }
        ProductMainlineOverlayRoleV1::Responder => {
            if config
                .expected_source_peer_id
                .as_deref()
                .is_none_or(str::is_empty)
            {
                bail!("product mainline overlay responder requires expected_source_peer_id");
            }
        }
    }
    Ok(())
}

fn recv_relay_event_or_idle_v1(
    relay: &mut ProductRelayClientV1,
) -> Result<Option<ProductRelayClientEventV1>> {
    match relay.recv_event() {
        Ok(event) => Ok(Some(event)),
        Err(error) if relay_read_timed_out_v1(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn relay_read_timed_out_v1(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(
                io.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
        })
    })
}

fn configured_remote_peer_id_v1(config: &ProductMainlineOverlayConfigV1) -> Result<&str> {
    match config.role {
        ProductMainlineOverlayRoleV1::Initiator => config
            .target_peer_id
            .as_deref()
            .context("product overlay initiator target peer is missing"),
        ProductMainlineOverlayRoleV1::Responder => config
            .expected_source_peer_id
            .as_deref()
            .context("product overlay responder source peer is missing"),
    }
}

fn validate_inbound_frame_v1(frame: &NovoRudpTransportFrameV0, chain_id: u64) -> Result<()> {
    if frame.kind != NovoRudpTransportFrameKindV0::Data {
        bail!("product mainline overlay accepts only NovoRUDP data frames");
    }
    if frame.stream_id != chain_id {
        bail!(
            "product mainline overlay frame chain {} does not match node chain {}",
            frame.stream_id,
            chain_id
        );
    }
    if frame.payload.is_empty() {
        bail!("product mainline overlay received an empty transaction payload");
    }
    Ok(())
}

fn load_ed25519_identity_v1(path: &Path) -> Result<SigningKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read product mainline identity key: {}", path.display()))?;
    let text = text.trim();
    if text.len() != 64 {
        bail!("product mainline identity key must contain exactly 64 hexadecimal characters");
    }
    let mut secret = [0u8; 32];
    for (index, output) in secret.iter_mut().enumerate() {
        *output = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .context("decode product mainline identity key hex")?;
    }
    Ok(SigningKey::from_bytes(&secret))
}

fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn default_connect_timeout_ms_v1() -> u64 {
    5_000
}

fn default_read_timeout_ms_v1() -> u64 {
    250
}

fn default_tls_trust_v1() -> ProductRelayTlsTrustV1 {
    ProductRelayTlsTrustV1::NodeKeyBoundEncrypted
}

fn default_channel_capacity_v1() -> usize {
    1_024
}

fn default_metric_peer_id_v1() -> u64 {
    9_990_777
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        product_node_overlay::ProductBootstrapSourceV1,
        product_relay_daemon::{run_product_relay_daemon_v1, ProductRelayDaemonConfigV1},
        tx_ingress::sign_nov_native_tx_with_seed_v1,
    };
    use novovm_network::{
        sign_bootstrap_manifest_v1, sign_relay_record_v1, BootstrapSourceKindV1,
        PeerSignedRelayRecordV1, RelayEndpointV1, SignedBootstrapManifestV1,
    };
    use novovm_protocol::{
        encode_nov_native_tx_wire_v1, NovExecuteTxV1, NovExecutionModeV1, NovExecutionPolicyV1,
        NovExecutionTargetV1, NovFeePolicyV1, NovNativeTxWireV1, NovPrivacyModeV1, NovTxKindV1,
        NovVerificationModeV1,
    };
    use std::{net::TcpListener, thread, time::Instant};

    fn config(role: ProductMainlineOverlayRoleV1) -> ProductMainlineOverlayConfigV1 {
        ProductMainlineOverlayConfigV1 {
            chain_id: 7,
            role,
            identity_key_path: PathBuf::from("identity.hex"),
            overlay: ProductNodeOverlayConfigV1 {
                cache_path: PathBuf::from("cache.json"),
                trusted_signer_public_keys: Vec::new(),
                minimum_bootstrap_signatures: 1,
                embedded_sources: Vec::new(),
                cooldown_base_ms: None,
                cooldown_max_ms: None,
            },
            target_peer_id: None,
            expected_source_peer_id: None,
            connect_timeout_ms: 5_000,
            read_timeout_ms: 250,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
            channel_capacity: 16,
            metric_peer_id: 91,
        }
    }

    #[test]
    fn lifecycle_role_requires_an_authenticated_remote_peer() {
        let initiator = config(ProductMainlineOverlayRoleV1::Initiator);
        assert!(validate_config_v1(&initiator)
            .unwrap_err()
            .to_string()
            .contains("target_peer_id"));

        let responder = config(ProductMainlineOverlayRoleV1::Responder);
        assert!(validate_config_v1(&responder)
            .unwrap_err()
            .to_string()
            .contains("expected_source_peer_id"));
    }

    #[test]
    fn inbound_boundary_rejects_non_data_cross_chain_and_empty_frames() {
        let control = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Ack,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            7,
            1,
            1,
            0,
            vec![1],
        );
        assert!(validate_inbound_frame_v1(&control, 7).is_err());

        let cross_chain = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            8,
            1,
            1,
            0,
            vec![1],
        );
        assert!(validate_inbound_frame_v1(&cross_chain, 7).is_err());

        let empty = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            7,
            1,
            1,
            0,
            Vec::new(),
        );
        assert!(validate_inbound_frame_v1(&empty, 7).is_err());
        assert!(ingest_product_mainline_overlay_payload_v1(7, &[1, 2, 3]).is_err());
    }

    #[test]
    fn mainline_lifecycle_owns_authenticated_overlay_delivery() {
        let now = now_ms_v1();
        let root = std::env::temp_dir().join(format!("novovm-product-mainline-overlay-{now}"));
        fs::create_dir_all(&root).unwrap();
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = root.join("relay-cert.pem");
        let tls_key_path = root.join("relay-key.pem");
        let relay_identity_path = root.join("relay-identity.hex");
        let node_a_identity_path = root.join("node-a.hex");
        let node_b_identity_path = root.join("node-b.hex");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&tls_key_path, certificate.serialize_private_key_pem()).unwrap();
        write_identity_v1(&relay_identity_path, [31; 32]);
        write_identity_v1(&node_a_identity_path, [32; 32]);
        write_identity_v1(&node_b_identity_path, [33; 32]);

        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let relay_report_path = root.join("relay-report.json");
        let daemon_config = ProductRelayDaemonConfigV1 {
            bind_addr: format!("127.0.0.1:{port}"),
            tls_cert_path: certificate_path,
            tls_key_path,
            relay_identity_key_path: relay_identity_path,
            report_path: relay_report_path.clone(),
            report_interval_ms: 20,
            run_for_ms: Some(3_000),
            session_queue_capacity: Some(16),
            offline_queue_per_peer: Some(16),
            offline_queue_total: Some(32),
            session_ttl_ms: Some(5_000),
            rate_limit_frames: Some(1_000),
            rate_limit_window_ms: Some(1_000),
        };
        let daemon = thread::spawn(move || run_product_relay_daemon_v1(daemon_config));
        wait_until_v1(Duration::from_secs(1), || relay_report_path.exists());

        let relay_identity = SigningKey::from_bytes(&[31; 32]);
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let bootstrap_signer = SigningKey::from_bytes(&[34; 32]);
        let source = signed_bootstrap_source_v1(
            &bootstrap_signer,
            &relay_identity,
            now.saturating_sub(1_000),
            now.saturating_add(30_000),
        );
        let node_a = SigningKey::from_bytes(&[32; 32]);
        let node_b = SigningKey::from_bytes(&[33; 32]);
        let node_a_peer_id = peer_id_from_ed25519_public_key_v1(&node_a.verifying_key().to_bytes());
        let node_b_peer_id = peer_id_from_ed25519_public_key_v1(&node_b.verifying_key().to_bytes());
        let chain_id = 8_000_000 + now % 100_000;
        let responder_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Responder,
            node_b_identity_path,
            root.join("node-b-cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source.clone(),
            None,
            Some(node_a_peer_id),
        );
        let initiator_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Initiator,
            node_a_identity_path,
            root.join("node-a-cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source,
            Some(node_b_peer_id),
            None,
        );

        let relay_override = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{port}/novovm"),
            expected_relay_peer_id: relay_peer_id.clone(),
            connect_timeout_ms: 1_000,
            read_timeout_ms: 50,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let mut responder = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            responder_config,
            now_ms_v1(),
            relay_override.clone(),
        )
        .unwrap();
        let mut initiator = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            initiator_config,
            now_ms_v1(),
            relay_override,
        )
        .unwrap();
        assert_eq!(
            initiator
                .startup()
                .route_plan
                .selected_relay
                .as_ref()
                .unwrap()
                .relay_peer_id,
            relay_peer_id
        );
        wait_for_event_v1(&initiator, Duration::from_secs(2), |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::E2eSessionEstablished { .. }
            )
        });
        let tx_hash = [0x45; 32];
        let raw_tx = signed_native_tx_v1(chain_id, &format!("overlay-{now}"));
        assert!(initiator.try_submit(tx_hash, raw_tx.clone()).unwrap());
        let inbound = wait_for_inbound_v1(&responder, Duration::from_secs(2));
        assert_eq!(inbound.source_peer_id, initiator.startup().local_peer_id);
        assert_eq!(inbound.frame.stream_id, chain_id);
        assert_eq!(inbound.frame.payload, raw_tx);
        let ingress =
            ingest_product_mainline_overlay_payload_v1(chain_id, &inbound.frame.payload).unwrap();
        assert_eq!(ingress.chain_id, chain_id);
        assert!(ingress.pending_only);
        assert_eq!(ingress.execution_owner, "aoem_runtime");
        wait_for_event_v1(&initiator, Duration::from_secs(1), |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::Delivery(
                    ProductMainlineOverlayDeliveryV1 {
                        tx_hash: delivered_hash,
                        delivered: true,
                        ..
                    }
                ) if *delivered_hash == tx_hash
            )
        });

        initiator.shutdown();
        responder.shutdown();
        daemon.join().unwrap().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[allow(clippy::too_many_arguments)]
    fn mainline_config_v1(
        chain_id: u64,
        role: ProductMainlineOverlayRoleV1,
        identity_key_path: PathBuf,
        cache_path: PathBuf,
        trusted_signer: [u8; 32],
        source: ProductBootstrapSourceV1,
        target_peer_id: Option<String>,
        expected_source_peer_id: Option<String>,
    ) -> ProductMainlineOverlayConfigV1 {
        ProductMainlineOverlayConfigV1 {
            chain_id,
            role,
            identity_key_path,
            overlay: ProductNodeOverlayConfigV1 {
                cache_path,
                trusted_signer_public_keys: vec![trusted_signer],
                minimum_bootstrap_signatures: 1,
                embedded_sources: vec![source],
                cooldown_base_ms: Some(10),
                cooldown_max_ms: Some(100),
            },
            target_peer_id,
            expected_source_peer_id,
            connect_timeout_ms: 1_000,
            read_timeout_ms: 50,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
            channel_capacity: 16,
            metric_peer_id: 91,
        }
    }

    fn signed_bootstrap_source_v1(
        signer: &SigningKey,
        relay: &SigningKey,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> ProductBootstrapSourceV1 {
        let record: PeerSignedRelayRecordV1 = sign_relay_record_v1(
            relay,
            "mainline-relay",
            vec![RelayEndpointV1 {
                transport: RelayTransportV1::Wss443,
                uri: "wss://localhost:443/novovm".into(),
                priority: 1,
                max_sessions: 16,
                max_bytes_per_minute: 1_000_000,
            }],
            issued_at_ms,
            expires_at_ms,
            1,
        )
        .unwrap();
        let mut manifest = SignedBootstrapManifestV1 {
            version: 1,
            manifest_id: "mainline-overlay-manifest".into(),
            issued_at_ms,
            expires_at_ms,
            candidate_limit: 1,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: vec![record],
            signatures: Vec::new(),
        };
        sign_bootstrap_manifest_v1(&mut manifest, signer).unwrap();
        ProductBootstrapSourceV1 {
            source_kind: BootstrapSourceKindV1::EmbeddedInstall,
            priority: 1,
            manifest,
        }
    }

    fn write_identity_v1(path: &Path, key: [u8; 32]) {
        let hex = key
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        fs::write(path, hex).unwrap();
    }

    fn signed_native_tx_v1(chain_id: u64, account: &str) -> Vec<u8> {
        let mut tx = NovNativeTxWireV1 {
            chain_id,
            kind: NovTxKindV1::Execute(NovExecuteTxV1 {
                caller: Vec::new(),
                account_id: Some(account.into()),
                fee_owner_account_id: Some(account.into()),
                nonce_owner_account_id: Some(account.into()),
                target: NovExecutionTargetV1::NativeModule("treasury".into()),
                method: "deposit_reserve".into(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 7
                }))
                .unwrap(),
                execution_mode: NovExecutionModeV1::Standard,
                execution_policy: NovExecutionPolicyV1::Standard,
                privacy_mode: NovPrivacyModeV1::Public,
                verification_mode: NovVerificationModeV1::Standard,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "USDT".into(),
                    max_pay_amount: 50,
                    slippage_bps: 100,
                },
                gas_like_limit: Some(90_000),
                nonce: 1,
            }),
            signature: Vec::new(),
        };
        sign_nov_native_tx_with_seed_v1(&mut tx, [0x44; 32]).unwrap();
        encode_nov_native_tx_wire_v1(&tx).unwrap()
    }

    fn wait_until_v1(timeout: Duration, mut predicate: impl FnMut() -> bool) {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if predicate() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for product overlay condition");
    }

    fn wait_for_event_v1(
        runtime: &ProductMainlineOverlayRuntimeV1,
        timeout: Duration,
        predicate: impl Fn(&ProductMainlineOverlayEventV1) -> bool,
    ) {
        let started = Instant::now();
        while started.elapsed() < timeout {
            for event in runtime.drain_events(32) {
                if let ProductMainlineOverlayEventV1::WorkerFailed(error) = &event {
                    panic!("product overlay worker failed: {error}");
                }
                if predicate(&event) {
                    return;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for product overlay lifecycle event");
    }

    fn wait_for_inbound_v1(
        runtime: &ProductMainlineOverlayRuntimeV1,
        timeout: Duration,
    ) -> ProductMainlineOverlayInboundV1 {
        let started = Instant::now();
        while started.elapsed() < timeout {
            for event in runtime.drain_events(32) {
                match event {
                    ProductMainlineOverlayEventV1::Inbound(inbound) => return inbound,
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("product overlay worker failed: {error}")
                    }
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for product overlay inbound payload");
    }
}
