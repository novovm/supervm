use super::*;
use crate::native_pipeline::service::config::test_custody::TestCredentialGuard;
use novovm_relay::transport_identity::ephemeral_transport_identity;
use std::{net::UdpSocket, process::Command};

#[test]
#[ignore = "requires approved AOEM and the packaged native Client; uses loopback only"]
fn clean_device_authenticates_through_second_source_and_preserves_custody() -> Result<()> {
    run_first_contact(LocatorRoute::Reachable)
}

#[test]
#[ignore = "requires approved AOEM and the packaged native Client; uses loopback only"]
fn clean_device_recovers_expired_bootstrap_files_through_pinned_contacts() -> Result<()> {
    run_first_contact(LocatorRoute::Expired)
}

#[test]
#[ignore = "requires approved AOEM and the packaged native Client; uses loopback only"]
fn clean_device_recovers_retired_address_before_signed_locator_expires() -> Result<()> {
    run_first_contact(LocatorRoute::Retired)
}

#[test]
#[ignore = "requires approved AOEM and the native Client; uses loopback only"]
fn clean_device_recovers_rotated_endpoint_before_signed_locator_expires() -> Result<()> {
    run_first_contact(LocatorRoute::Rotated)
}

#[test]
#[ignore = "requires approved AOEM and the native Client; uses loopback only"]
fn clean_device_preserves_slow_signed_locator_with_unreachable_contact() -> Result<()> {
    run_first_contact(LocatorRoute::Slow)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocatorRoute {
    Reachable,
    Expired,
    Retired,
    Rotated,
    Slow,
}

fn run_first_contact(route: LocatorRoute) -> Result<()> {
    let expired = route == LocatorRoute::Expired;
    let rotated = route == LocatorRoute::Rotated;
    let slow = route == LocatorRoute::Slow;
    let uses_contact = route != LocatorRoute::Reachable;
    let library = PathBuf::from(std::env::var_os("NOVOVM_DISCOVERY_AOEM").context("AOEM path")?)
        .canonicalize()?;
    let digest = std::env::var("NOVOVM_DISCOVERY_AOEM_SHA256")?;
    let client = PathBuf::from(std::env::var_os("NOVOVM_DISCOVERY_CLIENT").context("Client path")?)
        .canonicalize()?;
    let root = std::env::temp_dir().join(format!("novovm-discover-{:016x}", rand::random::<u64>()));
    fs::create_dir(&root)?;
    let evidence_root = root.clone();
    let _credential = TestCredentialGuard::new(&root)?;
    let (unreachable, _) = ephemeral_transport_identity(&library, &digest)?;
    let (responsive, _) = ephemeral_transport_identity(&library, &digest)?;
    let loopback = "127.0.0.1".parse()?;
    let data = root.join("clean-device");
    let mut profile = Profile {
        chain_id: 170_415,
        genesis: [0x25; 32],
        protocol: [0x91; 32],
        library,
        library_sha256: digest,
        nameserver: loopback,
        relays: Vec::new(),
        seeds: [
            SeedPin {
                locator: Some("primary.json".into()),
                contact: None,
                transport_key: unreachable.public_key().to_vec(),
            },
            SeedPin {
                locator: Some("secondary.json".into()),
                contact: None,
                transport_key: responsive.public_key().to_vec(),
            },
        ],
        lookup_transport_keys: vec![responsive.public_key().to_vec()],
        native_query_peers: Vec::new(),
        verification: None,
    };
    let pins = profile.pins();
    let sink = UdpSocket::bind("127.0.0.1:0")?;
    let carrier = IrohConfigV1::new(&[], loopback)?.with_bind_address("127.0.0.1:0".parse()?)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let server = IrohEndpointOwnerV1::bind(carrier, || Ok(())).await?;
        let proof = server
            .with_continuous_service_v4(|| Ok(()), |scope| Box::pin(async move {
                let instance = [0x84; 32];
                let store = Arc::new(Mutex::new(RendezvousStoreV1::new(instance, Instant::now() + Duration::from_secs(120))?));
                let now = wall_ms()?;
                let primary_contact = IrohAddressV1 {
                    endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[67; 32]).verifying_key().to_bytes(),
                    direct: vec![sink.local_addr()?],
                    relays: Vec::new(),
                };
                let (issued, expiry) = if expired { (now - 120_000, now - 1) } else { (now, now + 120_000) };
                let primary = sign_relay_record_v1(
                    &unreachable,
                    native_locator_id_v1(pins),
                    vec![IrohServiceEndpointV1 {
                        instance: [0x78; 32],
                        address: primary_contact.clone(),
                    }.endpoint()?],
                    issued, expiry, 1,
                )?;
                let mut address = scope.address()?;
                address.direct.retain(|entry| entry.ip().is_loopback());
                let endpoint = IrohServiceEndpointV1 { instance, address: address.clone() }.endpoint()?;
                let secondary = sign_relay_record_v1(
                    &responsive,
                    native_locator_id_v1(pins),
                    vec![endpoint.clone()],
                    now, now + 120_000, 7,
                )?;
                let local_secondary = if uses_contact && !slow {
                    let mut obsolete_address = address.clone();
                    obsolete_address.direct = vec![sink.local_addr()?];
                    if rotated {
                        obsolete_address.endpoint_key = ed25519_dalek::SigningKey::from_bytes(&[69; 32])
                            .verifying_key()
                            .to_bytes();
                        assert_ne!(obsolete_address.endpoint_key, address.endpoint_key);
                    }
                    let obsolete = IrohServiceEndpointV1 {instance, address: obsolete_address}.endpoint()?;
                    sign_relay_record_v1(
                        &responsive, native_locator_id_v1(pins), vec![obsolete], issued, expiry, 6,
                    )?
                } else {
                    secondary.clone()
                };
                let owner_id = node_owner_id_v1(responsive.public_key())?;
                let locator = encode_node_locator_v1(&secondary, pins)?;
                fs::write(root.join("primary.json"), serde_json::to_vec(&primary)?)?;
                fs::write(root.join("secondary.json"), serde_json::to_vec(&local_secondary)?)?;
                let file = root.join("discovery.json");
                let legacy = root.join("legacy.json");
                fs::write(&legacy, serde_json::to_vec(&profile)?)?;
                if uses_contact {
                    profile.seeds[0].contact = Some(primary_contact);
                    profile.seeds[1].contact = Some(if slow {
                        IrohAddressV1 {
                            endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[69; 32])
                                .verifying_key()
                                .to_bytes(),
                            direct: vec![sink.local_addr()?],
                            relays: Vec::new(),
                        }
                    } else {
                        address
                    });
                }
                fs::write(&file, serde_json::to_vec(&profile)?)?;
                let expected_source = fingerprint(responsive.public_key());
                let client_task = async move {
                    if !expired && !rotated && !slow {
                        verify_active_refresh(&profile, &root, &expected_source).await?;
                    }
                    tokio::task::spawn_blocking(move || -> Result<()> {
                        assert!(!data.exists());
                        if uses_contact && !slow {
                            let rejected = run_client(&client, &legacy, &root.join("legacy-device"), None);
                            assert!(format!("{:#}", rejected.unwrap_err()).contains("no authenticated NOVONET bootstrap"));
                        }
                        let started = Instant::now();
                        let pin = fingerprint(&fs::read(&file)?);
                        let rejected = run_client(&client, &file, &data, Some(&"0".repeat(64)));
                        assert!(format!("{:#}", rejected.unwrap_err()).contains("differs from its release pin"));
                        assert!(!data.exists());
                        let first = run_client(&client, &file, &data, Some(&pin))?;
                        assert_eq!(first["profile_sha256"], pin);
                        if rotated {
                            assert!(wall_ms()? < expiry, "rotation recovered after locator expiry");
                            println!(
                                "still-valid old endpoint recovered through the distinct installed contact in {:?}",
                                started.elapsed()
                            );
                        }
                        if slow {
                            assert!(wall_ms()? < expiry, "slow signed locator expired during lookup");
                        }
                        let restarted = run_client(&client, &file, &data, None)?;
                        assert!(restarted.get("profile_sha256").is_none());
                        let sources = serde_json::json!([expected_source]);
                        for report in [&first, &restarted] {
                            assert_eq!(report["authenticated_discovery_sources"], sources);
                            assert_eq!(report["signed_locator_candidates"][0]["sequence"], 7);
                            assert_eq!(
                                report["signed_locators_by_source"],
                                serde_json::json!({ (expected_source.as_str()): report["signed_locator_candidates"] })
                            );
                        }
                        let fingerprint = &first["transport_identity_sha256"];
                        assert_eq!(*fingerprint, restarted["transport_identity_sha256"]);
                        assert_ne!(*fingerprint, sources[0]);
                        if expired {
                            fs::remove_file(root.join("primary.json"))?;
                            fs::remove_file(root.join("secondary.json"))?;
                            let cold = run_client(&client, &file, &root.join("contact-only-device"), None)?;
                            assert_eq!(cold["authenticated_discovery_sources"], sources);
                            assert_eq!(cold["signed_locator_candidates"][0]["sequence"], 7);
                            println!("expired files with retired routes could not reach a source; second pinned contact retrieved a fresh signed locator with expired files and with both files absent");
                        } else if uses_contact && !rotated && !slow {
                            println!("still-valid signed locator with a retired address used the pinned coordinates for the same Iroh and NOVONET identities");
                        }
                        println!("authenticated through secondary source, one signed candidate, stable transport identity across two Client processes");
                        Ok(())
                    }).await?
                };
                let service = async {
                    for revision in 1..=10 {
                        let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
                        assert_eq!(stream.read_frame().await?, DISCOVERY_ENTRY);
                        if slow {
                            let delay = INITIAL_SOURCE_BUDGET / 2 + Duration::from_secs(1);
                            tokio::time::sleep(delay).await;
                            println!("signed route held its pre-authentication response for {delay:?}");
                        }
                        store.lock().unwrap().put_node_locator_v1(
                            owner_id, revision, Duration::from_secs(30), locator.clone(), Instant::now(),
                        )?;
                        let served = serve(stream, responsive.clone(), pins, store.clone()).await;
                        // The old half-budget Client cancels this stream before the delay.
                        if !slow {
                            served.context("serving the responsive test source")?;
                        }
                    }
                    anyhow::bail!("fixture bootstrap request budget exhausted")
                };
                tokio::select! {
                    result = client_task => result,
                    result = service => result,
                }
            })).await;
        let closed = server.close().await;
        proof.and(closed)
    });
    println!(
        "isolated discovery fixture retained at {}",
        evidence_root.display()
    );
    result
}

