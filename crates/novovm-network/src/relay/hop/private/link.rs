//! The isolated setup and opaque records on the existing NovoRUDP hop carrier.
use super::{OpenedPrivateHop, PrivateEndpoint, PrivateHopKey, PrivateRoute, LAYER};
use crate::relay::hop::{
    authenticate_structured_bound_session_observed,
    authenticate_structured_service_client_observed, charge_receive, decode_iroh_envelope_v1,
    encode_iroh_envelope_v1, frame, unwrap_payload, wall, write_wire, CarrierStreamV1,
    HandshakeReplayCacheV1, HopBudget, HopReadHalf, HopWriteHalf, ServicePeerPolicyV1, Shared,
    TransportIdentity, DATA, MAX_INNER,
};
use anyhow::{ensure, Result};
use chacha20poly1305::ChaCha20Poly1305;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

pub const PRIVATE_HOP_ENTRY: &[u8; 8] = b"NVNETH02";
const ADJACENT_READY: &[u8; 8] = b"NVHRDY02";

pub struct PrivateHopLink<'a> {
    stream: CarrierStreamV1<'a>,
    shared: Shared,
    peer: Vec<u8>,
    ready: bool,
    forward: Option<(Vec<u8>, [u8; 32])>,
}

/// Endpoints use this stream for the native handshake as well as business
/// frames. Its construction does not qualify application discovery or custody.
pub struct PrivateCircuit<'a> {
    link: PrivateHopLink<'a>,
    endpoint: PrivateEndpoint,
}

impl<'a> PrivateCircuit<'a> {
    pub async fn connect(
        stream: CarrierStreamV1<'a>,
        identity: &TransportIdentity,
        route: PrivateRoute,
        budget: Option<HopBudget>,
    ) -> Result<Self> {
        let mut link = PrivateHopLink::connect(
            stream,
            identity,
            &route.first_identity,
            &route.first_layer,
            budget,
        )
        .await?;
        let mut endpoint = route.endpoint;
        endpoint.confirm_terminal(&link.read_record().await?)?;
        Ok(Self { link, endpoint })
    }

    pub async fn accept(mut link: PrivateHopLink<'a>, opened: OpenedPrivateHop) -> Result<Self> {
        let OpenedPrivateHop::Terminal(mut endpoint) = opened else {
            anyhow::bail!("private circuit endpoint requires terminal role");
        };
        link.confirm_terminal().await?;
        link.write_record(&endpoint.terminal_ready()?).await?;
        Ok(Self { link, endpoint })
    }

    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        self.endpoint.receive(&self.link.read_record().await?)
    }

    pub async fn write_frame(&mut self, payload: &[u8]) -> Result<()> {
        self.link.write_record(&self.endpoint.send(payload)?).await
    }

    pub(crate) fn binding(&self) -> Result<crate::transport_binding::VerifiedTransportBinding> {
        self.check_live()?;
        let seed = &*self.endpoint.binding_seed;
        Ok(
            crate::transport_binding::VerifiedTransportBinding::from_private_circuit(
                seed[..32].try_into()?,
                seed[32..64].try_into()?,
                seed[64..].try_into()?,
                self.endpoint.initiator,
            )?,
        )
    }

    pub(crate) fn check_live(&self) -> Result<()> {
        ensure!(
            self.endpoint.ready && self.link.ready,
            "private circuit not confirmed"
        );
        self.link.stream.selected_path()?;
        Ok(())
    }

    #[cfg(any(test, feature = "private-apfl-listener"))]
    pub(crate) fn check_terminal(&self) -> Result<()> {
        ensure!(!self.endpoint.initiator, "APFL requires a terminal circuit");
        self.check_live()
    }

    pub(crate) fn split(&mut self) -> Result<(PrivateWriteHalf<'_>, PrivateReadHalf<'_>)> {
        self.check_live()?;
        let (write, read) = self.link.split()?;
        Ok((
            PrivateWriteHalf {
                write,
                cipher: &self.endpoint.send,
                counter: &mut self.endpoint.sent,
            },
            PrivateReadHalf {
                read,
                cipher: &self.endpoint.receive,
                counter: &mut self.endpoint.received,
            },
        ))
    }

    pub(crate) async fn finish(&mut self) -> Result<()> {
        self.link.stream.finish().await
    }
}

pub(crate) struct PrivateWriteHalf<'a> {
    write: HopWriteHalf<'a>,
    cipher: &'a ChaCha20Poly1305,
    counter: &'a mut u64,
}

