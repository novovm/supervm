//! Actual bounded forwarding on the existing authenticated framed carriers.
//! Relays decrypt only hop envelopes, never the original end-to-end payload.
//! One circuit per original stream; no daemon, route registry or business queue.
use crate::{
    bound_session::{
        authenticate_structured_bound_session_observed,
        authenticate_structured_service_client_observed, ServicePeerPolicyV1,
    },
    carrier_stream::{CarrierReadHalfV1, CarrierStreamV1, CarrierWriteHalfV1},
    duplex::{decode_iroh_envelope_v1, encode_iroh_envelope_v1, HandshakeReplayCacheV1},
    novorudp::{
        carrier::VerifiedCarrierRoute,
        wire_budget::{NativeSendReservation, NativeWireBudget},
        NovoRudpTransportFrameKindV0 as Kind, NovoRudpTransportFrameV0 as Frame,
    },
    product_overlay::{
        TransportIdentity, MLDSA65_PUBLIC_KEY_BYTES, PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1,
    },
    transport_binding::BoundSecureChannelV1,
};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub const ENTRY: &[u8; 8] = b"NVNETH01";
const OPEN: &[u8; 8] = b"NVHOPEN1";
const READY: &[u8; 8] = b"NVHREADY";
const DATA: &[u8; 8] = b"NVHDATA1";
const MAX_INNER: usize = 12 * 1024;
const MAX_PATH: usize = 3;
mod e2e;
pub use e2e::VerifiedRelaySession;
#[cfg(test)]
mod tests;
pub type HopBudget = Arc<Mutex<NativeWireBudget>>;
#[derive(Debug, thiserror::Error)]
#[error("native relay protocol rejected: {0}")]
pub struct RelayProtocolRejected(pub(crate) &'static str);
fn wall() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

/// Original route identities, not IP hints or a declaration of delivery.
#[derive(Clone)]
pub struct HopRoute {
    origin: Vec<u8>,
    path: Vec<Vec<u8>>,
    context: [u8; 32],
    circuit: [u8; 16],
    expires: u64,
    cursor: usize,
}
impl HopRoute {
    pub fn new(origin: Vec<u8>, path: Vec<Vec<u8>>, context: [u8; 32]) -> Result<Self> {
        let route = Self {
            origin,
            path,
            context,
            circuit: rand::random(),
            expires: wall()?
                .checked_add(30_000)
                .context("relay setup clock overflow")?,
            cursor: 0,
        };
        route.validate(wall()?)?;
        Ok(route)
    }
    fn validate(&self, now: u64) -> Result<()> {
        ensure!(
            (1..=MAX_PATH).contains(&self.path.len())
                && self.cursor < self.path.len()
                && self.expires > now
                // Original future-issuance skew only; expiration stays strict.
                && self.expires <= now.saturating_add(30_000 + PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1)
                && self.circuit != [0; 16],
            "relay path/setup budget"
        );
        let keys = std::iter::once(&self.origin)
            .chain(self.path.iter())
            .collect::<Vec<_>>();
        ensure!(
            keys.iter()
                .all(|k| k.len() == MLDSA65_PUBLIC_KEY_BYTES && k.iter().any(|b| *b != 0))
                && keys.iter().collect::<BTreeSet<_>>().len() == keys.len(),
            "relay identity width or route loop"
        );
        Ok(())
    }
    pub fn digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"NOVONET/native-hop-route/v1\0");
        h.update(self.circuit);
        h.update(self.context);
        h.update(self.expires.to_be_bytes());
        h.update(&self.origin);
        h.update([self.path.len() as u8]);
        for key in &self.path {
            h.update(key);
        }
        h.finalize().into()
    }
    pub fn next(&self) -> Option<&[u8]> {
        self.path.get(self.cursor + 1).map(Vec::as_slice)
    }
    pub fn origin(&self) -> &[u8] {
        &self.origin
    }
    pub fn target(&self) -> &[u8] {
        self.path.last().expect("validated path").as_slice()
    }
    pub fn forwarded(&self) -> Result<Self> {
        let mut route = self.clone();
        route.cursor += 1;
        route.validate(wall()?)?;
        Ok(route)
    }
    fn encode(&self) -> Result<Vec<u8>> {
        self.validate(wall()?)?;
        let mut out = OPEN.to_vec();
        out.extend_from_slice(&self.circuit);
        out.extend_from_slice(&self.context);
        out.extend_from_slice(&self.expires.to_be_bytes());
        out.extend_from_slice(&[self.path.len() as u8, self.cursor as u8]);
        out.extend_from_slice(&self.origin);
        for key in &self.path {
            out.extend_from_slice(key);
        }
        Ok(out)
    }
    fn decode(bytes: &[u8]) -> Result<Self> {
        const HEADER: usize = 8 + 16 + 32 + 8 + 2;
        ensure!(
            bytes.len() >= HEADER + 2 * MLDSA65_PUBLIC_KEY_BYTES
                && bytes.len() <= HEADER + (MAX_PATH + 1) * MLDSA65_PUBLIC_KEY_BYTES
                && &bytes[..8] == OPEN,
            "relay open shape/version"
        );
        let count = bytes[64] as usize;
        ensure!(
            (1..=MAX_PATH).contains(&count)
                && bytes.len() == HEADER + (count + 1) * MLDSA65_PUBLIC_KEY_BYTES,
            "relay path noncanonical length"
        );
        let route = Self {
            circuit: bytes[8..24].try_into()?,
            context: bytes[24..56].try_into()?,
            expires: u64::from_be_bytes(bytes[56..64].try_into()?),
            cursor: bytes[65] as usize,
            origin: bytes[HEADER..HEADER + MLDSA65_PUBLIC_KEY_BYTES].to_vec(),
            path: bytes[HEADER + MLDSA65_PUBLIC_KEY_BYTES..]
                .chunks_exact(MLDSA65_PUBLIC_KEY_BYTES)
                .map(|k| k.to_vec())
                .collect(),
        };
        route.validate(wall()?)?;
        Ok(route)
    }
}

