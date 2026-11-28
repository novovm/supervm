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
    if let Some(headers) = std::env::var_os("NOVOVM_QUERY_COPY_PORTABLE_HEADERS") {
        return verify_portable(
            owner,
            binary,
            profile,
            &pin,
            &installed,
            first_hash,
            Path::new(&headers),
        );
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

fn verify_portable(
    owner: &mut QueryOwner,
    binary: &Path,
    profile: &Path,
    pin: &str,
    installed: &Value,
    first_hash: &str,
    headers: &Path,
) -> Result<()> {
    let stage = profile.parent().context("portable receipt stage")?;
    let online = stage.join("portable-export-device");
    let offline = stage.join("portable-verifier-device");
    let query = |hash: &str, height: &str, file: &Path| {
        let mut command = Command::new(binary);
        command
            .arg(profile)
            .arg("discover-verified-receipt")
            .arg(hash)
            .arg(pin)
            .args(["--height", height, "--export-proof"])
            .arg(file)
            .env("LOCALAPPDATA", &online)
            .env("XDG_STATE_HOME", &online);
        command
    };
    let files = [
        stage.join("original-h1.nvreceipt"),
        stage.join("original-h2.nvreceipt"),
    ];
    let cases = [(first_hash, "1", &files[0]), (SECOND_HASH, "2", &files[1])];
    let mut online_reports = Vec::new();
    for (hash, height, file) in cases {
        let output = run_portable_query(
            owner,
            query(hash, height, file),
            stage,
            &format!("export-{height}"),
        )?;
        ensure!(
            output.status.success(),
            "proof export failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let sha = format!("{:x}", Sha256::digest(fs::read(file)?));
        ensure!(
            report["proof_sha256"] == sha,
            "export digest differs from verified proof"
        );
        online_reports.push(report);
    }
    let held = fs::read(&files[0])?;
    let duplicate = run_portable_query(
        owner,
        query(first_hash, "1", &files[0]),
        stage,
        "export-duplicate",
    )?;
    ensure!(
        !duplicate.status.success() && duplicate.stdout.is_empty() && fs::read(&files[0])? == held,
        "existing receipt proof was overwritten or accepted"
    );

    let run_offline = |label: &str,
                       cfg: &Path,
                       digest: &str,
                       hash: &str,
                       height: &str,
                       file: &Path|
     -> Result<std::process::Output> {
        let result = Command::new(binary)
            .arg(cfg)
            .arg("verify-receipt-proof")
            .arg(digest)
            .arg(hash)
            .arg(height)
            .arg(file)
            .env("LOCALAPPDATA", &offline)
            .env("XDG_STATE_HOME", &offline)
            .output()?;
        fs::write(stage.join(format!("{label}.json")), &result.stdout)?;
        fs::write(stage.join(format!("{label}.stderr")), &result.stderr)?;
        Ok(result)
    };
    let verified = |result: std::process::Output, expected: &Value| -> Result<()> {
        ensure!(
            result.status.success(),
            "offline receipt failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let actual: Value = serde_json::from_slice(&result.stdout)?;
        for field in [
            "proof_sha256",
            "independently_verified_finality",
            "height",
            "state_root",
            "block_hash",
            "tx_hash",
            "success",
            "verified_parent_state_root",
        ] {
            ensure!(
                actual[field] == expected[field],
                "offline receipt {field} differs"
            );
        }
        ensure!(
            actual.get("transport_identity_sha256").is_none(),
            "offline receipt touched transport custody"
        );
        Ok(())
    };
    let rejected = |result: std::process::Output, reason: &str| -> Result<()> {
        let error = String::from_utf8_lossy(&result.stderr);
        ensure!(
            !result.status.success() && result.stdout.is_empty() && error.contains(reason),
            "offline proof unexpectedly accepted or failed at another step: {error}"
        );
        Ok(())
    };
    verified(
        run_offline("offline-h1", profile, pin, first_hash, "1", &files[0])?,
        &online_reports[0],
    )?;
    rejected(
        run_offline(
            "offline-no-parent",
            profile,
            pin,
            SECOND_HASH,
            "2",
            &files[1],
        )?,
        "certified local headers through its parent",
    )?;
    rejected(
        run_offline(
            "offline-wrong-hash",
            profile,
            pin,
            SECOND_HASH,
            "1",
            &files[0],
        )?,
        "expected transaction missing",
    )?;
    let altered = stage.join("altered.nvreceipt");
    let mut modified = held;
    *modified.last_mut().context("empty original proof")? ^= 1;
    fs::write(&altered, modified)?;
    rejected(
        run_offline("offline-altered", profile, pin, first_hash, "1", &altered)?,
        "",
    )?;
    ensure!(
        !offline.exists(),
        "offline verification created device state"
    );

    let mut minimum_profile = installed.clone();
    let minimum_block = crate::native_pipeline::service::config::decode_hex(SECOND_BLOCK)?;
    minimum_profile["verification"]["minimum_finalized_checkpoint"] = serde_json::json!({
        "height":2, "block_hash":minimum_block,
        "valid_until_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64 + 120_000,
    });
    let strict = stage.join("offline-with-minimum.json");
    let strict_bytes = serde_json::to_vec(&minimum_profile)?;
    fs::write(&strict, &strict_bytes)?;
    let strict_pin = format!("{:x}", Sha256::digest(&strict_bytes));
    rejected(
        run_offline(
            "offline-no-minimum",
            &strict,
            &strict_pin,
            first_hash,
            "1",
            &files[0],
        )?,
        "certified local headers through the installed checkpoint",
    )?;
    let product = headers.ancestors().nth(3).context("portable header path")?;
    ensure!(
        product.file_name().is_some_and(|name| name == "NOVOVM"),
        "portable header path must be within the Client cache layout"
    );
    let cache =
        offline.join(headers.strip_prefix(product.parent().context("portable cache root")?)?);
    fs::create_dir_all(&cache)?;
    for height in [1, 2] {
        let name = format!("{height:020}.proof");
        fs::copy(headers.join(&name), cache.join(name))?;
    }
    verified(
        run_offline("offline-h2", profile, pin, SECOND_HASH, "2", &files[1])?,
        &online_reports[1],
    )?;
    verified(
        run_offline(
            "offline-h1-with-minimum",
            &strict,
            &strict_pin,
            first_hash,
            "1",
            &files[0],
        )?,
        &online_reports[0],
    )?;
    ensure!(
        fs::read_dir(&offline)?.count() == 1
            && fs::read_dir(cache.parent().context("portable domain root")?)?.count() == 1
            && fs::read_dir(&cache)?.count() == 2,
        "offline verification created local custody or modified its certified cache"
    );
    println!("portable H1/H2 proofs exported, independently verified without transport custody, negatives and overwrite guard passed");
    Ok(())
}

fn run_portable_query(
    owner: &mut QueryOwner,
    mut command: Command,
    stage: &Path,
    label: &str,
) -> Result<std::process::Output> {
    let stdout = stage.join(format!("{label}.json"));
    let stderr = stage.join(format!("{label}.stderr"));
    let mut child = IsolatedFollowChild(
        command
            .stdout(Stdio::from(fs::File::create_new(&stdout)?))
            .stderr(Stdio::from(fs::File::create_new(&stderr)?))
            .spawn()?,
    );
    let mut status = None;
    owner.until(|_| {
        status = child.0.try_wait()?;
        Ok(status.is_some())
    })?;
    Ok(std::process::Output {
        status: status.context("portable export Client exit status")?,
        stdout: fs::read(stdout)?,
        stderr: fs::read(stderr)?,
    })
}
