use super::{input, Context, Result, Value};
use crate::native_pipeline::consensus::statement::ParentPoint;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn header_cache(device: &Path, profile: &Value, anchor: &ParentPoint) -> Result<PathBuf> {
    let genesis: [u8; 32] = serde_json::from_value(profile["genesis"].clone())?;
    let protocol: [u8; 32] = serde_json::from_value(profile["protocol"].clone())?;
    Ok(device
        .join("NOVOVM/native-discovery")
        .join(format!(
            "{}-{}-{}",
            profile["chain_id"],
            super::super::hex(&genesis),
            super::super::hex(&protocol)
        ))
        .join(format!(
            "headers-{}-{:x}",
            anchor.height,
            Sha256::digest(serde_json::to_vec(anchor)?)
        )))
}

#[test]
#[ignore = "requires the original h1/h2 UCA archives, installed profile, certified headers and Client"]
fn offline_uca_replay_binds_historical_archive_to_later_checkpoint() -> Result<()> {
    let binary = input("NOVOVM_QUERY_COPY_PROOF_CLIENT")?;
    let source = input("NOVOVM_UCA_CHECKPOINT_FIXTURE")?;
    let work = tempfile::tempdir()?;
    let device = work.path().join("device");
    let path = work.path().join("installed.json");
    let original = fs::read(source.join("installed-discovery.json"))?;
    let mut profile: Value = serde_json::from_slice(&original)?;
    let verification = &profile["verification"];
    assert!(verification["minimum_finalized_checkpoint"].is_null());
    let anchor: ParentPoint = serde_json::from_value(verification["trusted_parent"].clone())?;
    assert_eq!(anchor.height, 0);
    let headers = header_cache(&device, &profile, &anchor)?;
    let run = |profile_bytes: &[u8], height: u64| -> Result<std::process::Output> {
        fs::write(&path, profile_bytes)?;
        Ok(Command::new(&binary)
            .arg(&path)
            .arg("verify-uca-execution")
            .arg(format!("{:x}", Sha256::digest(profile_bytes)))
            .arg(source.join(format!("archive-height-{height}.nvreplay")))
            .env("LOCALAPPDATA", &device)
            .env("XDG_STATE_HOME", &device)
            .output()?)
    };
    let verify = |bytes: &[u8], height: u64| -> Result<Value> {
        let output = run(bytes, height)?;
        anyhow::ensure!(
            output.status.success(),
            "h{height} replay failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(value["verification"], "independent_uca_execution_replay");
        assert_eq!(value["height"], height);
        assert_eq!(value["parent_height"], height - 1);
        Ok(value)
    };
    let reject = |bytes: &[u8], expected: &str| -> Result<()> {
        let output = run(bytes, 1)?;
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::ensure!(
            !output.status.success() && output.stdout.is_empty() && error.contains(expected),
            "expected rejection {expected:?}; status={} stdout={} stderr={error}",
            output.status,
            String::from_utf8_lossy(&output.stdout)
        );
        println!("h1 rejected: {}", error.trim());
        Ok(())
    };

    let first = verify(&original, 1)?;
    assert!(!device.exists());
    fs::create_dir_all(&headers)?;
    let h1 = "00000000000000000001.proof";
    let h2 = "00000000000000000002.proof";
    fs::copy(source.join(h1), headers.join(h1))?;
    let second = verify(&original, 2)?;
    assert_eq!(
        first["receipt_batch_commitment"],
        "0xecf5d727891d35eecb91539ed2d49f2ba762a57780918198b47a55e0e3f70c11"
    );
    assert_eq!(
        second["receipt_batch_commitment"],
        "0x8dc7832a8074bfa97ce8a8bba7646e862fe9183e2138035de104bb77fcd72abf"
    );
    println!("original profile h1={first} h2={second}");

    let hash = second["block_hash"]
        .as_str()
        .context("missing h2 block hash")?;
    let hex = hash
        .strip_prefix("0x")
        .context("invalid block hash prefix")?;
    assert_eq!(
        hex,
        "7f2d438862e6bd2934862e4cafcb86dc3bf6ffa0a414d532014e420efa1aafc6"
    );
    let mut pin = [0u8; 32];
    for (i, byte) in pin.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    // Only this isolated copy gets a current policy window. All release trust roots are retained.
    let until = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())? + 120_000;
    profile["verification"]["minimum_finalized_checkpoint"] =
        json!({"height":2, "block_hash":pin, "valid_until_unix_ms":until});
    let correct = serde_json::to_vec(&profile)?;
    reject(
        &correct,
        "requires certified headers through the installed minimum checkpoint at height 2",
    )?;
    // A header bearing the expected block hash still needs its own valid QC.
    let mut damaged = fs::read(source.join(h2))?;
    *damaged.last_mut().context("empty h2 proof")? ^= 1;
    fs::write(headers.join(h2), damaged)?;
    reject(&correct, "invalid AOEM ML-DSA signature")?;
    fs::copy(source.join(h2), headers.join(h2))?;
    assert_eq!(verify(&correct, 1)?, first);
    println!(
        "h1 replay verified through the original certified h2, test checkpoint expiry {until}"
    );
    pin[0] ^= 1;
    profile["verification"]["minimum_finalized_checkpoint"]["block_hash"] = json!(pin);
    reject(
        &serde_json::to_vec(&profile)?,
        "conflicts with the installed minimum checkpoint at height 2",
    )?;
    Ok(())
}