impl PrivateWriteHalf<'_> {
    pub(crate) async fn write_frame(&mut self, data: &[u8]) -> Result<()> {
        self.write
            .write_frame(&PrivateEndpoint::seal_record(
                self.cipher,
                self.counter,
                data,
            )?)
            .await
    }
}

pub(crate) struct PrivateReadHalf<'a> {
    read: HopReadHalf<'a>,
    cipher: &'a ChaCha20Poly1305,
    counter: &'a mut u64,
}

impl PrivateReadHalf<'_> {
    pub(crate) async fn read_frame(&mut self) -> Result<Vec<u8>> {
        PrivateEndpoint::open_record(self.cipher, self.counter, &self.read.read_frame().await?)
    }
}

impl<'a> PrivateHopLink<'a> {
    pub(crate) async fn connect_metadata(
        mut stream: CarrierStreamV1<'a>,
        identity: &TransportIdentity,
        pin: &[u8],
        selector: &[u8; 8],
        budget: Option<HopBudget>,
    ) -> Result<Self> {
        stream.write_frame(selector).await?;
        let channel = authenticate_structured_bound_session_observed(
            &mut stream,
            identity,
            pin.to_vec(),
            true,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await?;
        channel.verify_transport_binding(&stream.binding()?)?;
        Ok(Self {
            stream,
            shared: Shared {
                channel: Arc::new(Mutex::new(channel)),
                digest: commitment(selector),
                budget,
            },
            peer: pin.to_vec(),
            ready: true,
            forward: None,
        })
    }

    #[cfg(feature = "private-apfl-listener")]
    pub(crate) async fn accept_metadata(
        mut stream: CarrierStreamV1<'a>,
        identity: &TransportIdentity,
        pins: &[Vec<u8>],
        selector: &[u8; 8],
        budget: HopBudget,
    ) -> Result<Self> {
        let (channel, peer) = authenticate_structured_service_client_observed(
            &mut stream,
            identity,
            ServicePeerPolicyV1::Pinned(pins),
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await?;
        channel.verify_transport_binding(&stream.binding()?)?;
        Ok(Self {
            stream,
            shared: Shared {
                channel: Arc::new(Mutex::new(channel)),
                digest: commitment(selector),
                budget: Some(budget),
            },
            peer,
            ready: true,
            forward: None,
        })
    }

    pub async fn connect(
        mut stream: CarrierStreamV1<'a>,
        identity: &TransportIdentity,
        next: &[u8],
        layer: &[u8],
        budget: Option<HopBudget>,
    ) -> Result<Self> {
        ensure!(
            !stream.is_private()
                && layer.len() <= MAX_INNER
                && layer.starts_with(LAYER)
                && next != identity.public_key(),
            "private outgoing hop setup rejected"
        );
        stream.write_frame(PRIVATE_HOP_ENTRY).await?;
        let channel = authenticate_structured_bound_session_observed(
            &mut stream,
            identity,
            next.to_vec(),
            true,
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await?;
        let mut hop = Self {
            stream,
            shared: Shared {
                channel: Arc::new(Mutex::new(channel)),
                digest: commitment(layer),
                budget,
            },
            peer: next.to_vec(),
            ready: false,
            forward: None,
        };
        hop.write(layer).await?;
        ensure!(
            hop.read().await? == adjacent_ready(hop.shared.digest),
            "private adjacent readiness differs"
        );
        hop.ready = true;
        Ok(hop)
    }

    /// The enclosing carrier owner consumes PRIVATE_HOP_ENTRY and supplies its
    /// own pinned admission list and existing shared byte budget.
    pub async fn accept(
        stream: CarrierStreamV1<'a>,
        identity: &TransportIdentity,
        key: &PrivateHopKey,
        allowed_previous: &[Vec<u8>],
        budget: HopBudget,
    ) -> Result<(Self, OpenedPrivateHop)> {
        Self::accept_rotating(stream, identity, key, None, allowed_previous, budget).await
    }

    pub(crate) async fn accept_rotating(
        mut stream: CarrierStreamV1<'a>,
        identity: &TransportIdentity,
        current: &PrivateHopKey,
        previous: Option<&PrivateHopKey>,
        allowed_previous: &[Vec<u8>],
        budget: HopBudget,
    ) -> Result<(Self, OpenedPrivateHop)> {
        ensure!(
            !stream.is_private()
                && current.identity == identity.public_key()
                && previous.is_none_or(|key| key.identity == current.identity)
                && !allowed_previous.is_empty(),
            "private hop local identity or admission missing"
        );
        let (channel, peer) = authenticate_structured_service_client_observed(
            &mut stream,
            identity,
            ServicePeerPolicyV1::Pinned(allowed_previous),
            &mut HandshakeReplayCacheV1::new(8),
            &mut |_, _| Ok(()),
        )
        .await?;
        channel.verify_transport_binding(&stream.binding()?)?;
        let mut hop = Self {
            stream,
            shared: Shared {
                channel: Arc::new(Mutex::new(channel)),
                digest: [0; 32],
                budget: Some(budget),
            },
            peer,
            ready: false,
            forward: None,
        };
        let layer = hop.read().await?;
        let now = wall()?;
        let opened = current
            .open(&hop.peer, &layer, now)
            .or_else(|error| previous.map_or(Err(error), |key| key.open(&hop.peer, &layer, now)))?;
        hop.shared.digest = commitment(&layer);
        if let OpenedPrivateHop::Forward { next, sealed } = &opened {
            hop.forward = Some((next.clone(), commitment(sealed)));
        }
        Ok((hop, opened))
    }

    async fn confirm_terminal(&mut self) -> Result<()> {
        ensure!(
            self.forward.is_none(),
            "private hop expects a downstream link"
        );
        self.confirm().await
    }

    async fn confirm(&mut self) -> Result<()> {
        ensure!(!self.ready, "private adjacent link already confirmed");
        self.write(&adjacent_ready(self.shared.digest)).await?;
        self.ready = true;
        Ok(())
    }

    pub(crate) async fn finish_metadata(&mut self) -> Result<()> {
        self.stream.finish().await
    }

    pub(crate) async fn read_record(&mut self) -> Result<Vec<u8>> {
        ensure!(self.ready, "private adjacent link not confirmed");
        self.read().await
    }

    pub(crate) async fn write_record(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(self.ready, "private adjacent link not confirmed");
        self.write(bytes).await
    }

    async fn read(&mut self) -> Result<Vec<u8>> {
        let wire = self.stream.read_frame().await?;
        charge_receive(self.shared.budget.as_ref(), wire.len())?;
        let mut channel = self
            .shared
            .channel
            .lock()
            .map_err(|_| anyhow::anyhow!("private link unavailable"))?;
        let opened = channel.open_novorudp_frame(&decode_iroh_envelope_v1(&wire)?)?;
        ensure!(
            opened == frame(channel.session_id(), opened.payload.clone()),
            "private link frame shape"
        );
        unwrap_payload(opened.payload, self.ready, self.shared.digest)
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_INNER,
            "private link frame limit"
        );
        let wire = {
            let mut channel = self
                .shared
                .channel
                .lock()
                .map_err(|_| anyhow::anyhow!("private link unavailable"))?;
            let data = if self.ready {
                [DATA.as_slice(), &self.shared.digest, bytes].concat()
            } else {
                bytes.to_vec()
            };
            let record = frame(channel.session_id(), data);
            encode_iroh_envelope_v1(&channel.seal_novorudp_frame(&record)?)?
        };
        write_wire(&mut self.stream, &wire, self.shared.budget.as_ref()).await
    }

    fn split(&mut self) -> Result<(HopWriteHalf<'_>, HopReadHalf<'_>)> {
        let (write, read) = self.stream.split_relay_continuous()?;
        Ok((
            HopWriteHalf {
                write,
                shared: self.shared.clone(),
            },
            HopReadHalf {
                read,
                shared: self.shared.clone(),
            },
        ))
    }

    pub async fn forward(&mut self, downstream: &mut PrivateHopLink<'_>) -> Result<()> {
        ensure!(
            downstream.ready
                && self
                    .forward
                    .as_ref()
                    .is_some_and(|(next, digest)| *next == downstream.peer
                        && *digest == downstream.shared.digest),
            "private next-hop peer or layer differs"
        );
        self.confirm().await?;
        let (mut up_write, mut up_read) = self.split()?;
        let (mut down_write, mut down_read) = downstream.split()?;
        tokio::try_join!(
            pipe(&mut up_read, &mut down_write),
            pipe(&mut down_read, &mut up_write),
        )?;
        Ok(())
    }
}

fn commitment(layer: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"NOVONET/private-hop-adjacent/v1\0");
    hash.update(layer);
    hash.finalize().into()
}

fn adjacent_ready(digest: [u8; 32]) -> Vec<u8> {
    [ADJACENT_READY.as_slice(), &digest].concat()
}

async fn pipe(read: &mut HopReadHalf<'_>, write: &mut HopWriteHalf<'_>) -> Result<()> {
    loop {
        write.write_frame(&read.read_frame().await?).await?;
    }
}