pub struct HopStream<'a> {
    stream: CarrierStreamV1<'a>,
    channel: Arc<Mutex<BoundSecureChannelV1>>,
    route: HopRoute,
    budget: Option<HopBudget>,
    ready: bool,
}
impl<'a> HopStream<'a> {
    pub fn route(&self) -> &HopRoute {
        &self.route
    }
    pub async fn connect(
        mut stream: CarrierStreamV1<'a>,
        identity: &TransportIdentity,
        route: HopRoute,
        budget: Option<HopBudget>,
    ) -> Result<Self> {
        route.validate(wall()?)?;
        let expected = if route.cursor == 0 {
            &route.origin
        } else {
            &route.path[route.cursor - 1]
        };
        ensure!(
            identity.public_key() == expected,
            "relay outgoing authenticated role differs"
        );
        stream.write_frame(ENTRY).await?;
        let channel = authenticate_structured_bound_session_observed(
            &mut stream,
            identity,
            route.path[route.cursor].clone(),
            true,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await?;
        let mut hop = Self {
            stream,
            channel: Arc::new(Mutex::new(channel)),
            route,
            budget,
            ready: false,
        };
        hop.write(&hop.route.encode()?).await?;
        let mut expected = READY.to_vec();
        expected.extend_from_slice(&hop.route.digest());
        ensure!(
            hop.read().await? == expected,
            RelayProtocolRejected("downstream readiness")
        );
        hop.ready = true;
        Ok(hop)
    }
    /// ENTRY was consumed by the enclosing original stream dispatcher. The
    /// verified immediate sender, not an alleged origin, owns this hop's quota.
    pub async fn accept(
        mut stream: CarrierStreamV1<'a>,
        identity: &TransportIdentity,
        context: [u8; 32],
        budget: HopBudget,
    ) -> Result<Self> {
        let (channel, remote) = authenticate_structured_service_client_observed(
            &mut stream,
            identity,
            ServicePeerPolicyV1::AnyAuthenticated,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await?;
        let binding = stream.binding()?;
        channel.verify_transport_binding(&binding)?;
        // The OPEN has no route digest wrapper yet; bind it with this Finished
        // channel, then use the immutable route commitment on every data frame.
        let wire = stream.read_frame().await?;
        let mut channel = channel;
        let opened = channel
            .open_novorudp_frame(
                &decode_iroh_envelope_v1(&wire).context(RelayProtocolRejected("open envelope"))?,
            )
            .context(RelayProtocolRejected("open authentication"))?;
        ensure!(
            opened == frame(channel.session_id(), opened.payload.clone()),
            RelayProtocolRejected("open frame route")
        );
        let route =
            HopRoute::decode(&opened.payload).context(RelayProtocolRejected("open route"))?;
        ensure!(
            route.context == context && route.path[route.cursor] == identity.public_key(),
            RelayProtocolRejected("context/current hop")
        );
        let previous = if route.cursor == 0 {
            &route.origin
        } else {
            &route.path[route.cursor - 1]
        };
        ensure!(
            *previous == remote,
            RelayProtocolRejected("immediate sender; origin hint grants no authority")
        );
        ensure!(
            budget
                .lock()
                .map_err(|_| anyhow::anyhow!("relay budget poisoned"))?
                .admit_receive(wire.len(), Instant::now()),
            "relay byte budget exhausted"
        );
        Ok(Self {
            stream,
            channel: Arc::new(Mutex::new(channel)),
            route,
            budget: Some(budget),
            ready: false,
        })
    }
    pub async fn confirm(&mut self) -> Result<()> {
        let mut payload = READY.to_vec();
        payload.extend_from_slice(&self.route.digest());
        self.write(&payload).await?;
        self.ready = true;
        Ok(())
    }
    pub async fn write(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_INNER && !bytes.is_empty(),
            "relay inner frame limit"
        );
        let payload = if self.ready {
            let mut p = DATA.to_vec();
            p.extend_from_slice(&self.route.digest());
            p.extend_from_slice(bytes);
            p
        } else {
            bytes.to_vec()
        };
        let wire = {
            let mut channel = self
                .channel
                .lock()
                .map_err(|_| anyhow::anyhow!("relay channel poisoned"))?;
            let session = channel.session_id();
            encode_iroh_envelope_v1(&channel.seal_novorudp_frame(&frame(session, payload))?)?
        };
        write_wire(&mut self.stream, &wire, self.budget.as_ref()).await
    }
    pub async fn read(&mut self) -> Result<Vec<u8>> {
        let wire = self.stream.read_frame().await?;
        charge_receive(self.budget.as_ref(), wire.len())?;
        let mut channel = self
            .channel
            .lock()
            .map_err(|_| anyhow::anyhow!("relay channel poisoned"))?;
        let opened = channel
            .open_novorudp_frame(
                &decode_iroh_envelope_v1(&wire).context(RelayProtocolRejected("data envelope"))?,
            )
            .context(RelayProtocolRejected("data authentication"))?;
        ensure!(
            opened == frame(channel.session_id(), opened.payload.clone()),
            RelayProtocolRejected("data frame route")
        );
        unwrap_payload(opened.payload, self.ready, self.route.digest())
    }
    pub fn first_hop_route(&self) -> Result<VerifiedCarrierRoute> {
        Ok(VerifiedCarrierRoute::from_binding(&self.stream.binding()?))
    }
    pub fn split(&mut self) -> Result<(HopWriteHalf<'_>, HopReadHalf<'_>)> {
        ensure!(self.ready, "relay not confirmed");
        let (write, read) = self.stream.split_relay_continuous()?;
        let shared = Shared {
            channel: self.channel.clone(),
            digest: self.route.digest(),
            budget: self.budget.clone(),
        };
        Ok((
            HopWriteHalf {
                write,
                shared: shared.clone(),
            },
            HopReadHalf { read, shared },
        ))
    }
}

fn frame(session: [u8; 16], payload: Vec<u8>) -> Frame {
    Frame::new(
        Kind::Data,
        session,
        0,
        u64::from_be_bytes(*ENTRY),
        0,
        0,
        payload,
    )
}
fn unwrap_payload(payload: Vec<u8>, ready: bool, digest: [u8; 32]) -> Result<Vec<u8>> {
    if !ready {
        ensure!(payload.len() <= MAX_INNER, "relay control length");
        return Ok(payload);
    }
    ensure!(
        payload.len() > 40
            && payload.len() <= MAX_INNER + 40
            && payload.starts_with(DATA)
            && payload[8..40] == digest,
        RelayProtocolRejected("circuit/data binding")
    );
    Ok(payload[40..].to_vec())
}
fn charge_receive(budget: Option<&HopBudget>, bytes: usize) -> Result<()> {
    if let Some(b) = budget {
        ensure!(
            b.lock()
                .map_err(|_| anyhow::anyhow!("relay budget poisoned"))?
                .admit_receive(bytes, Instant::now()),
            "relay byte budget exhausted"
        );
    }
    Ok(())
}
async fn write_wire(
    stream: &mut CarrierStreamV1<'_>,
    wire: &[u8],
    budget: Option<&HopBudget>,
) -> Result<()> {
    let held = HeldSend::reserve(budget, wire.len())?;
    let result = stream.write_frame(wire).await;
    if let Some(held) = held {
        held.complete(if result.is_ok() { wire.len() } else { 0 }, result.is_err())?;
    }
    result
}

/// Cancellation must resolve a held slot as uncertain, never delivered. Keep
/// the bytes conservatively charged in the original rolling budget.
struct HeldSend {
    budget: HopBudget,
    token: Option<NativeSendReservation>,
}
impl HeldSend {
    fn reserve(budget: Option<&HopBudget>, bytes: usize) -> Result<Option<Self>> {
        let Some(budget) = budget else {
            return Ok(None);
        };
        let token = budget
            .lock()
            .map_err(|_| anyhow::anyhow!("relay budget poisoned"))?
            .reserve_send(bytes, Instant::now())?
            .context("relay byte budget unavailable")?;
        Ok(Some(Self {
            budget: budget.clone(),
            token: Some(token),
        }))
    }
    fn complete(mut self, sent: usize, uncertain: bool) -> Result<()> {
        self.budget
            .lock()
            .map_err(|_| anyhow::anyhow!("relay budget poisoned"))?
            .finish_send(
                self.token.take().expect("held send token"),
                sent,
                uncertain,
                Instant::now(),
            )
    }
}
impl Drop for HeldSend {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            if let Ok(mut budget) = self.budget.lock() {
                let _ = budget.finish_send(token, 0, true, Instant::now());
            }
        }
    }
}