#[test]
fn installed_contact_preserves_endpoint_binding_and_bounded_coordinates() {
    let signed: Vec<_> = (1..=8).map(|port| ([127, 0, 0, 1], port).into()).collect();
    let contact_only = ([127, 0, 0, 1], 9).into();
    let mut route = IrohAddressV1 {
        endpoint_key: [7; 32],
        direct: signed.clone(),
        relays: vec![],
    };
    let mut installed = IrohAddressV1 {
        endpoint_key: [9; 32],
        direct: vec![signed[0], contact_only],
        relays: vec![],
    };
    add_installed_contact(&mut route, &installed);
    assert_eq!(route.direct, signed);
    installed.endpoint_key = route.endpoint_key;
    add_installed_contact(&mut route, &installed);
    assert_eq!(route.direct, signed);
    route.direct.pop();
    add_installed_contact(&mut route, &installed);
    assert_eq!(&route.direct[..7], &signed[..7]);
    assert_eq!(route.direct[7], contact_only);
    assert_eq!(route.direct.len(), 8);
}

async fn verify_active_refresh(profile: &Profile, root: &Path, source: &str) -> Result<()> {
    let (identity, _) = ephemeral_transport_identity(&profile.library, &profile.library_sha256)?;
    let candidates = prepare_seed_candidates(profile, &identity, read_seeds(root, profile))?;
    let client = bind_carrier(
        IrohConfigV1::new(&[], profile.nameserver)?.with_bind_address("127.0.0.1:0".parse()?)?,
    )
    .await?;
    let started = Instant::now();
    let report = client
        .with_continuous_service_v4(
            || Ok(()),
            |scope| {
                Box::pin(lookup_sources(
                    scope,
                    identity,
                    profile.pins(),
                    candidates,
                    profile.lookup_transport_keys.clone(),
                    REFRESH_SOURCE_BUDGET,
                ))
            },
        )
        .await;
    let elapsed = started.elapsed();
    let closed = client.close().await;
    let report = report.context("refresh through the active Client owner")?;
    closed.context("closing the active Client owner")?;
    ensure!(
        elapsed >= REFRESH_SOURCE_BUDGET
            && report.sources == [source]
            && report.records.len() == 1
            && report.records.values().all(|record| record.sequence == 7),
        "stalled first source did not leave the valid secondary locator available"
    );
    println!(
        "active refresh retained signed locator sequence 7 after the first source exhausted its deadline in {elapsed:?}"
    );
    Ok(())
}

