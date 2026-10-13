//! Idle transport connections only. No application identity, lease, plaintext,
//! replay window or signing key is retained here. The endpoint's existing owner
//! and semaphore bound every live connection, including idle ones.
use super::*;
use std::sync::Mutex;

struct Idle {
    lane: u8,
    connection: Option<Connection>,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
}
impl Drop for Idle {
    fn drop(&mut self) {
        if let Some(connection) = &self.connection {
            connection.close(0u32.into(), b"retained connection retired");
        }
    }
}

#[derive(Default)]
pub(super) struct Pool(Mutex<Vec<Idle>>);
impl Pool {
    pub(super) fn clear(&self) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("carrier pool unavailable"))?
            .clear();
        Ok(())
    }
    pub(super) fn put(
        &self,
        lane: u8,
        connection: Connection,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<()> {
        let mut pool = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("carrier pool unavailable"))?;
        // Leave two of the existing four slots for discovery/short operations.
        pool.retain(|entry| {
            entry.connection.as_ref().is_some_and(|old| {
                old.close_reason().is_none()
                    && !(entry.lane == lane && old.remote_id() == connection.remote_id())
            })
        });
        if pool.len() == 2 {
            pool.remove(0);
        }
        pool.push(Idle {
            lane,
            connection: Some(connection),
            permit: Some(permit),
        });
        Ok(())
    }
    fn take(&self, lane: u8, remote: [u8; 32]) -> Result<Option<Idle>> {
        let mut pool = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("carrier pool unavailable"))?;
        pool.retain(|entry| {
            entry
                .connection
                .as_ref()
                .is_some_and(|old| old.close_reason().is_none())
        });
        Ok(pool
            .iter()
            .position(|entry| {
                entry.lane == lane
                    && entry
                        .connection
                        .as_ref()
                        .is_some_and(|old| old.remote_id().as_bytes() == &remote)
            })
            .map(|i| pool.remove(i)))
    }
}

// Clearing on error AND cancellation also covers a finished stream followed by
// a later application rejection in the same operation. Preserve the first error.
pub(super) struct OperationGuard(Option<Arc<Pool>>);
impl OperationGuard {
    pub(super) fn new(pool: Option<Arc<Pool>>) -> Self {
        Self(pool)
    }
    pub(super) fn commit(&mut self) {
        self.0 = None;
    }
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        if let Some(pool) = &self.0 {
            let _ = pool.clear();
        }
    }
}

