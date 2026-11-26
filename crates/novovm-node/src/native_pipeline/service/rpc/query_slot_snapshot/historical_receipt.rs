//! Actual UCA archive of the detached two-block chain, queried by cold Windows Clients.
mod cached;
use super::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::process::Stdio;

const SECOND_HASH: &str = "0x2d0436f0893d5a6e1e996470a2fd223b3b256a11a6240e39418078e9be9a5f3d";
const FIRST_ROOT: &str = "0x4223c7609d20861ed8e549382ea279a685f3c3a742ee917d334ada9acb44cca0";
const SECOND_BLOCK: &str = "0x7f2d438862e6bd2934862e4cafcb86dc3bf6ffa0a414d532014e420efa1aafc6";
const SECOND_ROOT: &str = "0x418cb1c74c5121d43d4a392a309a052d0f6a8a513bc9377b6aa503d80591e439";

pub(super) fn verify(
    owner: &mut QueryOwner,
    binary: &Path,
    profile: &Path,
    provider: &[u8],
    first_hash: &str,
) -> Result<()> {
    let stage = profile.parent().context("historical receipt stage")?;
    let profile_bytes = fs::read(profile)?;
    let pin = format!("{:x}", Sha256::digest(&profile_bytes));
    let provider = format!("{:x}", Sha256::digest(provider));
    let installed: Value = serde_json::from_slice(&profile_bytes)?;
    ensure!(
        installed["verification"]["trusted_parent"]["height"] == 0,
        "fixture must start at genesis"
    );

    if std::env::var("NOVOVM_QUERY_COPY_HISTORICAL_CACHE").as_deref() == Ok("1") {
        return cached::verify(owner, binary, profile, &pin, &installed, &provider);
    }
    let unknown = format!(
        "0x{:x}",
        Sha256::digest(b"historical receipt fixture missing index key")
    );
    for (label, hash, options) in [
        ("first-default", first_hash, &[][..]),
        ("second-explicit", SECOND_HASH, &["--height", "2"][..]),
        ("second-lookup", SECOND_HASH, &["--lookup-height"][..]),
        ("second-wrong-height", SECOND_HASH, &["--height", "1"][..]),
        ("unknown-lookup", unknown.as_str(), &["--lookup-height"][..]),
    ] {
        let device = stage.join(format!("{label}-device"));
        let stdout = stage.join(format!("{label}.json"));
        let stderr = stage.join(format!("{label}.stderr"));
        ensure!(
            !device.exists(),
            "historical receipt Client cannot use a warm parent or custody"
        );
        let mut child = IsolatedFollowChild(
            Command::new(binary)
                .arg(profile)
                .arg("discover-verified-receipt")
                .arg(hash)
                .arg(&pin)
                .args(options)
                .env("LOCALAPPDATA", &device)
                .env("XDG_STATE_HOME", &device)
                .stdout(Stdio::from(fs::File::create_new(&stdout)?))
                .stderr(Stdio::from(fs::File::create_new(&stderr)?))
                .spawn()?,
        );
        let mut status = None;
        owner.until(|_| {
            status = child.0.try_wait()?;
            Ok(status.is_some())
        })?;
        let ok = status
            .context("historical receipt Client exit status")?
            .success();
        let error = fs::read_to_string(stderr)?;
        let bytes = fs::read(stdout)?;
        if label == "unknown-lookup" {
            ensure!(
                !ok && bytes.is_empty() && error.contains("finalized receipt height unavailable"),
                "unindexed hash failed to produce the authenticated Unavailable result: {error}"
            );
            continue;
        }
        if label == "second-wrong-height" {
            ensure!(
                !ok && bytes.is_empty()
                    && error
                        .contains("expected transaction missing from the committed receipt list"),
                "wrong-height transaction did not fail membership verification: {error}"
            );
            continue;
        }
        ensure!(ok, "{label} failed: {error}");
        let report: Value = serde_json::from_slice(&bytes)?;
        if label == "first-default" {
            verify_original_discovered_receipt(&report, hash, &provider)?;
        } else {
            ensure!(
                report["authenticated_discovery_sources"] == serde_json::json!([provider])
                    && report["serving_transport_sha256"] == provider
                    && report["success"] == true
                    && report["independently_verified_finality"] == true
                    && report["verified_parent_height"] == 1
                    && report["verified_parent_state_root"] == FIRST_ROOT
                    && report["tx_hash"] == SECOND_HASH
                    && report["height"] == 2
                    && report["block_hash"] == SECOND_BLOCK
                    && report["state_root"] == SECOND_ROOT
                    && report["proof_bytes"].as_u64().is_some_and(|size| size > 0),
                "{label} differs from the original UCA header, parent or receipt: {report}"
            );
        }
    }
    Ok(())
}
