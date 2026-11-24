//! Authenticated endpoint observation on the existing NovoRUDP data stream.
use crate::novorudp::{
    carrier::NativeRoute,
    endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer, NativeReceivedFrame},
    NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
};
use crate::product_overlay::TransportIdentity;
use anyhow::ensure;
use anyhow::{bail, Result};
use rand::{rngs::OsRng, RngCore};
use std::{
    net::{IpAddr, SocketAddr, UdpSocket},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const OBSERVATION_BUDGET: Duration = Duration::from_secs(3);
pub(crate) const OBSERVATION_LIFETIME_MS: u64 = 80_000;

/// Temporarily borrow the original bound socket before the mailbox loop takes
/// ownership. None of these sessions survive this call or carry mailbox work.
pub(crate) async fn observe_bound_socket_v2(
    socket: &UdpSocket,
    identity: TransportIdentity,
    peers: Vec<NativePeer>,
    max_bytes_per_minute: u64,
    authority: impl Fn() -> Result<()>,
) -> Result<Option<SocketAddr>> {
    ensure!(
        peers.len() == 2,
        "two independently pinned observers required"
    );
    ensure!(
        peers[0].key != peers[1].key && peers[0].address.ip() != peers[1].address.ip(),
        "observer identities and IPs must differ"
    );
    let start = Instant::now();
    let clock = || -> Result<u64> {
        Ok(SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .try_into()?)
    };
    authority()?;
    let mut endpoint = NativeEndpoint::connect_clients(
        socket.try_clone()?,
        identity,
        peers,
        NativeHandshakeBudget {
            lifetime: OBSERVATION_BUDGET,
            retry: Duration::from_millis(350),
        },
        clock()?,
        start,
    )?;
    endpoint.set_wire_budget_v1(max_bytes_per_minute, start)?;
    let probes = [
        EndpointObservationProbeV2::default(),
        EndpointObservationProbeV2::default(),
    ];
    let mut sent = [None, None];
    let mut address = [None, None];
    while start.elapsed() < OBSERVATION_BUDGET {
        authority()?;
        for frame in endpoint.poll(clock()?, Instant::now(), 16)?.frames {
            let peer = frame.peer();
            if let Some(session) = sent[peer] {
                if endpoint.session_id(peer) == Some(session) {
                    address[peer] = probes[peer]
                        .observed(&frame, peer, session)
                        .filter(|address| is_public_observed_endpoint_v2(*address))
                        .or(address[peer]);
                }
            }
        }
        if let [Some(first), Some(second)] = address {
            authority()?;
            // Destination-dependent mappings are not reusable signed routes.
            return Ok((first == second).then_some(first));
        }
        for peer in 0..2 {
            if let Some(session) = endpoint.session_id(peer) {
                if sent[peer].is_none()
                    && endpoint.try_send(peer, &probes[peer].request(session))?
                {
                    sent[peer] = Some(session);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    Ok(None)
}

pub const ENDPOINT_OBSERVATION_STREAM: u64 = 1;
const MAGIC: &[u8; 8] = b"NVNATP02";

pub struct EndpointObservationProbeV2 {
    nonce: [u8; 16],
}

impl Default for EndpointObservationProbeV2 {
    fn default() -> Self {
        let mut nonce = [0; 16];
        OsRng.fill_bytes(&mut nonce);
        Self { nonce }
    }
}

impl EndpointObservationProbeV2 {
    pub fn request(&self, session: [u8; 16]) -> NovoRudpTransportFrameV0 {
        let mut payload = MAGIC.to_vec();
        payload.push(1);
        payload.extend_from_slice(&self.nonce);
        NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            session,
            ENDPOINT_OBSERVATION_STREAM,
            self.object(),
            0,
            0,
            payload,
        )
    }

    fn object(&self) -> u64 {
        u64::from_be_bytes(self.nonce[..8].try_into().expect("fixed nonce prefix"))
    }

    pub fn observed(
        &self,
        response: &NativeReceivedFrame,
        peer: usize,
        session: [u8; 16],
    ) -> Option<SocketAddr> {
        let frame = response.frame();
        if response.peer() != peer
            || !is_observation(frame)
            || frame.session_id != session
            || frame.object_id != self.object()
            || frame.sequence != 0
            || frame.payload.get(8) != Some(&2)
            || frame.payload.get(9..25) != Some(self.nonce.as_slice())
        {
            return None;
        }
        decode_address(frame.payload.get(25..)?).ok()
    }
}

fn is_observation(frame: &NovoRudpTransportFrameV0) -> bool {
    frame.stream_id == ENDPOINT_OBSERVATION_STREAM
        && frame.kind == NovoRudpTransportFrameKindV0::Data
        && frame.payload.starts_with(MAGIC)
}

pub fn observation_reply_v2(
    endpoint: &NativeEndpoint,
    request: &NativeReceivedFrame,
) -> Option<(SocketAddr, NovoRudpTransportFrameV0)> {
    let frame = request.frame();
    if !is_observation(frame)
        || frame.payload.len() != 25
        || frame.payload[8] != 1
        || endpoint.session_id(request.peer()) != Some(frame.session_id)
    {
        return None;
    }
    let NativeRoute::Udp(source) = endpoint.peer_route(request.peer()).ok()? else {
        return None;
    };
    let mut payload = frame.payload.clone();
    payload[8] = 2;
    encode_address(source, &mut payload);
    Some((
        source,
        NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            frame.session_id,
            ENDPOINT_OBSERVATION_STREAM,
            frame.object_id,
            frame.sequence,
            0,
            payload,
        ),
    ))
}

pub fn is_public_observed_endpoint_v2(address: SocketAddr) -> bool {
    if address.port() == 0 {
        return false;
    }
    match address {
        SocketAddr::V4(address) => {
            let [a, b, c, _] = address.ip().octets();
            !(address.ip().is_private()
                || address.ip().is_link_local()
                || matches!(a, 0 | 127 | 224..=255)
                || a == 100 && (64..=127).contains(&b)
                || a == 192 && matches!((b, c), (0, 0 | 2) | (88, 99))
                || a == 198 && (matches!(b, 18 | 19) || (b, c) == (51, 100))
                || a == 203 && (b, c) == (0, 113))
        }
        SocketAddr::V6(address) => {
            let [a, b, ..] = address.ip().segments();
            address.scope_id() == 0
                && address.flowinfo() == 0
                && (0x2000..=0x3fff).contains(&a)
                && (a, b) != (0x2001, 0x0db8)
                && !(a == 0x3fff && b < 0x1000)
                && !(a == 0x2001 && b < 0x0200)
                && a != 0x2002
        }
    }
}

pub fn encode_address(address: SocketAddr, out: &mut Vec<u8>) {
    match address.ip() {
        IpAddr::V4(ip) => {
            out.push(4);
            out.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            out.push(6);
            out.extend_from_slice(&ip.octets());
        }
    }
    out.extend_from_slice(&address.port().to_be_bytes());
}

pub fn decode_address(bytes: &[u8]) -> Result<SocketAddr> {
    let ip = match bytes {
        [4, a, b, c, d, _, _] => IpAddr::V4([*a, *b, *c, *d].into()),
        value if value.len() == 19 && value[0] == 6 => {
            IpAddr::V6(<[u8; 16]>::try_from(&value[1..17])?.into())
        }
        _ => bail!("invalid authenticated observed endpoint"),
    };
    Ok(SocketAddr::new(
        ip,
        u16::from_be_bytes(bytes[bytes.len() - 2..].try_into()?),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_identity::key;

    #[test]
    fn observer_returns_the_authenticated_socket_port_and_rejects_another_challenge() -> Result<()>
    {
        let localhost = "127.0.0.1:0";
        let socket = UdpSocket::bind(localhost)?;
        let reflected = socket.local_addr()?;
        let remote = UdpSocket::bind(localhost)?;
        let remote_addr = remote.local_addr()?;
        let identity = key(1);
        let observer = key(2);
        let start = Instant::now();
        let wall_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let budget = NativeHandshakeBudget {
            lifetime: Duration::from_secs(3),
            retry: Duration::from_millis(40),
        };
        let mut server =
            NativeEndpoint::listen_clients(remote, observer.clone(), 2, vec![7], budget)?;
        server.enable_endpoint_observer_v2()?;
        let mut client = NativeEndpoint::connect_client(
            socket.try_clone()?,
            identity,
            NativePeer {
                key: observer.public_key().to_vec(),
                address: remote_addr,
            },
            budget,
            wall_ms,
            start,
        )?;
        let probe = EndpointObservationProbeV2::default();
        let different = EndpointObservationProbeV2::default();
        let mut sent = false;
        while start.elapsed() < budget.lifetime {
            let now = Instant::now();
            let wall = wall_ms + start.elapsed().as_millis() as u64;
            assert!(server.poll(wall, now, 16)?.frames.is_empty());
            for frame in client.poll(wall, now, 16)?.frames {
                let session = client.session_id(0).expect("authenticated observer");
                assert_eq!(probe.observed(&frame, 0, session), Some(reflected));
                assert_eq!(different.observed(&frame, 0, session), None);
                assert_eq!(probe.observed(&frame, 1, session), None);
                let mut wrong_session = session;
                wrong_session[0] ^= 1;
                assert_eq!(probe.observed(&frame, 0, wrong_session), None);
                drop(client);
                assert_eq!(socket.local_addr()?, reflected);
                return Ok(());
            }
            if let Some(session) = client.session_id(0) {
                if !sent {
                    sent = client.try_send(0, &probe.request(session))?;
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        bail!("observer did not return authenticated source")
    }

    #[test]
    fn public_observed_endpoint_excludes_reserved_or_unsuitable_addresses() {
        for invalid in [
            "127.0.0.1:1",
            "10.0.0.1:1",
            "100.64.1.1:1",
            "192.0.2.1:1",
            "198.19.1.1:1",
            "203.0.113.1:1",
            "225.0.0.1:1",
            "[::ffff:8.8.8.8]:1",
            "[fe80::1]:1",
            "[2001:db8::1]:1",
            "[3fff:100::1]:1",
            "8.8.8.8:0",
        ] {
            assert!(!is_public_observed_endpoint_v2(invalid.parse().unwrap()));
        }
        for valid in ["8.8.8.8:1234", "[2606:4700:4700::1111]:42"] {
            assert!(is_public_observed_endpoint_v2(valid.parse().unwrap()));
        }
    }
}
