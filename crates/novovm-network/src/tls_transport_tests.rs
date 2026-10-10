use super::*;
use crate::{
    duplex::HandshakeReplayCacheV1,
    transport_binding::{BoundHandshakeInitiatorV1, BoundHandshakeResponderV1},
};
use tokio::{
    io::duplex,
    net::{TcpListener, TcpStream},
};
static SERIAL: Semaphore = Semaphore::const_new(1);

fn control() -> TlsControlV1 {
    TlsControlV1::new(Instant::now() + Duration::from_secs(5), || Ok(())).unwrap()
}
async fn pair<'a>(
    a: &'a TlsScopeV1,
    b: &'a TlsScopeV1,
) -> Result<(TlsStreamV1<'a>, TlsStreamV1<'a>)> {
    let (left, right) = duplex(65536);
    tokio::try_join!(
        a.connect(left, b.endpoint_key()),
        b.accept(right, Some(a.endpoint_key()))
    )
}

#[tokio::test]
async fn real_tcp_tls_mutual_keys_exporter_bound_finished_and_graceful_close() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    let a = TlsScopeV1::new(control())?;
    let b = TlsScopeV1::new(control())?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (client, accepted) = tokio::try_join!(TcpStream::connect(address), listener.accept())?;
    let (mut astream, mut bstream) = tokio::try_join!(
        a.connect(client, b.endpoint_key()),
        b.accept(accepted.0, Some(a.endpoint_key()))
    )?;
    assert_eq!(astream.remote_endpoint_key()?, b.endpoint_key());
    assert_eq!(bstream.remote_endpoint_key()?, a.endpoint_key());
    assert_eq!(astream.selected_path()?, "tls-unclassified");
    let ak = SigningKey::from_bytes(&[71; 32]);
    let bk = SigningKey::from_bytes(&[72; 32]);
    let now = 1000;
    let initiator = BoundHandshakeInitiatorV1::start(
        &ak,
        bk.verifying_key().to_bytes(),
        astream.binding()?,
        now,
        1000,
    )?;
    astream
        .write_frame(&initiator.offer().encode_json()?)
        .await?;
    let offer = crate::transport_binding::BoundNodeHandshakeOfferV1::decode_json(
        &bstream.read_frame().await?,
    )?;
    let mut replay = HandshakeReplayCacheV1::new(8);
    let responder = BoundHandshakeResponderV1::respond(
        &offer,
        &bk,
        ak.verifying_key().to_bytes(),
        bstream.binding()?,
        now,
        1000,
        &mut replay,
    )?;
    bstream
        .write_frame(&responder.response().encode_json()?)
        .await?;
    let response = crate::transport_binding::BoundNodeHandshakeResponseV1::decode_json(
        &astream.read_frame().await?,
    )?;
    let mut apending = initiator.complete(&response, now, &mut HandshakeReplayCacheV1::new(8))?;
    let mut bpending = responder.into_pending();
    let af = apending.make_finished(now)?;
    let bf = bpending.make_finished(now)?;
    astream.write_frame(&serde_json::to_vec(&af)?).await?;
    bstream.write_frame(&serde_json::to_vec(&bf)?).await?;
    apending.verify_finished(&serde_json::from_slice(&astream.read_frame().await?)?, now)?;
    bpending.verify_finished(&serde_json::from_slice(&bstream.read_frame().await?)?, now)?;
    apending.into_channel(now)?;
    bpending.into_channel(now)?;
    tokio::try_join!(astream.finish(), bstream.finish())?;
    assert!(astream.binding().is_err());
    assert!(bstream.read_frame().await.is_err());
    Ok(())
}

