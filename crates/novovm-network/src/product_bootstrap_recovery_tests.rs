//! Real loopback QUIC fault cases, with explicit local signature fixtures.
use super::*;
use crate::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1},
    product_bootstrap_refresh::{
        refresh_bootstrap_sources_observed_v1, BootstrapRefreshEventV1 as Event,
        BootstrapRefreshPeerV1, BootstrapTransportUnavailableV1,
    },
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

fn config() -> Result<IrohConfigV1> {
    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
}

async fn exchange(mode: &'static str) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(15);
    let (tx, rx) = tokio::sync::oneshot::channel();
    let events = Arc::new(Mutex::new(Vec::new()));
    let saved = Arc::new(Mutex::new(book()));
    let server = with_iroh_scope_v1(config()?, IrohControlV1::new(end, || Ok(()))?, |scope| {
        Box::pin(async move {
            tx.send(scope.address()?)
                .map_err(|_| anyhow::anyhow!("test address receiver closed"))?;
            let mut first: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
            if mode == "reset" || mode == "slow" {
                // Consume the real request, then lose the response. The slow case
                // also demonstrates that a response timeout releases the stream.
                first.read_frame().await?;
                if mode == "slow" {
                    tokio::time::sleep(Duration::from_millis(3200)).await;
                }
                drop(first);
                let mut second = scope.accept_inbound().await?.into();
                serve_bootstrap_sources_v2(
                    &mut second,
                    vec![source(vec![record(2, 2, 2000, 400_000)], 2000)],
                )
                .await?;
                second.finish().await?;
            } else {
                let mut offered = source(vec![record(2, 2, 2000, 400_000)], 2000);
                if mode == "forged" {
                    offered.manifest.signatures[0].signature[0] ^= 1;
                }
                if mode == "version" {
                    first.read_frame().await?;
                    first.write_frame(b"invalid-v1-response").await?;
                } else {
                    serve_bootstrap_sources_v2(&mut first, vec![offered]).await?;
                }
                let _ = first.finish().await;
                // Any redial of a rejected source would arrive here.
                assert!(
                    tokio::time::timeout(Duration::from_millis(600), scope.accept_inbound())
                        .await
                        .is_err()
                );
            }
            Ok(())
        })
    });
    let client_events = events.clone();
    let client_saved = saved.clone();
    let client = async move {
        let events = client_events;
        let saved = client_saved;
        let peer = BootstrapRefreshPeerV1 {
            address: rx.await?,
            expires_at_ms: 10_000,
        };
        with_iroh_scope_v1(
            config()?,
            IrohControlV1::new(end, || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    let result = refresh_bootstrap_sources_observed_v1(
                        scope,
                        &[peer],
                        &saved,
                        || Ok(3000),
                        |e| events.lock().unwrap().push(e),
                    )
                    .await;
                    if mode == "reset" || mode == "slow" {
                        result?;
                    } else {
                        assert!(!result.unwrap_err().is::<BootstrapTransportUnavailableV1>());
                    }
                    Ok(())
                })
            },
        )
        .await
    };
    let (s, c) = tokio::join!(server, client);
    s?;
    c?;
    let events = events.lock().unwrap();
    if mode == "reset" || mode == "slow" {
        assert_eq!(
            events.iter().filter(|e| **e == Event::Connecting).count(),
            2
        );
        assert!(events.contains(&Event::Retry));
        assert!(events.contains(&if mode == "reset" {
            Event::ReadUnavailable
        } else {
            Event::ReadTimeout
        }));
        assert_eq!(events.last(), Some(&Event::Complete));
        assert_eq!(
            saved.lock().unwrap().sources()[0].manifest.issued_at_ms,
            2000
        );
    } else {
        assert_eq!(
            events.iter().filter(|e| **e == Event::Connecting).count(),
            1
        );
        assert!(!events.contains(&Event::Retry));
        assert_eq!(events.last(), Some(&Event::Rejected));
        assert_eq!(
            saved.lock().unwrap().sources()[0].manifest.issued_at_ms,
            1000
        );
    }
    Ok(())
}

