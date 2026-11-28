use super::*;
use crate::native_pipeline::service::network::query_carrier::{self, Ingress, QuerySlot, Read};
use novovm_network::carrier_stream::CarrierStreamV1;
use novovm_protocol::native_query::QueryError;
use novovm_relay::transport_identity::ephemeral_transport_identity;
use std::{
    num::NonZeroUsize,
    sync::{atomic::AtomicBool, atomic::Ordering, mpsc},
};

#[test]
#[ignore = "requires the signed H42 read-snapshot archive and its pinned profile; loopback only"]
fn advancing_route_requires_the_state_root_on_the_authenticated_path() -> Result<()> {
    let fixture = PathBuf::from(
        std::env::var_os("NOVOVM_SUCCESSOR_FIXTURE").context("snapshot fixture path")?,
    );
    let profile_path = fixture.join("profile.json");
    let pin = fingerprint(&fs::read(&profile_path)?);
    let (profile, domain, anchor) = installed_chain_profile(&profile_path, Some(&pin))?;
    let (provider, consumer) = {
        let library = fixture.join(&profile.library);
        (
            ephemeral_transport_identity(&library, &profile.library_sha256)?.0,
            ephemeral_transport_identity(&library, &profile.library_sha256)?.0,
        )
    };
    let pins = profile.pins();
    let data = fixture.join("device");
    let source_headers = checkpoint_root_in(&data, &profile, anchor)?;
    let expected = fs::read(source_headers.join("00000000000000000042.proof"))?;
    let mut snapshot = read_snapshot::ReadSnapshot::open(
        &data,
        &profile_path,
        &pin,
        42,
        NonZeroUsize::new(128).expect("nonzero state budget"),
        Instant::now() + Duration::from_secs(30),
    )?;
    let scratch = tempfile::tempdir()?;
    for height in (anchor.height + 1)..=41 {
        let name = format!("{height:020}.proof");
        fs::copy(source_headers.join(&name), scratch.path().join(&name))?;
    }
    let chain = VerifiedHeaders::open(
        scratch.path().to_path_buf(),
        anchor,
        FinalityVerifier::new(&domain, consumer.clone()),
        profile
            .verification
            .as_ref()
            .context("trusted archive pins")?,
    )?;
    ensure!(
        chain.tip.height == 41,
        "test must begin at the certified predecessor"
    );
    let mut follower = Follower {
        base: fixture,
        chain,
        cache: StateCache::open(scratch.path().join("nodes"))?,
        locators: Locators::open(scratch.path().join("locators.json"), &profile, &consumer)?,
        profile: Arc::new(profile),
        identity: consumer.clone(),
        domain: domain.clone(),
        native_cursor: 0,
        snapshot: None,
        reported: None,
        retrying: false,
        waiting_finality: None,
        publication_height: None,
    };
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let local = || {
            IrohConfigV1::new(&[], "127.0.0.1".parse()?)?
                .with_bind_address("127.0.0.1:0".parse()?)
        };
        let server = bind_carrier(local()?).await?;
        let client = bind_carrier(local()?).await?;
        let (ready, address) = tokio::sync::oneshot::channel();
        let (requests, reads) = mpsc::sync_channel::<Read>(1);
        let serves_state = Arc::new(AtomicBool::new(false));
        let server_state = serves_state.clone();
        let worker = tokio::task::spawn_blocking(move || {
            while let Ok(read) = reads.recv() {
                let answer = if matches!(read.request.query, Query::StateNode(_))
                    && !server_state.load(Ordering::Acquire)
                {
                    Answer::Error(QueryError::Unavailable)
                } else {
                    snapshot.answer(&read.request.query)
                };
                let _ = read.answer.send(answer);
            }
        });
        let provider_key = provider.public_key().to_vec();
        let serve = server.with_continuous_service_v4(|| Ok(()), |scope| Box::pin(async move {
            ready.send(scope.address()?).map_err(|_| anyhow::anyhow!("test dialer stopped"))?;
            let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
            ensure!(stream.read_frame().await? == query_carrier::ENTRY, "unexpected test stream");
            query_carrier::accept(stream, provider, pins, Ingress {
                requests, slot: Some(Arc::new(QuerySlot::default())), peers: None,
            }).await
        }));
        let attempt = client.with_continuous_service_v4(|| Ok(()), |scope| Box::pin(async move {
            let address = address.await?;
            let (mut stream, channel) = connect_route(scope, &address, &provider_key, consumer, &domain).await?;
            let path = stream.iroh_path_observer();
            let mut session = QuerySession::attach(&mut stream, channel, domain)?;
            let advertised = peer_status(&mut session, follower.chain.tip).await?;
            ensure!(advertised.height == 42, "test provider must advertise its certified snapshot");
            let error = follower.probe_successor(&mut session, &advertised).await.unwrap_err();
            ensure!(error.to_string().contains("cannot serve the certified state snapshot"), "unexpected probe failure: {error:#}");
            ensure!(!scratch.path().join("00000000000000000042.proof").exists(), "failed route published a checkpoint");
            serves_state.store(true, Ordering::Release);
            let proof = follower.probe_successor(&mut session, &advertised).await?;
            ensure!(proof == expected, "qualified route returned a different proof");
            println!("certified H42 proof rejected without state, accepted after the same authenticated peer served the root");
            follower.chain.append(&proof)?;
            let state = follower.sync(&mut session).await?.context("certified snapshot not ready")?;
            ensure!(state.nodes == 19 && state.leaves == 10, "cache did not verify the original H42 state");
            let route = Route {
                carrier: Carrier::Iroh,
                peer: fingerprint(&provider_key),
                native_address: None,
                locator_sequence: None,
            };
            serves_state.store(false, Ordering::Release);
            let denied = follower.run_route(&mut session, route.clone(), None, path).await;
            ensure!(
                matches!(denied, Err(ref error) if error.to_string().contains("cannot serve the certified state snapshot")),
                "authenticated status alone qualified a warm-cache route"
            );
            ensure!(follower.reported.is_none(), "status-only route recorded a recovery");

            serves_state.store(true, Ordering::Release);
            let hold = tokio::time::timeout(Duration::from_secs(2),
                follower.run_route(&mut session, route.clone(), None, None)).await;
            ensure!(hold.is_err(), "qualified warm-cache route terminated");
            ensure!(follower.reported == Some((follower.chain.tip, route)),
                "available authenticated state was not reported");
            println!("warm cache did not qualify the status-only session, but the authenticated root read did");
            Ok(())
        }));
        let result = tokio::time::timeout(Duration::from_secs(20), async {
            tokio::select! {
                value = attempt => value,
                value = serve => value.and_then(|_| anyhow::bail!("test server exited before the probe")),
            }
        }).await?;
        client.close().await?;
        server.close().await?;
        worker.await?;
        result
    })
}