#[tokio::test]
async fn bidirectional_full_size_frames_do_not_serialize_pending_read_and_write() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    let a = TlsScopeV1::new(control())?;
    let b = TlsScopeV1::new(control())?;
    let (mut left, mut right) = pair(&a, &b).await?;
    let a_bytes: Vec<u8> = (0..TLS_MAX_FRAME_V1)
        .map(|i| (i * 173 % 251) as u8)
        .collect();
    let b_bytes: Vec<u8> = (0..TLS_MAX_FRAME_V1)
        .map(|i| (i * 191 % 241) as u8)
        .collect();
    {
        let (mut aw, mut ar) = left.split_io()?;
        let (mut bw, mut br) = right.split_io()?;
        let ((), got_b, (), got_a) = tokio::try_join!(
            aw.write_frame(&a_bytes),
            ar.read_frame(),
            bw.write_frame(&b_bytes),
            br.read_frame()
        )?;
        assert_eq!(got_a, a_bytes);
        assert_eq!(got_b, b_bytes);
    }
    tokio::try_join!(left.finish(), right.finish())?;
    Ok(())
}

#[tokio::test]
async fn wrong_pin_missing_client_identity_and_alpn_never_produce_binding() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    let a = TlsScopeV1::new(control())?;
    let b = TlsScopeV1::new(control())?;
    let wrong = SigningKey::from_bytes(&[21; 32]).verifying_key().to_bytes();
    let (left, right) = duplex(65536);
    let (client, server) = tokio::join!(
        a.connect(left, wrong),
        b.accept(right, Some(a.endpoint_key()))
    );
    assert!(client.is_err());
    assert!(server.is_err());
    // Cases: missing client proof, wrong ALPN, and the expected public key
    // advertised with a CertificateVerify made by a different private key.
    for case in 0..3 {
        let (left, right) = duplex(65536);
        let mut config = if case == 0 {
            ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::aws_lc_rs::default_provider(),
            ))
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(RawKeyVerifier {
                expected: Some(b.endpoint_key()),
            }))
            .with_no_client_auth()
        } else {
            (*a.client_config(b.endpoint_key())?).clone()
        };
        if case == 2 {
            let mut spki = ED25519_SPKI_PREFIX.to_vec();
            spki.extend_from_slice(&a.endpoint_key());
            let counterfeit = Arc::new(CertifiedKey::new(
                vec![CertificateDer::from(spki.clone())],
                Arc::new(TransportSigner {
                    key: Arc::new(SigningKey::from_bytes(&[73; 32])),
                    spki,
                }),
            ));
            config.client_auth_cert_resolver =
                Arc::new(AlwaysResolvesClientRawPublicKeys::new(counterfeit));
        }
        config.alpn_protocols = vec![if case == 1 {
            b"wrong-alpn".to_vec()
        } else {
            BOUND_TRANSPORT_ALPN_V1.to_vec()
        }];
        let server_name = ServerName::try_from("bound-carrier.invalid")?;
        let client = async {
            let tls = TlsConnector::from(Arc::new(config))
                .connect(server_name, left)
                .await;
            if let Ok(mut tls) = tls {
                let mut bytes = [0; 1];
                let _ = tls.read(&mut bytes).await;
            }
        };
        let (_, server) = tokio::join!(client, b.accept(right, Some(a.endpoint_key())));
        assert!(server.is_err());
        assert!(!server.err().unwrap().is::<TlsStreamIoFailureV1>());
    }
    Ok(())
}

#[tokio::test]
async fn a_real_exporter_from_one_connection_cannot_authenticate_a_second_connection() -> Result<()>
{
    let _serial = SERIAL.acquire().await?;
    let a = TlsScopeV1::new(control())?;
    let b = TlsScopeV1::new(control())?;
    let (first, _peer) = pair(&a, &b).await?;
    let (_second, second_peer) = pair(&a, &b).await?;
    let ak = SigningKey::from_bytes(&[31; 32]);
    let bk = SigningKey::from_bytes(&[32; 32]);
    let start = BoundHandshakeInitiatorV1::start(
        &ak,
        bk.verifying_key().to_bytes(),
        first.binding()?,
        1000,
        1000,
    )?;
    assert!(BoundHandshakeResponderV1::respond(
        start.offer(),
        &bk,
        ak.verifying_key().to_bytes(),
        second_peer.binding()?,
        1000,
        1000,
        &mut HandshakeReplayCacheV1::new(8)
    )
    .is_err());
    Ok(())
}

