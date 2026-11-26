use super::*;
use crate::native_pipeline::service::config::test_custody::TestCredentialGuard;
use novovm_relay::transport_identity::ephemeral_transport_identity;
use std::{net::UdpSocket, process::Command};

#[test]
#[ignore = "requires approved AOEM and the packaged native Client; uses loopback only"]
fn clean_device_authenticates_through_second_source_and_preserves_custody() -> Result<()> {
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
    let profile = Profile {
        chain_id: 170_415,
        genesis: [0x25; 32],
        protocol: [0x91; 32],
        library,
        library_sha256: digest,
        nameserver: loopback,
        relays: Vec::new(),
        seeds: [
            SeedPin {
                locator: "primary.json".into(),
                transport_key: unreachable.public_key().to_vec(),
            },
            SeedPin {
                locator: "secondary.json".into(),
                transport_key: responsive.public_key().to_vec(),
            },
        ],
        lookup_transport_keys: vec![responsive.public_key().to_vec()],
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
                let primary = sign_relay_record_v1(
                    &unreachable,
                    native_locator_id_v1(pins),
                    vec![IrohServiceEndpointV1 {
                        instance: [0x78; 32],
                        address: IrohAddressV1 {
                            endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[67; 32]).verifying_key().to_bytes(),
                            direct: vec![sink.local_addr()?],
                            relays: Vec::new(),
                        },
                    }.endpoint()?],
                    now, now + 120_000, 1,
                )?;
                let mut address = scope.address()?;
                address.direct.retain(|entry| entry.ip().is_loopback());
                let secondary = sign_relay_record_v1(
                    &responsive,
                    native_locator_id_v1(pins),
                    vec![IrohServiceEndpointV1 { instance, address }.endpoint()?],
                    now, now + 120_000, 7,
                )?;
                let owner_id = node_owner_id_v1(responsive.public_key())?;
                let locator = encode_node_locator_v1(&secondary, pins)?;
                fs::write(root.join(&profile.seeds[0].locator), serde_json::to_vec(&primary)?)?;
                fs::write(root.join(&profile.seeds[1].locator), serde_json::to_vec(&secondary)?)?;
                let file = root.join("discovery.json");
                fs::write(&file, serde_json::to_vec(&profile)?)?;
                let expected_source = fingerprint(responsive.public_key());
                let mut client_task = tokio::task::spawn_blocking(move || -> Result<()> {
                    assert!(!data.exists());
                    let first = run_client(&client, &file, &data)?;
                    let restarted = run_client(&client, &file, &data)?;
                    let sources = serde_json::json!([expected_source]);
                    for report in [&first, &restarted] {
                        assert_eq!(report["authenticated_discovery_sources"], sources);
                        assert_eq!(report["signed_locator_candidates"][0]["sequence"], 7);
                    }
                    let fingerprint = &first["transport_identity_sha256"];
                    assert_eq!(*fingerprint, restarted["transport_identity_sha256"]);
                    assert_ne!(*fingerprint, sources[0]);
                    println!("authenticated through secondary source, one signed candidate, stable transport identity across two Client processes");
                    Ok(())
                });
                let mut revision = 0;
                loop {
                    tokio::select! {
                        result = &mut client_task => break result?,
                        stream = scope.accept_inbound() => {
                            let mut stream: CarrierStreamV1<'_> = stream?.into();
                            assert_eq!(stream.read_frame().await?, DISCOVERY_ENTRY);
                            revision += 1;
                            store.lock().unwrap().put_node_locator_v1(
                                owner_id, revision, Duration::from_secs(30), locator.clone(), Instant::now(),
                            )?;
                            serve(stream, responsive.clone(), pins, store.clone()).await?;
                        }
                    }
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

fn run_client(client: &Path, profile: &Path, data: &Path) -> Result<serde_json::Value> {
    let output = Command::new(client)
        .arg(profile)
        .arg("discover")
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
