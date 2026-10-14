//! Transport-only failure injection; no application authorization is asserted.
use super::*;
use zeroize::Zeroizing;
fn config() -> IrohConfigV1 {
    IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
        .unwrap()
        .with_bind_address("127.0.0.1:0".parse().unwrap())
        .unwrap()
}
#[tokio::test]
async fn idle_partial_frame_keeps_original_budget_and_cannot_be_revived() -> Result<()> {
    failure(false).await
}
#[tokio::test]
async fn idle_cancel_remains_terminal() -> Result<()> {
    failure(true).await
}
async fn failure(cancel: bool) -> Result<()> {
    let _permit = tests::SERIAL.acquire().await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    let server = with_iroh_owned_listener_v1(
        config().with_owned_service_identity(Zeroizing::new([84; 32]))?,
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                tx.send(scope.address()?)
                    .map_err(|_| anyhow::anyhow!("fixture address"))?;
                let mut stream = scope.accept_inbound().await?;
                assert_eq!(stream.read_frame().await?, b"idle fixture");
                // Test the transport state seam directly. No mailbox request is executed.
                stream.admit_continuous_service_v4()?;
                stream.complete_service_request_v6()?;
                if cancel {
                    assert!(tokio::time::timeout(
                        Duration::from_millis(100),
                        stream.read_idle_service_frame_v6()
                    )
                    .await
                    .is_err());
                } else {
                    let started = Instant::now();
                    let error = stream.read_idle_service_frame_v6().await.unwrap_err();
                    assert!(
                        error.is::<IrohFrameDeadlineV1>(),
                        "unexpected type: {error:#}"
                    );
                    assert!(started.elapsed() >= Duration::from_secs(30));
                }
                assert!(stream.begin_service_request_v4().is_err());
                assert!(stream.complete_service_request_v6().is_err());
                assert!(stream.read_frame().await.is_err());
                let _ = done_tx.send(());
                Ok(())
            })
        },
    );
    let owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
    let client = owner.with_continuous_service_v4(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                let mut stream = scope.connect(&rx.await?).await?;
                stream.write_frame(b"idle fixture").await?;
                if !cancel {
                    stream.send.write_all(&[0]).await?;
                }
                tokio::time::timeout(Duration::from_secs(40), done_rx).await??;
                Ok(())
            })
        },
    );
    let (server, client) = tokio::join!(server, client);
    server?;
    client?;
    owner.close().await?;
    Ok(())
}
