use super::*;
use crate::novorudp::{
    endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
    object_link::NativeSendPolicy,
    NovoRudpPacingProfile, NovoRudpWindowConfig,
};
use crate::product_overlay::peer_id_from_mldsa65_public_key_v1;
use std::{
    net::UdpSocket,
    sync::atomic::{AtomicUsize, Ordering},
};
const STREAM: u64 = 829;
const DOMAIN: [u8; 32] = [73; 32];
struct Echo {
    allowed: String,
    calls: Arc<AtomicUsize>,
}
impl NativeApplicationHandlerV1 for Echo {
    fn request(&self, peer: AuthenticatedApplicationPeerV1, input: Vec<u8>) -> ApplicationFutureV1 {
        let allowed = self.allowed.clone();
        let calls = self.calls.clone();
        Box::pin(async move {
            ensure!(
                peer.is_current() && peer.id() == allowed,
                "application not granted"
            );
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok(input)
        })
    }
}
fn pair() -> Result<(NativeObjectLink, NativeObjectLink)> {
    let sockets = [
        UdpSocket::bind("127.0.0.1:0")?,
        UdpSocket::bind("127.0.0.1:0")?,
    ];
    let addresses = [sockets[0].local_addr()?, sockets[1].local_addr()?];
    let keys = [
        crate::test_identity::key(121),
        crate::test_identity::key(122),
    ];
    let now = Instant::now();
    let mut links = Vec::new();
    for (index, socket) in sockets.into_iter().enumerate() {
        let endpoint = NativeEndpoint::bind(
            socket,
            keys[index].clone(),
            vec![NativePeer {
                key: keys[1 - index].public_key().to_vec(),
                address: addresses[1 - index],
            }],
            NativeHandshakeBudget {
                lifetime: Duration::from_secs(5),
                retry: Duration::from_millis(10),
            },
            1000,
            now,
        )?;
        links.push(NativeObjectLink::new(
            endpoint,
            vec![lane(STREAM, DOMAIN, MAX_WIRE_BYTES * 2)?],
            NativeSendPolicy {
                window: NovoRudpWindowConfig::default(),
                pacing: NovoRudpPacingProfile::default(),
                ttl: DEADLINE,
            },
        )?);
    }
    while links.iter().any(|link| link.session_id(0).is_none()) {
        ensure!(
            now.elapsed() < Duration::from_secs(5),
            "application fixture handshake timeout"
        );
        for link in &mut links {
            let _ = link.poll(1000, Instant::now(), 64)?;
        }
    }
    let client = links.pop().unwrap();
    let server = links.pop().unwrap();
    Ok((server, client))
}
fn dispatcher(allowed: String, calls: Arc<AtomicUsize>) -> NativeApplicationDispatcherV1 {
    NativeApplicationDispatcherV1::new(NativeApplicationRouteV1 {
        stream: STREAM,
        domain: DOMAIN,
        handler: Arc::new(Echo { allowed, calls }),
    })
}
async fn receive(
    server: &mut NativeObjectLink,
    client: &mut NativeObjectLink,
    dispatcher: &mut NativeApplicationDispatcherV1,
    request: &mut NativeApplicationClientV1,
) -> Result<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let now = Instant::now();
            let _ = server.poll(1000, now, 64)?;
            let _ = client.poll(1000, now, 64)?;
            dispatcher.poll(server, now, None)?;
            if let Some(response) = request.poll(client, now)? {
                return Ok(response);
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?
}
#[tokio::test]
async fn real_udp_application_roundtrip_preserves_utf8_and_exact_authenticated_principal(
) -> Result<()> {
    let (mut a, mut b) = pair()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let principal = peer_id_from_mldsa65_public_key_v1(b.local_identity());
    let mut route = dispatcher(principal, calls.clone());
    let mut client = NativeApplicationClientV1::attach(&mut b, 0, STREAM, DOMAIN)?;
    for payload in [
        "B instruction: 读取文件\n".repeat(3000),
        "B tool receipt: 修改已验证".into(),
    ] {
        client.begin(payload.as_bytes(), Instant::now())?;
        assert_eq!(
            receive(&mut a, &mut b, &mut route, &mut client).await?,
            payload.as_bytes()
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(NativeApplicationClientV1::attach(&mut b, 0, STREAM, DOMAIN).is_err());
    Ok(())
}
#[tokio::test]
async fn spoofed_body_cannot_replace_transport_principal_or_retry_rejected_call() -> Result<()> {
    let (mut a, mut b) = pair()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut route = dispatcher("another authorized node".into(), calls.clone());
    let mut client = NativeApplicationClientV1::attach(&mut b, 0, STREAM, DOMAIN)?;
    client.begin(
        br#"{"peerId":"another authorized node","operation":"submit"}"#,
        Instant::now(),
    )?;
    assert!(receive(&mut a, &mut b, &mut route, &mut client)
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(client.begin(b"retry", Instant::now()).is_err());
    Ok(())
}
#[tokio::test]
async fn changed_payload_at_old_sequence_is_not_executed_again() -> Result<()> {
    let (mut a, mut b) = pair()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut route = dispatcher(a.peer_id(0)?.into(), calls.clone());
    let mut client = NativeApplicationClientV1::attach(&mut b, 0, STREAM, DOMAIN)?;
    client.begin(b"first", Instant::now())?;
    receive(&mut a, &mut b, &mut route, &mut client).await?;
    let old = encode(
        REQUEST,
        b.session_id(0).unwrap(),
        0,
        b"different body, old sequence",
        DOMAIN,
    )?;
    assert!(b.try_send(0, STREAM, &old, Instant::now())?);
    for _ in 0..150 {
        let now = Instant::now();
        let _ = a.poll(1000, now, 64)?;
        let _ = b.poll(1000, now, 64)?;
        route.poll(&mut a, now, None)?;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(route.bindings[&0].busy);
    Ok(())
}
#[test]
fn retired_link_deadline_and_oversized_body_fail_closed() -> Result<()> {
    let (a, mut b) = pair()?;
    let mut client = NativeApplicationClientV1::attach(&mut b, 0, STREAM, DOMAIN)?;
    assert!(client
        .begin(&vec![0; MAX_WIRE_BYTES], Instant::now())
        .is_err());
    client.begin(b"original", Instant::now())?;
    assert!(client.poll(&mut b, Instant::now() + DEADLINE).is_err());
    assert!(client.begin(b"replay", Instant::now()).is_err());
    drop(a);
    drop(b);
    assert!(!client.binding.peer.is_current());
    Ok(())
}
