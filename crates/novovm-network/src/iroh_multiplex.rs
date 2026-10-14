//! Bounded streams under one existing owner/QUIC admission. No application
//! channel, credential, replay state or trust decision is shared between lanes.
use super::*;

pub const IROH_MAX_MULTIPLEX_STREAMS_V1: usize = 4;

pub(super) struct SharedConnection {
    connection: Connection,
    _admission: tokio::sync::OwnedSemaphorePermit,
    streams: Arc<tokio::sync::Semaphore>,
}
impl Drop for SharedConnection {
    fn drop(&mut self) {
        self.connection
            .close(0u32.into(), b"shared carrier retired");
    }
}

/// A single scoped owner, not an idle cache or another endpoint. Dropping it
/// closes every lane; the original connection permit remains held until all
/// stream references have gone. Streams cannot escape the authorized scope.
pub struct IrohMultiplexV1<'scope> {
    scope: &'scope IrohScopeV1,
    shared: Arc<SharedConnection>,
    initiator: bool,
    accept_consumer: tokio::sync::Mutex<()>,
}
impl Drop for IrohMultiplexV1<'_> {
    fn drop(&mut self) {
        self.shared
            .connection
            .close(0u32.into(), b"multiplex owner stopped");
    }
}

impl<'scope> IrohStreamV1<'scope> {
    /// Only an explicit crate-owned service selector may opt in. This changes
    /// the TLS exporter context BEFORE Finished, so version stripping cannot
    /// borrow a legacy channel. Selection itself grants no application trust.
    pub(crate) fn multiplex_v1(&mut self) -> Result<IrohMultiplexV1<'scope>> {
        self.check()?;
        ensure!(
            self.scope.control.deadline.is_none()
                && (self.scope.continuous_service || self.scope.listener_budget.is_some())
                && self.scope.reuse.is_none()
                && self.shared.is_none()
                && !self.finished
                && !self.failed.load(Ordering::Acquire)
                && !self.handed_off
                && !self.service_window
                && !self.continuous_service
                && !self.reused
                && u64::from(self.send.id()) == 0,
            "multiplex requires fresh owned service connection"
        );
        let streams = Arc::new(tokio::sync::Semaphore::new(IROH_MAX_MULTIPLEX_STREAMS_V1));
        let stream_admission = streams.clone().try_acquire_owned()?;
        let admission = self
            ._admission
            .take()
            .context("missing carrier admission")?;
        let shared = Arc::new(SharedConnection {
            connection: self.connection.clone(),
            _admission: admission,
            streams,
        });
        // Connection receive/send windows stay unchanged. Only an explicitly
        // selected responder accepts extra client streams, up to this budget.
        if !self.initiator {
            self.connection
                .set_max_concurrent_bi_streams((IROH_MAX_MULTIPLEX_STREAMS_V1 as u32).into());
        }
        self.shared = Some(shared.clone());
        self._stream_admission = Some(stream_admission);
        Ok(IrohMultiplexV1 {
            scope: self.scope,
            shared,
            initiator: self.initiator,
            accept_consumer: tokio::sync::Mutex::new(()),
        })
    }

    pub(crate) fn is_multiplexed(&self) -> bool {
        self.shared.is_some()
    }
}

impl<'scope> IrohMultiplexV1<'scope> {
    fn check(&self) -> Result<()> {
        self.scope.control.check()?;
        if let Some(error) = self.shared.connection.close_reason() {
            return Err(IrohStreamIoFailureV1(error.into()).into());
        }
        Ok(())
    }

