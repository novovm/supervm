//! Real TLS over in-memory reliable IO; no timer scaling or relaxed bounds.
use super::*;

async fn pair<'a>(
    a: &'a TlsScopeV1,
    b: &'a TlsScopeV1,
) -> Result<(TlsStreamV1<'a>, TlsStreamV1<'a>)> {
    let (left, right) = tokio::io::duplex(65536);
    tokio::try_join!(
        a.connect(left, b.endpoint_key()),
        b.accept(right, Some(a.endpoint_key()))
    )
}
fn continuous() -> Result<TlsScopeV1> {
    TlsScopeV1::new_continuous_v4(|| Ok(()))
}

#[tokio::test]
async fn native_tls_idle_survives_initial_budget_and_preserves_mode() -> Result<()> {
    let _serial = TLS_TEST_SERIAL.acquire().await?;
    let a = continuous()?;
    let b = continuous()?;
    let (mut left, mut right) = pair(&a, &b).await?;
    {
        let (mut aw, mut ar) = left.split_native_continuous_v7()?;
        let (mut bw, mut br) = right.split_native_continuous_v7()?;
        assert!(aw.write_frame(&[]).await.is_err());
        assert!(aw.write_frame(&[0; 1201]).await.is_err());
        let receive = async {
            assert_eq!(ar.read_frame().await?, b"after idle");
            aw.write_frame(b"still live").await
        };
        let send = async {
            tokio::time::sleep(SERVICE_REQUEST_TIMEOUT + Duration::from_millis(100)).await;
            bw.write_frame(b"after idle").await?;
            assert_eq!(br.read_frame().await?, b"still live");
            Ok::<_, anyhow::Error>(())
        };
        tokio::try_join!(receive, send)?;
    }
    assert!(left.state.request_deadline.lock().unwrap().is_none());
    assert!(left.binding().is_ok());
    assert!(left.split_native_continuous_v7().is_err());
    assert!(left.split_io().is_err());
    assert!(left.admit_continuous_service_v4().is_err());
    assert!(left.begin_service_request_v4().is_err());
    assert!(left.complete_service_request_v6().is_err());
    assert!(left.write_frame(b"legacy").await.is_err());
    assert!(left.read_frame().await.is_err());
    assert!(left.finish().await.is_err());
    Ok(())
}

#[tokio::test]
async fn native_tls_partial_frame_has_fixed_deadline() -> Result<()> {
    let _serial = TLS_TEST_SERIAL.acquire().await?;
    let a = continuous()?;
    let b = continuous()?;
    let (mut left, mut right) = pair(&a, &b).await?;
    // Peer remains legitimately idle while providing one plaintext prefix byte.
    right.admit_continuous_service_v4()?;
    right.complete_service_request_v6()?;
    right.tls.write_all(&[0]).await?;
    right.tls.flush().await?;
    let (writer, mut reader) = left.split_native_continuous_v7()?;
    let started = Instant::now();
    let error = reader.read_frame().await.unwrap_err();
    assert!(started.elapsed() >= SERVICE_REQUEST_TIMEOUT);
    assert!(
        error.to_string().contains("native TLS frame deadline"),
        "{error:#}"
    );
    drop(reader);
    drop(writer);
    assert!(left.state.failed.load(Ordering::Acquire));
    assert!(left.state.io.0.lock().unwrap().is_none());
    Ok(())
}

#[tokio::test]
async fn native_tls_cancellation_and_oversize_are_terminal() -> Result<()> {
    let _serial = TLS_TEST_SERIAL.acquire().await?;
    for oversized in [false, true] {
        let a = continuous()?;
        let b = continuous()?;
        let (mut left, mut right) = pair(&a, &b).await?;
        {
            let (mut writer, mut reader) = left.split_native_continuous_v7()?;
            if oversized {
                right.tls.write_all(&1201u32.to_be_bytes()).await?;
                right.tls.flush().await?;
                assert!(reader
                    .read_frame()
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("native TLS frame limit"));
            } else {
                right.tls.write_all(&[0]).await?;
                right.tls.flush().await?;
                assert!(
                    tokio::time::timeout(Duration::from_millis(30), reader.read_frame())
                        .await
                        .is_err()
                );
            }
            assert!(writer.write_frame(b"must fail").await.is_err());
        }
        assert!(left.binding().is_err());
        assert!(left.state.io.0.lock().unwrap().is_none());
    }
    Ok(())
}

#[tokio::test]
async fn native_tls_idle_revocation_and_finite_scope_rejection() -> Result<()> {
    let _serial = TLS_TEST_SERIAL.acquire().await?;
    let live = Arc::new(AtomicBool::new(true));
    let check = live.clone();
    let a = TlsScopeV1::new_continuous_v4(move || {
        ensure!(check.load(Ordering::Acquire), "native test revoked");
        Ok(())
    })?;
    let b = continuous()?;
    let (mut left, _right) = pair(&a, &b).await?;
    {
        let (mut writer, mut reader) = left.split_native_continuous_v7()?;
        let revoke = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            live.store(false, Ordering::Release);
        };
        let (result, ()) = tokio::join!(reader.read_frame(), revoke);
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("native test revoked"));
        assert!(writer.write_frame(b"must fail").await.is_err());
    }
    assert!(left.state.io.0.lock().unwrap().is_none());
    drop(left);
    drop(_right);
    drop(a);
    drop(b);
    let a = TlsScopeV1::new(TlsControlV1::new(
        Instant::now() + Duration::from_secs(5),
        || Ok(()),
    )?)?;
    let b = continuous()?;
    let (mut finite, mut legacy) = pair(&a, &b).await?;
    assert!(finite.split_native_continuous_v7().is_err());
    legacy.admit_continuous_service_v4()?;
    assert!(legacy.split_native_continuous_v7().is_err());
    // Rejected upgrade leaves the legitimate original protocol usable.
    let (written, read) = tokio::join!(finite.write_frame(b"original"), legacy.read_frame());
    written?;
    assert_eq!(read?, b"original");
    Ok(())
}