#[tokio::test]
async fn cancelling_a_partial_frame_drops_real_io_and_poisons_both_halves() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    let a = TlsScopeV1::new(control())?;
    let b = TlsScopeV1::new(control())?;
    let (mut left, mut right) = pair(&a, &b).await?;
    // Test-only peer intentionally writes a real partial plaintext frame through TLS.
    right.tls.write_all(&8u32.to_be_bytes()).await?;
    right.tls.write_all(&[1, 2]).await?;
    right.tls.flush().await?;
    {
        let (mut writer, mut reader) = left.split_io()?;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), reader.read_frame())
                .await
                .is_err()
        );
        assert!(writer.write_frame(b"cannot-reuse").await.is_err());
        assert!(reader.read_frame().await.is_err());
    }
    assert!(left.state.io.0.lock().unwrap().is_none());
    assert!(left.finish().await.is_err());
    let error = right.read_frame().await.unwrap_err();
    assert!(error.is::<TlsStreamIoFailureV1>());
    Ok(())
}

#[tokio::test]
async fn frame_limit_trailing_data_and_tls_corruption_are_not_retryable_io() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    let a = TlsScopeV1::new(control())?;
    let b = TlsScopeV1::new(control())?;
    let (mut left, mut right) = pair(&a, &b).await?;
    right
        .tls
        .write_all(&((TLS_MAX_FRAME_V1 + 1) as u32).to_be_bytes())
        .await?;
    right.tls.flush().await?;
    let error = left.read_frame().await.unwrap_err();
    assert!(!error.is::<TlsStreamIoFailureV1>());
    drop((left, right));
    let (mut left, mut right) = pair(&a, &b).await?;
    right.write_frame(b"trailing").await?;
    let error = left.finish().await.unwrap_err();
    assert!(!error.is::<TlsStreamIoFailureV1>());
    drop((left, right));
    let (mut left, right) = pair(&a, &b).await?;
    let mut raw = right.state.io.clone();
    // A TLS 1.3 application-data record with invalid encrypted contents.
    raw.write_all(&[23, 3, 3, 0, 17]).await?;
    raw.write_all(&[0; 17]).await?;
    raw.flush().await?;
    let error = left.read_frame().await.unwrap_err();
    assert!(!error.is::<TlsStreamIoFailureV1>());
    Ok(())
}

#[tokio::test]
async fn revocation_preserves_authority_error_and_closes_owned_transport() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    #[derive(Debug, thiserror::Error)]
    #[error("test authority revoked")]
    struct Revoked;
    let active = Arc::new(AtomicBool::new(true));
    let check = active.clone();
    let a = TlsScopeV1::new(TlsControlV1::new(
        Instant::now() + Duration::from_secs(5),
        move || {
            if check.load(Ordering::Acquire) {
                Ok(())
            } else {
                Err(Revoked.into())
            }
        },
    )?)?;
    let b = TlsScopeV1::new(control())?;
    let (mut left, mut right) = pair(&a, &b).await?;
    let stop = async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        active.store(false, Ordering::Release);
    };
    let (result, ()) = tokio::join!(left.read_frame(), stop);
    let error = result.unwrap_err();
    assert!(error.is::<Revoked>());
    assert!(!error.is::<TlsStreamIoFailureV1>());
    assert!(left.state.io.0.lock().unwrap().is_none());
    assert!(right
        .read_frame()
        .await
        .unwrap_err()
        .is::<TlsStreamIoFailureV1>());
    Ok(())
}