impl IrohScopeV1 {
    /// Borrow a previously finished TLS connection WITHOUT dialing an old
    /// address. Absence is not permission to use stale discovery metadata.
    /// Each returned stream checks this NEW scope and must authenticate its
    /// application peer anew. Purpose lanes are local, never peer-supplied.
    pub async fn retained(
        &self,
        remote: [u8; 32],
        initiator: bool,
    ) -> Result<Option<IrohStreamV1<'_>>> {
        self.retained_inner(remote, initiator, false).await
    }

    pub(super) async fn retained_inner(
        &self,
        remote: [u8; 32],
        initiator: bool,
        accept_fresh: bool,
    ) -> Result<Option<IrohStreamV1<'_>>> {
        self.control.check()?;
        let Some((pool, lane)) = &self.reuse else {
            return Ok(None);
        };
        let Some(mut idle) = pool.take(*lane, remote)? else {
            return Ok(None);
        };
        let pending = PendingConnection::new(
            idle.connection
                .take()
                .context("missing retained connection")?,
        );
        let permit = idle.permit.take().context("missing retained admission")?;
        let mut stream = if initiator {
            self.stream(pending, remote, true, permit).await?
        } else if !accept_fresh {
            let _consumer = self
                .accept_consumer
                .try_lock()
                .context("carrier accept already pending")?;
            self.stream(pending, remote, false, permit).await?
        } else {
            let _consumer = self
                .accept_consumer
                .try_lock()
                .context("carrier accept already pending")?;
            // A peer may have retired its idle connection and made a fresh
            // dial. Do not wait forever on the old accept_bi in that case.
            tokio::select! {
                result = self.stream(pending, remote, false, permit) => result?,
                result = self.accept_inner_locked(Some(remote)) => return result.map(Some),
            }
        };
        stream.reused = true;
        self.control.check()?;
        Ok(Some(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Result<IrohConfigV1> {
        IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
    }
    fn control(seconds: u64) -> Result<IrohControlV1> {
        IrohControlV1::new(Instant::now() + Duration::from_secs(seconds), || Ok(()))
    }
    async fn wave(
        a: &IrohEndpointOwnerV1,
        b: &IrohEndpointOwnerV1,
        lane: u8,
        expected: bool,
    ) -> Result<()> {
        let (at, br) = tokio::sync::oneshot::channel();
        let (bt, ar) = tokio::sync::oneshot::channel();
        let left = a.with_reusable_operation(control(5)?, lane, |scope| {
            Box::pin(async move {
                at.send(scope.address()?)
                    .map_err(|_| anyhow::anyhow!("fixture address closed"))?;
                let peer = ar.await?;
                let mut stream = scope.connect(&peer).await?;
                ensure!(
                    stream.connection_reused() == expected,
                    "wrong reuse observation"
                );
                stream
                    .write_frame(b"new independently authorized operation")
                    .await?;
                ensure!(stream.read_frame().await? == b"ack", "fixture response");
                stream.finish().await
            })
        });
        let right = b.with_reusable_operation(control(5)?, lane, |scope| {
            Box::pin(async move {
                bt.send(scope.address()?)
                    .map_err(|_| anyhow::anyhow!("fixture address closed"))?;
                let peer = br.await?;
                let mut stream = scope.accept(peer.endpoint_key).await?;
                ensure!(
                    stream.connection_reused() == expected,
                    "wrong reuse observation"
                );
                ensure!(
                    stream.read_frame().await? == b"new independently authorized operation",
                    "fixture input"
                );
                stream.write_frame(b"ack").await?;
                stream.finish().await
            })
        });
        let (left, right) = tokio::join!(left, right);
        left?;
        right?;
        Ok(())
    }

    #[tokio::test]
    async fn fresh_operations_reuse_transport_but_not_deadlines_or_purpose() -> Result<()> {
        let _serial = super::super::tests::SERIAL.acquire().await?;
        let a = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        let b = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        wave(&a, &b, 1, false).await?;
        wave(&a, &b, 1, true).await?;
        wave(&a, &b, 2, false).await?;
        wave(&a, &b, 2, true).await?;
        ensure!(
            a.connections.available_permits() == 2 && b.connections.available_permits() == 2,
            "idle admission lost"
        );
        let rejected = IrohControlV1::new(Instant::now() + Duration::from_secs(2), || {
            anyhow::bail!("fixture revoked")
        })?;
        let result = a
            .with_reusable_operation(rejected, 1, |_| {
                Box::pin(async { anyhow::bail!("must not enter") })
                    as Pin<Box<dyn Future<Output = Result<()>>>>
            })
            .await;
        ensure!(
            result.unwrap_err().to_string() == "fixture revoked",
            "revoked operation entered"
        );
        a.network_change(control(3)?).await?;
        b.network_change(control(3)?).await?;
        ensure!(
            a.connections.available_permits() == 4 && b.connections.available_permits() == 4,
            "network change retained stale connections"
        );
        wave(&a, &b, 1, false).await?;
        a.close().await?;
        b.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn rejected_operation_clears_finished_connections_and_new_authority_cannot_reuse_them(
    ) -> Result<()> {
        let _serial = super::super::tests::SERIAL.acquire().await?;
        let a = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        let b = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        wave(&a, &b, 1, false).await?;
        let result: Result<()> = a
            .with_reusable_operation(control(2)?, 1, |_| {
                Box::pin(async { anyhow::bail!("fixture signature rejection") })
            })
            .await;
        ensure!(
            result.unwrap_err().to_string() == "fixture signature rejection",
            "first rejection changed"
        );
        ensure!(
            a.reuse.0.lock().unwrap().is_empty(),
            "rejected operation retained connection"
        );
        a.close().await?;
        b.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn cancellation_closes_borrowed_connection_and_idle_owner_revocation_rejects(
    ) -> Result<()> {
        let _serial = super::super::tests::SERIAL.acquire().await?;
        let active = Arc::new(AtomicBool::new(true));
        let check = active.clone();
        let a = IrohEndpointOwnerV1::bind(config()?, move || {
            ensure!(check.load(Ordering::Acquire), "fixture owner revoked");
            Ok(())
        })
        .await?;
        let b = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        wave(&a, &b, 1, false).await?;
        let bkey = *b.endpoint.id().as_bytes();
        a.with_reusable_operation(control(2)?, 1, |scope| {
            Box::pin(async move {
                let stream = scope
                    .retained(bkey, true)
                    .await?
                    .context("missing retained stream")?;
                ensure!(stream.connection_reused(), "fresh connection");
                drop(stream); // No finish: must not return to pool.
                Ok(())
            })
        })
        .await?;
        ensure!(
            a.reuse.0.lock().unwrap().is_empty(),
            "cancelled stream retained"
        );
        active.store(false, Ordering::Release);
        assert!(a
            .with_reusable_operation(control(2)?, 1, |_| Box::pin(async { Ok(()) }))
            .await
            .is_err());
        a.close().await?;
        b.close().await?;
        Ok(())
    }
}
