use super::{input, Context, Result, Value};
use crate::native_pipeline::consensus::statement::ParentPoint;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

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
    let genesis: [u8; 32] = serde_json::from_value(profile["genesis"].clone())?;
    let protocol: [u8; 32] = serde_json::from_value(profile["protocol"].clone())?;
    let headers = device
        .join("NOVOVM/native-discovery")
        .join(format!(
            "{}-{}-{}",
            profile["chain_id"],
            super::super::hex(&genesis),
            super::super::hex(&protocol)
        ))
        .join(format!(
            "headers-0-{:x}",
            Sha256::digest(serde_json::to_vec(&anchor)?)
        ));
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
