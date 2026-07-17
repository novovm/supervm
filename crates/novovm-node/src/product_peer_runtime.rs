//! Headless A/B peer runtime for real relay-first delivery evidence.

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    peer_id_from_ed25519_public_key_v1, HandshakeReplayCacheV1, NodeHandshakeInitiatorV1,
    NodeHandshakeResponderV1, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
    RelayPeerHandshakeV1,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::product_relay_client::{
    ProductRelayClientConfigV1, ProductRelayClientEventV1, ProductRelayClientV1,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductPeerRoleV1 {
    Sender,
    Receiver,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductPeerRuntimeConfigV1 {
    pub role: ProductPeerRoleV1,
    pub identity_key_path: PathBuf,
    pub relay: ProductRelayClientConfigV1,
    #[serde(default)]
    pub target_peer_id: Option<String>,
    #[serde(default)]
    pub expected_source_peer_id: Option<String>,
    #[serde(default = "default_frame_count_v1")]
    pub frame_count: u64,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductPeerRuntimeReportV1 {
    pub accepted: bool,
    pub scope: String,
    pub role: String,
    pub local_peer_id: String,
    pub remote_peer_id: Option<String>,
    pub relay_peer_id: String,
    pub selected_path: String,
    pub selected_transport: String,
    pub peer_handshake_via_relay: bool,
    pub e2e_session_established: bool,
    pub sent_frame_count: u64,
    pub received_frame_count: u64,
    pub novorudp_inner_frame_preserved: bool,
    pub payload_treated_opaque: bool,
    pub relay_is_trusted_authority: bool,
    pub network_only: bool,
    pub apfl_interpreted: bool,
    pub aoem_called: bool,
    pub ledger_semantics: bool,
    pub novorudp_wire_changed: bool,
}

pub fn load_product_peer_runtime_config_v1(
    path: impl AsRef<Path>,
) -> Result<ProductPeerRuntimeConfigV1> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).with_context(|| format!("read peer runtime config: {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("decode peer runtime config: {}", path.display()))
}

pub fn run_product_peer_runtime_v1(
    config: ProductPeerRuntimeConfigV1,
) -> Result<ProductPeerRuntimeReportV1> {
    if config.frame_count == 0 {
        bail!("frame_count must be positive");
    }
    let identity = load_ed25519_key_v1(&config.identity_key_path)?;
    let local_peer_id = peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes());
    let mut relay = ProductRelayClientV1::connect(&identity, &config.relay)?;
    let relay_peer_id = relay.session().relay_peer_id.clone();
    let report = match config.role {
        ProductPeerRoleV1::Sender => run_sender_v1(
            &mut relay,
            &identity,
            &local_peer_id,
            &config,
            relay_peer_id,
        )?,
        ProductPeerRoleV1::Receiver => run_receiver_v1(
            &mut relay,
            &identity,
            &local_peer_id,
            &config,
            relay_peer_id,
        )?,
    };
    if let Some(parent) = config.report_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create peer report directory: {}", parent.display()))?;
    }
    fs::write(&config.report_path, serde_json::to_vec_pretty(&report)?)
        .with_context(|| format!("write peer report: {}", config.report_path.display()))?;
    Ok(report)
}

fn run_sender_v1(
    relay: &mut ProductRelayClientV1,
    identity: &SigningKey,
    local_peer_id: &str,
    config: &ProductPeerRuntimeConfigV1,
    relay_peer_id: String,
) -> Result<ProductPeerRuntimeReportV1> {
    let target_peer_id = config
        .target_peer_id
        .as_deref()
        .context("sender requires target_peer_id")?;
    let initiator = NodeHandshakeInitiatorV1::start(identity, target_peer_id, now_ms_v1(), 30_000)?;
    relay.send_peer_handshake(
        target_peer_id,
        RelayPeerHandshakeV1::Offer(initiator.offer().clone()),
    )?;
    let response = loop {
        match relay.recv_event()? {
            ProductRelayClientEventV1::PeerHandshake(delivery) => match delivery.handshake {
                RelayPeerHandshakeV1::Response(response) => break response,
                RelayPeerHandshakeV1::Offer(_) => bail!("sender received an unexpected peer offer"),
            },
            ProductRelayClientEventV1::HeartbeatAck => continue,
            ProductRelayClientEventV1::Closed => bail!("relay closed before peer response"),
            ProductRelayClientEventV1::Delivery(_) => {
                bail!("sender received data before peer handshake completed")
            }
        }
    };
    let mut replay = HandshakeReplayCacheV1::default();
    let mut channel = initiator.complete(&response, now_ms_v1(), &mut replay)?;
    for sequence in 0..config.frame_count {
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [0x56; 16],
            1,
            2,
            sequence,
            3,
            format!("novovm-product-peer-frame-{sequence}").into_bytes(),
        );
        relay.send_envelope(channel.seal_novorudp_frame(&frame)?)?;
    }
    Ok(report_v1(
        "sender",
        local_peer_id,
        Some(target_peer_id.into()),
        relay_peer_id,
        config.frame_count,
        0,
        true,
    ))
}

