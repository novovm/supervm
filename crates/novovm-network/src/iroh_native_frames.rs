//! Existing carrier framing for a continuously owned native session. Transport
//! frame budgets never replace or renew NativeMailbox's business deadlines.
use super::*;
use crate::novorudp::datagram::MAX_DATAGRAM_BYTES;

const FRAME_TIMEOUT: Duration = Duration::from_secs(30);

impl IrohStreamV1<'_> {
    /// Internal native caller verifies structured Finished and explicit service
    /// selection before this transition. A finite/expired or legacy scope cannot
    /// be upgraded. Each half retains cancellation and live owner checks.
    pub(crate) fn split_native_continuous_v7(
        &mut self,
    ) -> Result<(IrohWriteHalfV1<'_>, IrohReadHalfV1<'_>)> {
        self.check()?;
        ensure!(
            self.scope.control.deadline.is_none()
                && (self.scope.continuous_service || self.scope.listener_budget.is_some())
                && self.io_control.deadline.is_some()
                && !self.service_window
                && !self.continuous_service
                && !self.native_service
                && !self.finished
                && !self.handed_off
                && self.shared.is_none(),
            "native continuous framing requires fresh owned scope"
        );
        self.native_service = true;
        self.io_control.deadline = None;
        Ok((
            IrohWriteHalfV1 {
                control: &self.io_control,
                connection: &self.connection,
                send: &mut self.send,
                failed: &self.failed,
                native: true,
            },
            IrohReadHalfV1 {
                control: &self.io_control,
                connection: &self.connection,
                recv: &mut self.recv,
                failed: &self.failed,
                native: true,
            },
        ))
    }
}

pub(super) async fn write(
    control: &IrohControlV1,
    connection: &Connection,
    failed: &AtomicBool,
    send: &mut SendStream,
    bytes: &[u8],
) -> Result<()> {
    check_frame_io(control, connection, failed)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_DATAGRAM_BYTES,
        "native carrier frame limit"
    );
    let mut guard = FrameIoGuard {
        connection,
        failed,
        complete: false,
    };
    let deadline = Instant::now() + FRAME_TIMEOUT;
    tokio::select! {
        biased;
        error = control.revoked() => return Err(error),
        result = tokio::time::timeout_at(deadline.into(), async {
            send.write_all(&(bytes.len() as u32).to_be_bytes()).await
                .map_err(|error| IrohStreamIoFailureV1(error.into()))?;
            send.write_all(bytes).await.map_err(|error| IrohStreamIoFailureV1(error.into()))
        }) => result.map_err(|_| IrohFrameDeadlineV1(()))??,
    }
    check_frame_io(control, connection, failed)?;
    guard.complete = true;
    Ok(())
}

