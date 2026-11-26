use super::*;
use crate::native_pipeline::consensus::statement::ParentPoint;
use crate::native_pipeline::service::network::test_slow_cached_receipt;

pub(super) fn verify(
    owner: &mut QueryOwner,
    binary: &Path,
    profile: &Path,
    pin: &str,
    installed: &Value,
    provider: &str,
) -> Result<()> {
    let stage = profile.parent().context("cache fixture stage")?;
    let device = stage.join("historical-cache-device");
    ensure!(
        !device.exists(),
        "historical cache fixture already populated"
    );
    let run = |owner: &mut QueryOwner,
               cfg: &Path,
               args: &[&str],
               label: &str|
     -> Result<std::process::Output> {
        let mut command = Command::new(binary);
        command
            .arg(cfg)
            .args(args)
            .env("LOCALAPPDATA", &device)
            .env("XDG_STATE_HOME", &device);
        let worker = thread::spawn(move || command.output());
        owner.until(|_| Ok(worker.is_finished()))?;
        let output = worker
            .join()
            .map_err(|_| anyhow::anyhow!("historical cache Client panicked"))??;
        fs::write(stage.join(format!("{label}.json")), &output.stdout)?;
        fs::write(stage.join(format!("{label}.stderr")), &output.stderr)?;
        Ok(output)
    };
    let sync = run(owner, profile, &["sync-chain"], "header-sync")?;
    ensure!(
        sync.status.success(),
        "header sync failed: {}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let sync: Value = serde_json::from_slice(&sync.stdout)?;
    ensure!(
        sync["verified_height"] == 2,
        "cache lacks both original headers"
    );
    let anchor: ParentPoint =
        serde_json::from_value(installed["verification"]["trusted_parent"].clone())?;
    let genesis: [u8; 32] = serde_json::from_value(installed["genesis"].clone())?;
    let protocol: [u8; 32] = serde_json::from_value(installed["protocol"].clone())?;
    let hex = super::super::super::hex;
    let headers = device
        .join("NOVOVM/native-discovery")
        .join(format!(
            "{}-{}-{}",
            installed["chain_id"],
            hex(&genesis),
            hex(&protocol)
        ))
        .join(format!(
            "headers-{}-{:x}",
            anchor.height,
            Sha256::digest(serde_json::to_vec(&anchor)?)
        ));
    let first_path = headers.join(format!("{:020}.proof", 1));
    let second_path = headers.join(format!("{:020}.proof", 2));
    let original = fs::read(&first_path)?;
    let second = fs::read(&second_path)?;
    ensure!(
        !original.is_empty() && !second.is_empty(),
        "both original signed headers required"
    );
    let profile_path = profile.to_owned();
    let expected_pin = pin.to_owned();
    let cache_root = device.clone();
    let worker = thread::spawn(move || {
        test_slow_cached_receipt(&profile_path, &expected_pin, &cache_root, SECOND_HASH, 2)
    });
    owner.until(|_| Ok(worker.is_finished()))?;
    let report = worker
        .join()
        .map_err(|_| anyhow::anyhow!("slow cache verifier panicked"))??;
    fs::write(
        stage.join("slow-cached-receipt.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    ensure!(
        report["local_verification_elapsed_ms"]
            .as_u64()
            .is_some_and(|ms| ms >= 11_000)
            && report["authenticated_discovery_sources"] == serde_json::json!([provider])
            && report["serving_transport_sha256"] == provider
            && report["independently_verified_finality"] == true
            && report["verified_parent_state_root"] == FIRST_ROOT
            && report["verified_parent_height"] == 1
            && report["tx_hash"] == SECOND_HASH
            && report["height"] == 2
            && report["block_hash"] == SECOND_BLOCK
            && report["state_root"] == SECOND_ROOT,
        "slow verification lost the original receipt or the minimum delay: {report}"
    );
    let mut conflict = installed.clone();
    let mut wrong = owner.rpc.node.controller.parent().block_hash;
    wrong[0] ^= 1;
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    conflict["verification"]["minimum_finalized_checkpoint"] =
        serde_json::json!({"height":2,"block_hash":wrong,"valid_until_unix_ms":now+1800000});
    let bad_profile = stage.join("conflicting-checkpoint.json");
    let bytes = serde_json::to_vec(&conflict)?;
    let bad_pin = format!("{:x}", Sha256::digest(&bytes));
    fs::write(&bad_profile, bytes)?;
    let rejected = run(
        owner,
        &bad_profile,
        &[
            "discover-verified-receipt",
            "0x68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4",
            &bad_pin,
            "--height",
            "1",
        ],
        "checkpoint-conflict",
    )?;
    ensure!(
        !rejected.status.success()
            && rejected.stdout.is_empty()
            && String::from_utf8_lossy(&rejected.stderr)
                .contains("conflicts with the installed minimum checkpoint"),
        "valid cached branch bypassed a conflicting checkpoint: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    let mut altered = original.clone();
    *altered.last_mut().context("empty first header")? ^= 1;
    fs::write(&first_path, altered)?;
    let result = run(
        owner,
        profile,
        &[
            "discover-verified-receipt",
            SECOND_HASH,
            pin,
            "--height",
            "2",
        ],
        "tampered-parent",
    );
    fs::write(&first_path, &original)?;
    let rejected = result?;
    ensure!(
        !rejected.status.success()
            && rejected.stdout.is_empty()
            && String::from_utf8_lossy(&rejected.stderr)
                .contains("verifying stored header at height 1"),
        "damaged parent did not fail local header verification: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    ensure!(
        fs::read(first_path)? == original && fs::read(second_path)? == second,
        "isolated header recovery changed the fixture"
    );
    Ok(())
}