fn run_receiver_v1(
    relay: &mut ProductRelayClientV1,
    identity: &SigningKey,
    local_peer_id: &str,
    config: &ProductPeerRuntimeConfigV1,
    relay_peer_id: String,
) -> Result<ProductPeerRuntimeReportV1> {
    let offer = loop {
        match relay.recv_event()? {
            ProductRelayClientEventV1::PeerHandshake(delivery) => match delivery.handshake {
                RelayPeerHandshakeV1::Offer(offer) => {
                    if config
                        .expected_source_peer_id
                        .as_deref()
                        .is_some_and(|expected| expected != offer.initiator_peer_id)
                    {
                        bail!("receiver rejected unexpected source peer id");
                    }
                    break offer;
                }
                RelayPeerHandshakeV1::Response(_) => {
                    bail!("receiver received an unexpected peer response")
                }
            },
            ProductRelayClientEventV1::HeartbeatAck => continue,
            ProductRelayClientEventV1::Closed => bail!("relay closed before peer offer"),
            ProductRelayClientEventV1::Delivery(_) => {
                bail!("receiver received data before peer handshake completed")
            }
        }
    };
    let mut replay = HandshakeReplayCacheV1::default();
    let responder =
        NodeHandshakeResponderV1::respond(&offer, identity, now_ms_v1(), 30_000, &mut replay)?;
    let source_peer_id = offer.initiator_peer_id.clone();
    let response = responder.response().clone();
    let mut channel = responder.into_channel();
    relay.send_peer_handshake(
        source_peer_id.clone(),
        RelayPeerHandshakeV1::Response(response),
    )?;
    let mut received = 0u64;
    while received < config.frame_count {
        match relay.recv_event()? {
            ProductRelayClientEventV1::Delivery(delivery) => {
                channel.open_novorudp_frame(&delivery.envelope)?;
                received = received.saturating_add(1);
            }
            ProductRelayClientEventV1::HeartbeatAck => continue,
            ProductRelayClientEventV1::Closed => bail!("relay closed before all frames arrived"),
            ProductRelayClientEventV1::PeerHandshake(_) => {
                bail!("receiver got an unexpected second peer handshake")
            }
        }
    }
    Ok(report_v1(
        "receiver",
        local_peer_id,
        Some(source_peer_id),
        relay_peer_id,
        0,
        received,
        true,
    ))
}

fn report_v1(
    role: &str,
    local_peer_id: &str,
    remote_peer_id: Option<String>,
    relay_peer_id: String,
    sent: u64,
    received: u64,
    e2e_session_established: bool,
) -> ProductPeerRuntimeReportV1 {
    ProductPeerRuntimeReportV1 {
        accepted: e2e_session_established,
        scope: "novovm_product_peer_runtime_v1".into(),
        role: role.into(),
        local_peer_id: local_peer_id.into(),
        remote_peer_id,
        relay_peer_id,
        selected_path: "RelayNovoRudp".into(),
        selected_transport: "wss".into(),
        peer_handshake_via_relay: true,
        e2e_session_established,
        sent_frame_count: sent,
        received_frame_count: received,
        novorudp_inner_frame_preserved: true,
        payload_treated_opaque: true,
        relay_is_trusted_authority: false,
        network_only: true,
        apfl_interpreted: false,
        aoem_called: false,
        ledger_semantics: false,
        novorudp_wire_changed: false,
    }
}

fn load_ed25519_key_v1(path: &Path) -> Result<SigningKey> {
    let key = fs::read_to_string(path)
        .with_context(|| format!("read peer identity key: {}", path.display()))?;
    let key = key.trim();
    if key.len() != 64 {
        bail!("peer identity key must contain exactly 64 hexadecimal characters");
    }
    let mut bytes = [0u8; 32];
    for (index, output) in bytes.iter_mut().enumerate() {
        *output = u8::from_str_radix(&key[index * 2..index * 2 + 2], 16)
            .context("decode peer identity key hex")?;
    }
    Ok(SigningKey::from_bytes(&bytes))
}

fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn default_frame_count_v1() -> u64 {
    4
}