#[test]
#[ignore = "requires the original h1/h2 certified headers, installed profile and Client"]
fn offline_activation_parent_verifies_minimum_before_exporting_requested_height() -> Result<()> {
    let binary = input("NOVOVM_QUERY_COPY_PROOF_CLIENT")?;
    let source = input("NOVOVM_UCA_CHECKPOINT_FIXTURE")?;
    let work = tempfile::tempdir()?;
    let device = work.path().join("device");
    let profile_path = work.path().join("installed.json");
    let output = work.path().join("parent.json");
    let original = fs::read(source.join("installed-discovery.json"))?;
    let mut profile: Value = serde_json::from_slice(&original)?;
    let anchor: ParentPoint =
        serde_json::from_value(profile["verification"]["trusted_parent"].clone())?;
    assert_eq!(anchor.height, 0);
    assert!(profile["verification"]["minimum_finalized_checkpoint"].is_null());
    let headers = header_cache(&device, &profile, &anchor)?;
    fs::create_dir_all(&headers)?;
    let h1 = "00000000000000000001.proof";
    let h2 = "00000000000000000002.proof";
    fs::copy(source.join(h1), headers.join(h1))?;

    let run = |bytes: &[u8], height: u64| -> Result<std::process::Output> {
        fs::write(&profile_path, bytes)?;
        Ok(Command::new(&binary)
            .arg(&profile_path)
            .arg("activation-parent")
            .arg(height.to_string())
            .arg(format!("{:x}", Sha256::digest(bytes)))
            .arg(&output)
            .env("LOCALAPPDATA", &device)
            .env("XDG_STATE_HOME", &device)
            .output()?)
    };
    let export = |bytes: &[u8], height: u64| -> Result<ParentPoint> {
        let child = run(bytes, height)?;
        anyhow::ensure!(
            child.status.success(),
            "h{height}: {}",
            String::from_utf8_lossy(&child.stderr)
        );
        let report: Value = serde_json::from_slice(&child.stdout)?;
        let data = fs::read(&output)?;
        fs::remove_file(&output)?;
        let point: ParentPoint = serde_json::from_slice(&data)?;
        assert_eq!(point.height, height);
        assert_eq!(report["height"], height);
        assert_eq!(report["sha256"], format!("{:x}", Sha256::digest(&data)));
        println!(
            "exported h{height} {}",
            super::super::hex(&point.block_hash)
        );
        Ok(point)
    };
    let reject = |bytes: &[u8], height: u64, expected: &str| -> Result<()> {
        let child = run(bytes, height)?;
        let stderr = String::from_utf8_lossy(&child.stderr);
        anyhow::ensure!(
            !child.status.success()
                && child.stdout.is_empty()
                && !output.try_exists()?
                && stderr.contains(expected),
            "h{height}: expected {expected}, status={}, output exists={}, stderr={stderr}",
            child.status,
            output.exists()
        );
        println!("h{height} export refused without output: {}", stderr.trim());
        Ok(())
    };

    assert_eq!(export(&original, 0)?, anchor);
    let first = export(&original, 1)?;
    fs::copy(source.join(h2), headers.join(h2))?;
    let second = export(&original, 2)?;
    assert_eq!(
        super::super::hex(&second.block_hash),
        "0x7f2d438862e6bd2934862e4cafcb86dc3bf6ffa0a414d532014e420efa1aafc6"
    );
    let until = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())? + 120_000;
    profile["verification"]["minimum_finalized_checkpoint"] = json!({
        "height":2, "block_hash":second.block_hash, "valid_until_unix_ms":until
    });
    let pinned = serde_json::to_vec(&profile)?;
    fs::remove_file(headers.join(h2))?;
    for height in [0, 1] {
        reject(
            &pinned,
            height,
            "requires certified headers through the installed minimum checkpoint at height 2",
        )?;
    }
    let mut damaged = fs::read(source.join(h2))?;
    *damaged.last_mut().context("empty h2 proof")? ^= 1;
    fs::write(headers.join(h2), damaged)?;
    reject(&pinned, 1, "invalid AOEM ML-DSA signature")?;
    fs::copy(source.join(h2), headers.join(h2))?;
    assert_eq!(export(&pinned, 0)?, anchor);
    assert_eq!(export(&pinned, 1)?, first);
    assert_eq!(export(&pinned, 2)?, second);
    fs::write(&output, b"existing caller output")?;
    let already_exists = run(&pinned, 1)?;
    assert!(!already_exists.status.success() && already_exists.stdout.is_empty());
    assert_eq!(fs::read(&output)?, b"existing caller output");
    fs::remove_file(&output)?;
    let mut expired = profile.clone();
    expired["verification"]["minimum_finalized_checkpoint"]["valid_until_unix_ms"] = json!(1);
    reject(&serde_json::to_vec(&expired)?, 1, "expired")?;
    let mut bad = second.block_hash;
    bad[0] ^= 1;
    profile["verification"]["minimum_finalized_checkpoint"]["block_hash"] = json!(bad);
    for height in [0, 1, 2] {
        reject(
            &serde_json::to_vec(&profile)?,
            height,
            "conflicts with the installed minimum checkpoint at height 2",
        )?;
    }
    // The anchor itself is a sufficient proof when it is the pinned height.
    profile["verification"]["trusted_parent"] = json!(first);
    profile["verification"]["minimum_finalized_checkpoint"] = json!({
        "height":1, "block_hash":first.block_hash, "valid_until_unix_ms":until
    });
    assert!(!header_cache(&device, &profile, &first)?.try_exists()?);
    assert_eq!(export(&serde_json::to_vec(&profile)?, 1)?, first);
    profile["verification"]["minimum_finalized_checkpoint"]["block_hash"] = json!(bad);
    reject(
        &serde_json::to_vec(&profile)?,
        1,
        "conflicts with the installed minimum checkpoint at height 1",
    )?;
    Ok(())
}