#[derive(Clone)]
struct Shared {
    channel: Arc<Mutex<BoundSecureChannelV1>>,
    digest: [u8; 32],
    budget: Option<HopBudget>,
}
pub struct HopReadHalf<'a> {
    read: CarrierReadHalfV1<'a>,
    shared: Shared,
}
pub struct HopWriteHalf<'a> {
    write: CarrierWriteHalfV1<'a>,
    shared: Shared,
}
impl HopReadHalf<'_> {
    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        let wire = self.read.read_frame().await?;
        charge_receive(self.shared.budget.as_ref(), wire.len())?;
        let mut channel = self
            .shared
            .channel
            .lock()
            .map_err(|_| anyhow::anyhow!("relay channel poisoned"))?;
        let opened = channel
            .open_novorudp_frame(
                &decode_iroh_envelope_v1(&wire).context(RelayProtocolRejected("data envelope"))?,
            )
            .context(RelayProtocolRejected("data authentication"))?;
        ensure!(
            opened == frame(channel.session_id(), opened.payload.clone()),
            RelayProtocolRejected("data frame route")
        );
        unwrap_payload(opened.payload, true, self.shared.digest)
    }
}
impl HopWriteHalf<'_> {
    pub async fn write_frame(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_INNER,
            "relay inner frame limit"
        );
        let wire = {
            let mut channel = self
                .shared
                .channel
                .lock()
                .map_err(|_| anyhow::anyhow!("relay channel poisoned"))?;
            let mut p = DATA.to_vec();
            p.extend_from_slice(&self.shared.digest);
            p.extend_from_slice(bytes);
            let session = channel.session_id();
            encode_iroh_envelope_v1(&channel.seal_novorudp_frame(&frame(session, p))?)?
        };
        let held = HeldSend::reserve(self.shared.budget.as_ref(), wire.len())?;
        let result = self.write.write_frame(&wire).await;
        if let Some(held) = held {
            held.complete(if result.is_ok() { wire.len() } else { 0 }, result.is_err())?;
        }
        result
    }
}

pub async fn bridge(upstream: &mut HopStream<'_>, downstream: &mut HopStream<'_>) -> Result<()> {
    ensure!(
        upstream.route.digest() == downstream.route.digest() && downstream.ready,
        "relay downstream route differs"
    );
    upstream.confirm().await?;
    let (mut uw, mut ur) = upstream.split()?;
    let (mut dw, mut dr) = downstream.split()?;
    let a = async {
        loop {
            let bytes = ur.read_frame().await?;
            dw.write_frame(&bytes).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let b = async {
        loop {
            let bytes = dr.read_frame().await?;
            uw.write_frame(&bytes).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    tokio::try_join!(a, b)?;
    Ok(())
}