    /// Fresh stream, fresh IO budget, fresh Finished required by the caller.
    /// There is no dial or admission of another QUIC connection here.
    pub async fn open(&self) -> Result<IrohStreamV1<'scope>> {
        self.check()?;
        ensure!(self.initiator, "multiplex responder cannot initiate lanes");
        let permit = self
            .shared
            .streams
            .clone()
            .try_acquire_owned()
            .context("multiplex stream limit")?;
        let (send, recv) = tokio::select! {
            biased;
            error = self.scope.control.revoked() => return Err(error),
            result = tokio::time::timeout(INCOMING_SETUP_TIMEOUT, self.shared.connection.open_bi()) => {
                result.map_err(|_| IrohFrameDeadlineV1(()))?
                    .map_err(|error| IrohStreamIoFailureV1(error.into()))?
            }
        };
        self.stream(send, recv, permit)
    }

    /// Only one accept consumer; bounded at the application AND QUIC layers.
    /// Waiting does not refresh any previous stream's IO or business authority.
    pub async fn accept(&self) -> Result<IrohStreamV1<'scope>> {
        self.check()?;
        ensure!(!self.initiator, "multiplex initiator cannot accept lanes");
        let _consumer = self
            .accept_consumer
            .try_lock()
            .context("multiplex accept already pending")?;
        let permit = self
            .shared
            .streams
            .clone()
            .try_acquire_owned()
            .context("multiplex stream limit")?;
        let (send, recv) = tokio::select! {
            biased;
            error = self.scope.control.revoked() => return Err(error),
            result = self.shared.connection.accept_bi() => result.map_err(|error| IrohStreamIoFailureV1(error.into()))?,
        };
        self.stream(send, recv, permit)
    }

    fn stream(
        &self,
        send: SendStream,
        recv: RecvStream,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<IrohStreamV1<'scope>> {
        self.check()?;
        ensure!(send.id() == recv.id(), "multiplex stream ID mismatch");
        Ok(IrohStreamV1 {
            scope: self.scope,
            io_control: IrohControlV1 {
                check: self.scope.control.check.clone(),
                deadline: Some(Instant::now() + Duration::from_secs(30)),
            },
            connection: self.shared.connection.clone(),
            send,
            recv,
            failed: AtomicBool::new(false),
            initiator: self.initiator,
            _admission: None,
            shared: Some(self.shared.clone()),
            _stream_admission: Some(permit),
            finished: false,
            reused: true,
            handed_off: false,
            service_window: false,
            continuous_service: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> IrohConfigV1 {
        IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
            .unwrap()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .unwrap()
    }

    #[tokio::test]
    async fn stream_binding_budget_owner_drop_and_normal_sibling_completion() -> Result<()> {
        let _serial = super::super::tests::SERIAL.acquire().await?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (binding_tx, binding_rx) = tokio::sync::oneshot::channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let listener = with_iroh_owned_listener_v1(
            config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    tx.send(scope.address()?).unwrap();
                    let mut root = scope.accept_inbound().await?;
                    ensure!(root.read_frame().await? == b"select");
                    let group = root.multiplex_v1()?;
                    root.write_frame(b"selected").await?;
                    let root_binding = root.binding_material()?;
                    let mut child = group.accept().await?;
                    ensure!(child.read_frame().await? == b"child");
                    let child_binding = child.binding_material()?;
                    ensure!(
                        root_binding.2 != child_binding.2,
                        "streams share Finished context"
                    );
                    binding_tx.send((root_binding, child_binding)).unwrap();
                    ensure!(scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1 - 1);
                    root.finish().await?;
                    drop(root);
                    child.write_frame(b"root ended").await?;
                    ensure!(child.read_frame().await? == b"sibling works");
                    child.finish().await?;
                    drop(child);
                    // The connection owner keeps admission even between streams.
                    ensure!(scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1 - 1);
                    let child = group.accept().await?;
                    // Dropping the sole connection owner closes outstanding lanes.
                    drop(group);
                    ensure!(child.connection.close_reason().is_some());
                    ensure!(scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1 - 1);
                    drop(child);
                    ensure!(scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1);
                    let _ = done_tx.send(());
                    Ok(())
                })
            },
        );
        let client = async {
            let owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
            let result = owner
                .with_continuous_service_v4(
                    || Ok(()),
                    move |scope| {
                        Box::pin(async move {
                            let mut root = scope.connect(&rx.await?).await?;
                            let legacy = root.binding_material()?;
                            root.write_frame(b"select").await?;
                            ensure!(root.read_frame().await? == b"selected");
                            let group = root.multiplex_v1()?;
                            ensure!(root.multiplex_v1().is_err(), "duplicate group owner");
                            let mut child = group.open().await?;
                            child.write_frame(b"child").await?;
                            let (server_root, server_child) = binding_rx.await?;
                            ensure!(
                                legacy.2 != root.binding_material()?.2,
                                "version stripping not bound"
                            );
                            ensure!(
                                server_root == root.binding_material()?
                                    && server_child == child.binding_material()?
                            );
                            let unused_a = group.open().await?;
                            let unused_b = group.open().await?;
                            ensure!(group.open().await.is_err(), "fifth stream admitted");
                            ensure!(
                                scope.connections.available_permits()
                                    == IROH_MAX_CONNECTIONS_V1 - 1
                            );
                            drop((unused_a, unused_b));
                            root.finish().await?;
                            drop(root);
                            ensure!(child.read_frame().await? == b"root ended");
                            child.write_frame(b"sibling works").await?;
                            child.finish().await?;
                            drop(child);
                            let mut extra = group.open().await?;
                            // Make the stream observable; no application credential is sent.
                            let _ = extra.write_frame(b"retire").await;
                            done_rx.await?;
                            drop((extra, group));
                            ensure!(
                                scope.connections.available_permits() == IROH_MAX_CONNECTIONS_V1
                            );
                            Ok(())
                        })
                    },
                )
                .await;
            owner.close().await?;
            result
        };
        let (a, b) = tokio::time::timeout(Duration::from_secs(20), async {
            tokio::join!(listener, client)
        })
        .await?;
        a?;
        b?;
        Ok(())
    }
}
