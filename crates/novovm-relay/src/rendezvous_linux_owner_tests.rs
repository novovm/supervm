//! Process-isolated Linux credential/owner acceptance. No live credentials used.
use super::*;
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    process::Command,
    sync::atomic::Ordering,
    time::Duration,
};

#[test]
fn linux_custody_and_listener_restart() -> Result<()> {
    let root =
        std::env::temp_dir().join(format!("novovm-linux-owner-{:016x}", rand::random::<u64>()));
    std::fs::create_dir(&root)?;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    let credentials = root.join("credentials");
    std::fs::create_dir(&credentials)?;
    std::fs::set_permissions(&credentials, std::fs::Permissions::from_mode(0o700))?;
    create(&credentials.join("owner-key"), &[67; 32])?;
    let result = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "rendezvous_self_owned_owner::linux_tests::linux_owner_child",
            "--nocapture",
        ])
        .env("NOVOVM_OWNER_TEST_ROOT", &root)
        .env("CREDENTIALS_DIRECTORY", &credentials)
        .output()?;
    let _ = std::fs::remove_dir_all(&root);
    ensure!(
        result.status.success(),
        "Linux owner child failed: {}",
        String::from_utf8_lossy(&result.stdout)
    );
    Ok(())
}

#[tokio::test]
async fn linux_owner_child() -> Result<()> {
    let Some(root) = std::env::var_os("NOVOVM_OWNER_TEST_ROOT") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    let config = Config {
        version: 2,
        root: root.clone(),
        allowed_clients: [SigningKey::from_bytes(&[41; 32]).verifying_key().to_bytes()].into(),
        relays: vec![],
        nameserver: "127.0.0.1".parse()?,
        bind_address: Some("127.0.0.1:0".parse()?),
        relay_only: false,
        record_id: "linux-owner-fixture".into(),
    };
    let owner = Owner::open(config.clone(), true, None)?;
    let pin = owner.policy.node_pin;
    assert!(Owner::open(config.clone(), true, None).is_err());
    assert!(Owner::open(config.clone(), true, Some(serde_json::json!({}))).is_err());
    let native = NativeListenerConfig {
        version: 1,
        listen: "127.0.0.1:0".parse()?,
        advertised: "127.0.0.1:0".parse()?,
        max_connections: 4,
        max_bytes_per_minute: 64 * 1024 * 1024,
    };
    owner.set_carrier_mode(CarrierMode::DirectOnly)?;
    Owner::open(config.clone(), true, None)?.set_native_listener(Some(native))?;
    #[cfg(feature = "tls-transport")]
    {
        Owner::open(config.clone(), true, None)?.set_native_carrier_mailbox_v8(true)?;
        Owner::open(config.clone(), true, None)?.set_tls_listener(Some(TlsListenerConfig {
            version: 1,
            listen: "127.0.0.1:0".parse()?,
            advertised: "127.0.0.1:0".parse()?,
            max_connections: 4,
            native_mailbox_v8: true,
        }))?;
    }

    // Pending first activation is explicit; ordinary open cannot invent history.
    assert!(Owner::open(config.clone(), false, None).is_err());
    let sealed_keys = std::fs::read(root.join(KEYS_FILE))?;
    let original_policy = std::fs::read(root.join(POLICY_FILE))?;
    let credential_path = root.join("credentials/owner-key");
    let missing = credential_path.with_extension("missing");
    std::fs::rename(&credential_path, &missing)?;
    assert!(Owner::open(config.clone(), true, None).is_err());
    std::fs::rename(missing, &credential_path)?;
    for length in [31, 33] {
        std::fs::write(&credential_path, vec![67; length])?;
        assert!(Owner::open(config.clone(), true, None).is_err());
        assert_eq!(std::fs::read(root.join(KEYS_FILE))?, sealed_keys);
    }
    std::fs::write(&credential_path, [68; 32])?;
    assert!(Owner::open(config.clone(), true, None).is_err());
    assert_eq!(std::fs::read(root.join(KEYS_FILE))?, sealed_keys);
    assert_eq!(std::fs::read(root.join(POLICY_FILE))?, original_policy);
    std::fs::write(&credential_path, [67; 32])?;
    std::fs::set_permissions(&credential_path, std::fs::Permissions::from_mode(0o644))?;
    assert!(Owner::open(config.clone(), true, None).is_err());
    std::fs::set_permissions(&credential_path, std::fs::Permissions::from_mode(0o600))?;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755))?;
    assert!(Owner::open(config.clone(), true, None).is_err());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    let link = root.join("symlink-state");
    symlink(&root, &link)?;
    let mut invalid = config.clone();
    invalid.root = link.clone();
    assert!(Owner::open(invalid, true, None).is_err());
    std::fs::remove_file(link)?;
    let mut sequence = 0;
    for first in [true, false] {
        let owner = Owner::open(config.clone(), first, None)?;
        assert_eq!(owner.policy.node_pin, pin);
        let stop = Arc::new(AtomicBool::new(false));
        let control = stop.clone();
        let run = owner.run(stop);
        tokio::pin!(run);
        let observe = async {
            for _ in 0..500 {
                if let Ok(bytes) = read_bounded(&root.join("self-owned-record.json"), 65536) {
                    let record: novovm_network::product_directory::PeerSignedRelayRecordV1 =
                        serde_json::from_slice(&bytes)?;
                    if record.sequence > sequence {
                        assert_eq!(record.relay_public_key, pin);
                        sequence = record.sequence;
                        return Ok::<_, anyhow::Error>(());
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            anyhow::bail!("Linux native owner did not publish")
        };
        tokio::pin!(observe);
        tokio::select! {
            result = &mut run => { result?; anyhow::bail!("owner ended before readiness"); },
            result = &mut observe => { result?; },
        }
        control.store(true, Ordering::Release);
        let result = tokio::time::timeout(Duration::from_secs(5), run).await?;
        let expected_phase = if let Err(error) = result {
            ensure!(
                FailureEvidence::from_error(&error).stage == FailureStage::TransportRun
                    && error.root_cause().to_string() == "service stopped by owner",
                "unexpected owner stop: {error:#}"
            );
            "failed_closed"
        } else {
            "stopped"
        };
        let status: serde_json::Value =
            serde_json::from_slice(&read_bounded(&root.join("status.json"), 65536)?)?;
        assert_eq!(status["phase"], expected_phase);
    }
    assert_eq!(std::fs::read(root.join(KEYS_FILE))?, sealed_keys);
    // Missing committed history never triggers fresh keys or a new journal.
    std::fs::rename(root.join("service.journal"), root.join("service.saved"))?;
    assert!(Owner::open(config, true, None).is_err());
    assert!(!root.join("service.journal").exists());
    assert_eq!(std::fs::read(root.join(KEYS_FILE))?, sealed_keys);
    Ok(())
}