fn run_client(
    client: &Path,
    profile: &Path,
    data: &Path,
    pin: Option<&str>,
) -> Result<serde_json::Value> {
    let output = Command::new(client)
        .arg(profile)
        .arg("discover")
        .args(pin)
        .env("LOCALAPPDATA", data)
        .env("XDG_STATE_HOME", data)
        .output()?;
    ensure!(
        output.status.success(),
        "Client discovery failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
#[ignore = "requires the approved AOEM runtime; no sockets or device custody"]
fn release_preflight_verifies_signed_bootstrap_locators_offline() -> Result<()> {
    let library = PathBuf::from(std::env::var_os("NOVOVM_DISCOVERY_AOEM").context("AOEM path")?)
        .canonicalize()?;
    let digest = std::env::var("NOVOVM_DISCOVERY_AOEM_SHA256")?;
    let (first, _) = ephemeral_transport_identity(&library, &digest)?;
    let (second, _) = ephemeral_transport_identity(&library, &digest)?;
    let (validator, _) = ephemeral_transport_identity(&library, &digest)?;
    let root =
        std::env::temp_dir().join(format!("novovm-preflight-{:016x}", rand::random::<u64>()));
    fs::create_dir(&root)?;
    let path = root.join("installed-discovery.json");
    let chain_id = 170_415;
    let validator_set = ValidatorSet::new(
        chain_id,
        1,
        1,
        vec![Validator::new(validator.public_key(), 1)?],
    )?;
    let mut profile = Profile {
        chain_id,
        genesis: [0x25; 32],
        protocol: [0x91; 32],
        library,
        library_sha256: digest,
        nameserver: "127.0.0.1".parse()?,
        relays: Vec::new(),
        seeds: [
            SeedPin {
                locator: Some("primary.json".into()),
                contact: None,
                transport_key: first.public_key().to_vec(),
            },
            SeedPin {
                locator: Some("secondary.json".into()),
                contact: None,
                transport_key: second.public_key().to_vec(),
            },
        ],
        lookup_transport_keys: vec![first.public_key().to_vec()],
        native_query_peers: Vec::new(),
        verification: Some(VerificationPins {
            validator_epoch: 1,
            validator_set_hash: validator_set.hash(),
            validators: vec![GenesisValidator {
                public_key: validator.public_key().to_vec(),
                transport_public_key: first.public_key().to_vec(),
                weight: 1,
            }],
            trusted_parent: ParentPoint {
                height: 0,
                block_hash: [0; 32],
                state_root: [1; 32],
                receipt_batch_commitment: [2; 32],
                state_version: 0,
                decision_hash: [0; 32],
            },
            minimum_finalized_checkpoint: None,
        }),
    };
    let primary = IrohAddressV1 {
        endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[67; 32])
            .verifying_key()
            .to_bytes(),
        direct: vec!["127.0.0.1:43111".parse()?],
        relays: Vec::new(),
    };
    let secondary = IrohAddressV1 {
        endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[68; 32])
            .verifying_key()
            .to_bytes(),
        direct: vec!["127.0.0.1:43112".parse()?],
        relays: Vec::new(),
    };
    let pins = profile.pins();
    let now = wall_ms()?;
    let make_record = |identity: &TransportIdentity, pins, route: &IrohAddressV1, expires| {
        sign_relay_record_v1(
            identity,
            native_locator_id_v1(pins),
            vec![IrohServiceEndpointV1 {
                instance: [0x78; 32],
                address: route.clone(),
            }
            .endpoint()?],
            now - 120_000,
            expires,
            1,
        )
        .map_err(anyhow::Error::from)
    };
    let write_locator = |name: &str, record: &PeerSignedRelayRecordV1| -> Result<()> {
        fs::write(root.join(name), serde_json::to_vec(record)?)?;
        Ok(())
    };
    let check = |profile: &Profile| {
        let bytes = serde_json::to_vec(profile)?;
        fs::write(&path, &bytes)?;
        check_installed_profile(&path, &fingerprint(&bytes))
    };

    write_locator(
        "secondary.json",
        &make_record(&second, pins, &secondary, now + 120_000)?,
    )?;
    let current = make_record(&first, pins, &primary, now + 120_000)?;
    write_locator("primary.json", &current)?;
    assert_eq!(check(&profile)?["chain_id"], chain_id);

    let expired = make_record(&first, pins, &secondary, now - 1)?;
    write_locator("primary.json", &expired)?;
    assert!(check(&profile).unwrap_err().to_string().contains("expired"));
    profile.seeds[0].contact = Some(primary.clone());
    assert_eq!(check(&profile)?["trusted_height"], 0);

    let mut bad_signature = expired;
    bad_signature.signature[0] ^= 1;
    write_locator("primary.json", &bad_signature)?;
    assert!(check(&profile)
        .unwrap_err()
        .to_string()
        .contains("signature"));

    for wrong in [
        make_record(
            &first,
            (chain_id + 1, pins.1, pins.2),
            &primary,
            now + 120_000,
        )?,
        make_record(&second, pins, &primary, now + 120_000)?,
    ] {
        write_locator("primary.json", &wrong)?;
        assert!(check(&profile)
            .unwrap_err()
            .to_string()
            .contains("signer or chain domain"));
    }

    write_locator("primary.json", &current)?;
    write_locator(
        "secondary.json",
        &make_record(&second, pins, &primary, now + 120_000)?,
    )?;
    assert!(check(&profile)
        .unwrap_err()
        .to_string()
        .contains("distinct source"));
    println!("approved AOEM verified current and historical locators, and rejected expiry without contact, bad signature, wrong signer/domain and overlapping endpoints");
    Ok(())
}