#[tokio::test]
async fn bootstrap_recovery_exhaustion_is_typed_only_under_live_original_permission() -> Result<()>
{
    use std::sync::atomic::AtomicU64;
    for mode in ["unavailable", "expired", "revoked", "clock_rollback"] {
        let end = Instant::now() + Duration::from_secs(10);
        let wall = Arc::new(AtomicU64::new(3000));
        let revoked = Arc::new(AtomicBool::new(false));
        let gate = revoked.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = with_iroh_scope_v1(config()?, IrohControlV1::new(end, || Ok(()))?, |scope| {
            Box::pin(async move {
                tx.send(scope.address()?).unwrap();
                for _ in 0..2 {
                    let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
                    stream.read_frame().await?;
                    drop(stream); // Lose both responses on real QUIC streams.
                }
                // Keep the endpoint alive until both resets reach the client.
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(())
            })
        });
        let client = async move {
            let peer = BootstrapRefreshPeerV1 {
                address: rx.await?,
                expires_at_ms: 10_000,
            };
            let book = Arc::new(Mutex::new(book()));
            let before = book.lock().unwrap().checkpoint()?;
            let events = Arc::new(Mutex::new(Vec::new()));
            let saved = book.clone();
            let recorded = events.clone();
            let result = with_iroh_scope_v1(
                config()?,
                IrohControlV1::new(end, move || {
                    ensure!(!gate.load(Ordering::Acquire), "test permission revoked");
                    Ok(())
                })?,
                move |scope| {
                    Box::pin(async move {
                        refresh_bootstrap_sources_observed_v1(
                            scope,
                            &[peer],
                            &book,
                            || Ok(wall.load(Ordering::Acquire)),
                            |event| {
                                let mut events = events.lock().unwrap();
                                events.push(event);
                                if events
                                    .iter()
                                    .filter(|e| **e == Event::ReadUnavailable)
                                    .count()
                                    == 2
                                {
                                    match mode {
                                        "expired" => wall.store(10_000, Ordering::Release),
                                        "clock_rollback" => wall.store(2999, Ordering::Release),
                                        "revoked" => revoked.store(true, Ordering::Release),
                                        _ => {}
                                    }
                                }
                            },
                        )
                        .await
                    })
                },
            )
            .await;
            let error = result.unwrap_err();
            assert_eq!(
                error.is::<BootstrapTransportUnavailableV1>(),
                mode == "unavailable",
                "{mode}: {error:#}"
            );
            assert_eq!(saved.lock().unwrap().checkpoint()?, before);
            assert_eq!(
                recorded
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|e| **e == Event::Connecting)
                    .count(),
                2
            );
            Ok::<_, anyhow::Error>(())
        };
        let (server, client) = tokio::join!(server, client);
        server?;
        client?;
    }
    Ok(())
}

#[tokio::test]
async fn bootstrap_recovery_retries_lost_response_and_read_timeout() -> Result<()> {
    exchange("reset").await?;
    exchange("slow").await
}

#[tokio::test]
async fn bootstrap_recovery_never_retries_wrong_version_or_signature() -> Result<()> {
    exchange("forged").await?;
    exchange("version").await
}

#[tokio::test]
async fn bootstrap_recovery_original_deadline_and_permission_prevent_dials() -> Result<()> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let saved = Arc::new(Mutex::new(book()));
    let peer = BootstrapRefreshPeerV1 {
        address: address(3).address,
        expires_at_ms: 3000,
    };
    let recorded = events.clone();
    let began = Instant::now();
    with_iroh_scope_v1(
        config()?,
        IrohControlV1::new(began + Duration::from_millis(100), || Ok(()))?,
        move |scope| {
            Box::pin(async move {
                assert!(refresh_bootstrap_sources_observed_v1(
                    scope,
                    &[peer],
                    &saved,
                    || Ok(3000),
                    |e| events.lock().unwrap().push(e)
                )
                .await
                .is_err());
                Ok(())
            })
        },
    )
    .await
    .unwrap_err(); // original deadline also cancels the retry wait
    assert!(began.elapsed() < Duration::from_secs(1));
    assert!(!recorded.lock().unwrap().contains(&Event::Connecting));
    Ok(())
}

#[tokio::test]
async fn bootstrap_recovery_revocation_during_backoff_stops_redial() -> Result<()> {
    let revoked = Arc::new(AtomicBool::new(false));
    let gate = revoked.clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    let saved = Arc::new(Mutex::new(book()));
    let peer = BootstrapRefreshPeerV1 {
        address: address(3).address,
        expires_at_ms: 10_000,
    };
    let recorded = events.clone();
    let end = Instant::now() + Duration::from_secs(10);
    let result = with_iroh_scope_v1(
        config()?,
        IrohControlV1::new(end, move || {
            ensure!(!gate.load(Ordering::Acquire), "test authorization revoked");
            Ok(())
        })?,
        move |scope| {
            Box::pin(async move {
                refresh_bootstrap_sources_observed_v1(
                    scope,
                    &[peer],
                    &saved,
                    || Ok(3000),
                    |e| {
                        events.lock().unwrap().push(e);
                        if e == Event::Retry {
                            revoked.store(true, Ordering::Release);
                        }
                    },
                )
                .await
            })
        },
    )
    .await;
    assert!(result.is_err());
    let events = recorded.lock().unwrap();
    assert_eq!(
        events.iter().filter(|e| **e == Event::Connecting).count(),
        1
    );
    assert!(events.contains(&Event::Retry));
    Ok(())
}