#[tokio::test]
async fn bare_eof_cannot_complete_tls_finish_and_scope_budget_caps_handshake() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    let a = TlsScopeV1::new(control())?;
    let b = TlsScopeV1::new(control())?;
    let (mut left, right) = pair(&a, &b).await?;
    drop(right);
    assert!(left.finish().await.is_err());
    drop(left);
    let c = TlsScopeV1::new(TlsControlV1::new(
        Instant::now() + Duration::from_millis(50),
        || Ok(()),
    )?)?;
    let (left, _silent) = duplex(65536);
    let started = Instant::now();
    let error = c
        .connect(left, b.endpoint_key())
        .await
        .err()
        .context("unexpected successful handshake")?;
    assert!(!error.is::<TlsStreamIoFailureV1>());
    assert!(started.elapsed() < Duration::from_secs(1));
    Ok(())
}

#[test]
fn strict_raw_key_shape_rejects_noncanonical_key_types_and_weak_keys() {
    let mut bytes = ED25519_SPKI_PREFIX.to_vec();
    bytes.extend_from_slice(&SigningKey::from_bytes(&[42; 32]).verifying_key().to_bytes());
    assert!(public_key(&bytes).is_ok());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(public_key(&trailing).is_err());
    bytes[8] = 0x6e;
    assert!(public_key(&bytes).is_err());
    let mut weak = ED25519_SPKI_PREFIX.to_vec();
    weak.extend_from_slice(&[0; 32]);
    assert!(public_key(&weak).is_err());
    let key = SigningKey::from_bytes(&[43; 32]).verifying_key().to_bytes();
    let point = CompressedEdwardsY(key).decompress().unwrap();
    let mixed = (point + curve25519_dalek::constants::EIGHT_TORSION[1])
        .compress()
        .to_bytes();
    assert!(!VerifyingKey::from_bytes(&mixed).unwrap().is_weak());
    assert!(check_public_key(&mixed).is_err());
    let mut noncanonical = [0xff; 32];
    noncanonical[0] = 0xee;
    noncanonical[31] = 0x7f;
    assert!(check_public_key(&noncanonical).is_err());
}

#[tokio::test]
async fn idle_expiry_and_revocation_close_real_io_without_polling_local_stream() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    for revoke in [false, true] {
        let active = Arc::new(AtomicBool::new(true));
        let permission = active.clone();
        let a = TlsScopeV1::new(TlsControlV1::new(
            Instant::now() + Duration::from_millis(if revoke { 5000 } else { 500 }),
            move || {
                ensure!(permission.load(Ordering::Acquire), "test authority revoked");
                Ok(())
            },
        )?)?;
        let b = TlsScopeV1::new(control())?;
        let (left, mut right) = pair(&a, &b).await?;
        if revoke {
            active.store(false, Ordering::Release);
        }
        // Do not call binding/read/write on left: its independently scheduled
        // lifetime supervisor alone must close the real transport at the peer.
        let error = tokio::time::timeout(Duration::from_secs(2), right.read_frame())
            .await?
            .unwrap_err();
        assert!(error.is::<TlsStreamIoFailureV1>());
        assert!(left.state.io.0.lock().unwrap().is_none());
        assert!(left.binding().is_err());
    }
    Ok(())
}

#[tokio::test]
async fn dropping_stream_closes_io_and_releases_supervisor_and_admission() -> Result<()> {
    let _serial = SERIAL.acquire().await?;
    let a = TlsScopeV1::new(control())?;
    let b = TlsScopeV1::new(control())?;
    for _ in 0..TLS_MAX_CONNECTIONS_V1 + 1 {
        let (left, mut right) = pair(&a, &b).await?;
        let state = Arc::downgrade(&left.state);
        drop(left);
        assert!(right
            .read_frame()
            .await
            .unwrap_err()
            .is::<TlsStreamIoFailureV1>());
        drop(right);
        tokio::time::timeout(Duration::from_secs(1), async {
            while state.upgrade().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
    }
    Ok(())
}
