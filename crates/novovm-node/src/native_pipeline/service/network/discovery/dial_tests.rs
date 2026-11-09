use super::*;
use crate::native_pipeline::consensus::native::tests::separated_material;

#[test]
fn unreachable_latest_and_seed_obey_original_deadline_and_release_owner() -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (_, keys, domain, _) = separated_material()?;
            let pins = (domain.chain_id, domain.genesis, domain.protocol);
            let sink = std::net::UdpSocket::bind("127.0.0.1:0")?;
            let owner = IrohEndpointOwnerV1::bind(
                IrohConfigV1::new(&[], "127.0.0.1".parse()?)?
                    .with_bind_address("127.0.0.1:0".parse()?)?,
                || Ok(()),
            )
            .await?;
            let failures = Arc::new(NetworkFailures::default());
            failures.carrier.enabled.store(true, Ordering::Relaxed);
            let observed = failures.clone();
            let result = owner
                .with_continuous_service_v4(
                    || Ok(()),
                    move |scope| {
                        Box::pin(async move {
                            for source in ["latest_signed_record", "configured_seed"] {
                                let candidate = DialCandidate {
                                    key: keys[1].public_key().to_vec(),
                                    source,
                                    sequence: 4166,
                                    route: IrohAddressV1 {
                                        endpoint_key: ed25519_dalek::SigningKey::from_bytes(
                                            &[19; 32],
                                        )
                                        .verifying_key()
                                        .to_bytes(),
                                        direct: vec![sink.local_addr()?],
                                        relays: vec![],
                                    },
                                };
                                let result = query(
                                    scope,
                                    keys[0].clone(),
                                    candidate,
                                    pins,
                                    vec![],
                                    vec![],
                                    failures.clone(),
                                )
                                .await;
                                let error =
                                    result.expect_err("blackhole cannot complete a discovery RPC");
                                ensure!(error.is::<BootstrapDialUnavailable>());
                                ensure!(format!("{error:#}")
                                    .contains("discovery deadline at carrier connect"));
                            }
                            Ok(())
                        })
                    },
                )
                .await;
            ensure!(owner.snapshot().active_operations == 0);
            owner.close().await?;
            result?;
            let report = observed.carrier.report();
            let events = report["events"].as_array().unwrap();
            ensure!(events.iter().filter(|e| e["outcome"] == "deadline").count() == 2);
            ensure!(!events
                .iter()
                .any(|e| e["capacity_rejected"] == true || e["phase"] == "selected_path"));
            Ok(())
        })
}

#[test]
fn private_only_latest_retains_public_hint_without_sequence_authority_or_revocation_rollback(
) -> Result<()> {
    let (_, keys, domain, _) = separated_material()?;
    let pins = (domain.chain_id, domain.genesis, domain.protocol);
    let now = wall_ms()?;
    let mut endpoint = IrohServiceEndpointV1 {
        instance: [1; 32],
        address: IrohAddressV1 {
            endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[17; 32])
                .verifying_key()
                .to_bytes(),
            direct: vec!["47.103.59.206:42050".parse()?],
            relays: vec!["https://euc1-1.relay.n0.iroh.link/".into()],
        },
    };
    let root = sign_relay_record_v1(
        &keys[0],
        native_locator_id_v1(pins),
        vec![endpoint.endpoint()?],
        now,
        now + 120_000,
        12,
    )?;
    let mut seed = Seed {
        path: PathBuf::new(),
        book: None,
        key: None,
        bootstrap: None,
        fallback: false,
        job: None,
        next: Instant::now(),
        quarantined: None,
    };
    Discovery::update_seed(&mut seed, &root, pins, &keys[1], now)?;
    endpoint.address.direct = vec!["172.16.0.48:42050".parse()?];
    endpoint.address.relays.clear();
    let latest = sign_relay_record_v1(
        &keys[0],
        &root.record_id,
        vec![endpoint.endpoint()?],
        now,
        now + 120_000,
        4166,
    )?;
    Discovery::update_seed(&mut seed, &latest, pins, &keys[1], now)?;
    let candidate = seed.dial_candidate(now)?.unwrap();
    assert_eq!(candidate.source, "latest_signed_record");
    assert_eq!(candidate.route.direct, endpoint.address.direct);
    // Only a typed pre-connection transport failure rotates the bounded hint.
    seed.finish(&Err(anyhow::anyhow!("capacity or unknown local error")));
    assert!(!seed.fallback);
    seed.finish(&Err(anyhow::Error::new(BootstrapDialUnavailable)));
    let hint = seed.dial_candidate(now)?.unwrap();
    assert_eq!(hint.source, "configured_seed");
    assert_eq!(hint.sequence, 4166);
    assert_eq!(hint.key, latest.relay_public_key);
    assert_eq!(
        hint.route.direct,
        vec!["47.103.59.206:42050".parse::<SocketAddr>()?]
    );
    assert_eq!(hint.route.relays.len(), 1);
    assert_eq!(
        seed.book.as_ref().unwrap().self_owned_records()?,
        vec![latest.clone()]
    );
    assert!(Discovery::update_seed(&mut seed, &root, pins, &keys[1], now).is_err());
    seed.finish(&Err(anyhow::Error::new(BootstrapDialUnavailable)));
    assert_eq!(
        seed.dial_candidate(now)?.unwrap().source,
        "latest_signed_record"
    );
    seed.finish(&Err(anyhow::Error::new(BootstrapDialUnavailable)));
    seed.finish(&Ok(Vec::new()));
    assert!(!seed.fallback);
    seed.fallback = true;
    seed.finish(&Err(anyhow::Error::new(ServiceSelectionMismatch)));
    assert_eq!(seed.quarantined, Some(4166));
    assert!(seed.dial_candidate(now)?.is_none());
    seed.quarantined = None;
    // A newer carrier key must not be dialled using the old carrier identity.
    endpoint.address.endpoint_key = ed25519_dalek::SigningKey::from_bytes(&[18; 32])
        .verifying_key()
        .to_bytes();
    let rotated = sign_relay_record_v1(
        &keys[0],
        &root.record_id,
        vec![endpoint.endpoint()?],
        now,
        now + 120_000,
        4167,
    )?;
    Discovery::update_seed(&mut seed, &rotated, pins, &keys[1], now)?;
    assert_eq!(
        seed.dial_candidate(now)?.unwrap().source,
        "latest_signed_record"
    );
    seed.book
        .as_mut()
        .unwrap()
        .revoke_self_owned(keys[0].public_key(), now)?;
    assert!(seed.dial_candidate(now)?.is_none());
    assert!(Discovery::update_seed(&mut seed, &root, pins, &keys[1], now).is_err());
    Ok(())
}