pub(super) async fn read(
    control: &IrohControlV1,
    connection: &Connection,
    failed: &AtomicBool,
    recv: &mut RecvStream,
) -> Result<Vec<u8>> {
    check_frame_io(control, connection, failed)?;
    let mut guard = FrameIoGuard {
        connection,
        failed,
        complete: false,
    };
    let mut length = [0; 4];
    tokio::select! {
        biased;
        error = control.revoked() => return Err(error),
        result = recv.read_exact(&mut length[..1]) => {
            result.map_err(|error| IrohStreamIoFailureV1(error.into()))?;
        }
    }
    // A single absolute deadline covers remaining header and whole body. A
    // slow sender cannot renew it byte by byte; concurrent writes cannot reset it.
    let deadline = Instant::now() + FRAME_TIMEOUT;
    let bytes = tokio::select! {
        biased;
        error = control.revoked() => return Err(error),
        result = tokio::time::timeout_at(deadline.into(), async {
            recv.read_exact(&mut length[1..]).await.map_err(|error| IrohStreamIoFailureV1(error.into()))?;
            let length = u32::from_be_bytes(length) as usize;
            ensure!(length > 0 && length <= MAX_DATAGRAM_BYTES, "native carrier frame limit");
            let mut bytes = vec![0; length];
            recv.read_exact(&mut bytes).await.map_err(|error| IrohStreamIoFailureV1(error.into()))?;
            Ok::<_, anyhow::Error>(bytes)
        }) => result.map_err(|_| IrohFrameDeadlineV1(()))??,
    };
    check_frame_io(control, connection, failed)?;
    guard.complete = true;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    fn config() -> IrohConfigV1 {
        IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
            .unwrap()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .unwrap()
    }

    // Lower framing tests use actual QUIC streams. Structured identity and
    // native object delivery are covered separately by bound_session tests.
    async fn rejection(case: u8) -> Result<()> {
        let _permit = crate::SERVICE_TEST_SERIAL.acquire().await?;
        let server_owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
        let client_owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
        let live = Arc::new(AtomicBool::new(true));
        let server_live = live.clone();
        let (address_tx, address_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let server = server_owner.with_continuous_service_v4(
            move || {
                ensure!(
                    server_live.load(Ordering::Acquire),
                    "native fixture revoked"
                );
                Ok(())
            },
            move |scope| {
                Box::pin(async move {
                    address_tx.send(scope.address()?).unwrap();
                    let mut stream = scope.accept_inbound().await?;
                    ensure!(stream.read_frame().await? == b"framing fixture");
                    stream.write_frame(b"ready").await?;
                    {
                        let (mut write, mut read) = stream.split_native_continuous_v7()?;
                        ready_tx.send(()).unwrap();
                        let start = Instant::now();
                        let error = if case == 2 {
                            // Cancellation of a polled idle read also retires this owner,
                            // never reuses a stream that may have consumed partial input.
                            let result =
                                tokio::time::timeout(Duration::from_millis(50), read.read_frame())
                                    .await;
                            ensure!(result.is_err());
                            anyhow::anyhow!("intentional cancellation")
                        } else {
                            read.read_frame().await.unwrap_err()
                        };
                        if case == 0 {
                            ensure!(
                                error.is::<IrohFrameDeadlineV1>(),
                                "unexpected timeout error: {error:#}"
                            );
                            ensure!(start.elapsed() >= FRAME_TIMEOUT);
                        }
                        ensure!(write.write_frame(b"must reject").await.is_err());
                    }
                    ensure!(stream.split_io().is_err());
                    ensure!(stream.split_native_continuous_v7().is_err());
                    ensure!(stream.admit_continuous_service_v4().is_err());
                    let _ = done_tx.send(());
                    Ok(())
                })
            },
        );
        let client = client_owner.with_continuous_service_v4(
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    let mut stream = scope.connect(&address_rx.await?).await?;
                    stream.write_frame(b"framing fixture").await?;
                    ensure!(stream.read_frame().await? == b"ready");
                    ready_rx.await?;
                    match case {
                        0 => {
                            stream.send.write_all(&[0]).await?;
                        }
                        1 => {
                            stream.send.write_all(&1201u32.to_be_bytes()).await?;
                        }
                        2 => {}
                        3 => {
                            live.store(false, Ordering::Release);
                            // The scope supervisor can win before the inner
                            // reader observes revocation. Either path must
                            // close the actual connection, not leave idle IO.
                            stream.connection.closed().await;
                            return Ok(());
                        }
                        _ => unreachable!(),
                    }
                    done_rx.await?;
                    Ok(())
                })
            },
        );
        let result = tokio::time::timeout(Duration::from_secs(36), async {
            tokio::join!(server, client)
        })
        .await;
        server_owner.close().await?;
        client_owner.close().await?;
        let (server, client) = result?;
        if case == 3 {
            ensure!(server
                .unwrap_err()
                .to_string()
                .contains("native fixture revoked"));
        } else {
            server?;
        }
        client?;
        Ok(())
    }
    #[tokio::test]
    async fn native_quic_partial_frame_has_fixed_deadline() -> Result<()> {
        rejection(0).await
    }
    #[tokio::test]
    async fn native_quic_oversize_cancel_and_revoke_are_terminal() -> Result<()> {
        for case in 1..=3 {
            rejection(case).await?;
        }
        Ok(())
    }
}
