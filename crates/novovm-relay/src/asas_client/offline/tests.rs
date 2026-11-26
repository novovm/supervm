use super::*;
use crate::purpose_keys::PurposeKeyMaterialV3;
use novovm_protocol::{
    native_query::UcaIdentityIdV1,
    uca_application::{ApplicationGrantActionV1, SignedApplicationGrantV1},
    uca_registration::SignedUcaRegistrationV1,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};

#[test]
fn reviewed_json_rejects_unknown_fields_and_implicit_height() {
    let mut value = json!({"version":1,"execution_height":10,"intent":{
        "chain_id":7,"genesis":vec![1;32],"identity":"uca:test","primary_key_ref":vec![2;32],
        "valid_from_height":10,"valid_until_height":12}});
    assert!(review::<UcaRegistrationIntentV1>(&serde_json::to_vec(&value).unwrap()).is_ok());
    value["intent"]["unexpected"] = true.into();
    assert!(review::<UcaRegistrationIntentV1>(&serde_json::to_vec(&value).unwrap()).is_err());
    value["intent"]
        .as_object_mut()
        .unwrap()
        .remove("unexpected");
    value["execution_height"] = 0.into();
    assert!(review::<UcaRegistrationIntentV1>(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn signing_output_requires_an_explicit_nontraversing_path() {
    assert!(explicit_path(Path::new("relative.nuir")).is_err());
    assert!(explicit_path(&std::env::temp_dir().join("one/../two.nuir")).is_err());
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn request(root: &Path, value: &Value) -> Result<(PathBuf, String)> {
    let path = root.join("review.json");
    fs::write(&path, serde_json::to_vec(value)?)?;
    let hash = digest(&path)?;
    Ok((path, hash))
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "explicit qualified AOEM and fresh CLI required; creates/signs test keys only, no network or chain"]
fn protected_test_keys_sign_reviewed_nuir_and_nuap_through_cli() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    ensure!(
        std::env::var_os("CREDENTIALS_DIRECTORY").is_none(),
        "do not use a real custody credential in this test"
    );
    let credentials = crate::linux_custody::prepare_test_credential()?;
    let library = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY").context("qualified test library required")?,
    );
    let executable = PathBuf::from(
        std::env::var_os("NOVOVM_ASAS_TEST_CLIENT").context("freshly built test CLI required")?,
    );
    let contract: Value = serde_json::from_slice(
        &Command::new(&executable)
            .arg("--client-contract")
            .output()?
            .stdout,
    )?;
    ensure!(
        contract["offlineUcaSigning"] == 1,
        "current offline signing CLI required"
    );
    let library_sha256 = digest(&library)?;
    let (owner, runtime) =
        crate::transport_identity::load_qualified_runtime(&library, &library_sha256)?;
    let root = std::env::temp_dir().join(format!(
        "asas-offline-signing-{:016x}",
        rand::random::<u64>()
    ));
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let _cleanup = Fixture(root.clone());
    let identity_key = root.join("identity.sealed");
    let transport_key = root.join("transport.sealed");
    let create = |path: &Path, purpose: PurposeV1| {
        purpose_keys::generate_mldsa(
            path,
            purpose,
            7,
            || {
                let key = runtime.generate_keypair()?;
                PurposeKeyMaterialV3::new(key.public_key, key.expanded_secret_key)
            },
            |public, secret, challenge| {
                runtime.sign(public, secret, challenge, purpose.pair_context())?;
                Ok(())
            },
        )
    };
    let root_public = create(&identity_key, PurposeV1::Identity)?.public_key;
    let device_public = create(&transport_key, PurposeV1::Transport)?.public_key;
    let reference: [u8; 32] =
        derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &root_public)
            .try_into()
            .unwrap();
    let name = format!(
        "uca:{}",
        reference
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let config = ClientConfigV1 {
        version: 1,
        identity_path: transport_key.clone(),
        credentials_directory: None,
        chain_id: 7,
        library,
        library_sha256,
        server_key: runtime.generate_keypair()?.public_key,
        server_address: "127.0.0.1:9".parse()?,
        bind_address: "127.0.0.1:0".parse()?,
        uca: Some(super::super::ClientUcaV1 {
            identity: UcaIdentityIdV1::new(&name).map_err(anyhow::Error::msg)?,
            genesis: [9; 32],
        }),
    };
    assert_ne!(device_public, root_public);
    let selector = config.uca_selector()?;
    let registration = json!({"version":1,"execution_height":10,"intent":UcaRegistrationIntentV1 {
        chain_id:7,genesis:[9;32],identity:config.uca.as_ref().unwrap().identity.clone(),
        primary_key_ref:reference,valid_from_height:10,valid_until_height:12}});
    let application = json!({"version":1,"execution_height":10,"intent":ApplicationGrantIntentV1 {
        selector:selector.clone(),issuer_key_ref:reference,expected_revision:0,
        valid_from_height:10,valid_until_height:12,
        action:ApplicationGrantActionV1::Authorize {not_before_height:10,expires_at_height:100}}});
    let before = [digest(&identity_key)?, digest(&transport_key)?];
    for (kind, value, output_name) in [
        (
            SigningKind::Registration,
            &registration,
            "registration.nuir",
        ),
        (SigningKind::Application, &application, "application.nuap"),
    ] {
        let (input, hash) = request(&root, value)?;
        let output = root.join(output_name);
        let receipt = sign_to_file(&config, kind, &input, &hash, &identity_key, &output)?;
        assert!(!receipt.submitted);
        assert_eq!(receipt.sha256, digest(&output)?);
        assert_eq!(fs::metadata(&output)?.permissions().mode() & 0o077, 0);
        let bytes = fs::read(&output)?;
        match kind {
            SigningKind::Registration => {
                owner.verify_uca_registration_v1(
                    SignedUcaRegistrationV1::decode(&bytes)?,
                    7,
                    [9; 32],
                    10,
                )?;
            }
            SigningKind::Application => {
                owner
                    .application_grant_verifier_v1()?
                    .verify(SignedApplicationGrantV1::decode(&bytes)?)?;
            }
        }
        assert!(sign_to_file(&config, kind, &input, &hash, &identity_key, &output).is_err());
        assert_eq!(fs::read(&output)?, bytes);
    }
    let reject = |kind, value: &Value| -> Result<()> {
        let (input, hash) = request(&root, value)?;
        let output = root.join("must-not-exist");
        assert!(sign_to_file(&config, kind, &input, &hash, &identity_key, &output).is_err());
        assert!(!output.exists());
        Ok(())
    };
    for field in 0..6 {
        let mut bad = registration.clone();
        match field {
            0 => bad["intent"]["chain_id"] = 8.into(),
            1 => bad["intent"]["genesis"][0] = 8.into(),
            2 => bad["intent"]["identity"] = "uca:wrong".into(),
            3 => bad["intent"]["primary_key_ref"][0] = (reference[0] ^ 1).into(),
            4 => bad["execution_height"] = 12.into(),
            _ => bad["intent"]["valid_until_height"] = 300.into(),
        }
        reject(SigningKind::Registration, &bad)?;
    }
    for field in [
        "chain_id",
        "genesis",
        "identity",
        "application",
        "service",
        "device",
    ] {
        let mut bad = application.clone();
        match field {
            "chain_id" => bad["intent"]["selector"][field] = 8.into(),
            "identity" => bad["intent"]["selector"][field] = "uca:wrong".into(),
            _ => {
                let byte = bad["intent"]["selector"][field][0].as_u64().unwrap();
                bad["intent"]["selector"][field][0] = (byte ^ 1).into();
            }
        }
        reject(SigningKind::Application, &bad)?;
    }
    for field in 0..4 {
        let mut bad = application.clone();
        match field {
            0 => bad["intent"]["issuer_key_ref"][0] = (reference[0] ^ 1).into(),
            1 => bad["intent"]["expected_revision"] = u64::MAX.into(),
            2 => bad["execution_height"] = 12.into(),
            _ => bad["intent"]["action"]["Authorize"]["unexpected"] = true.into(),
        }
        reject(SigningKind::Application, &bad)?;
    }
    let (input, hash) = request(&root, &registration)?;
    assert!(sign_to_file(
        &config,
        SigningKind::Registration,
        &input,
        &"0".repeat(64),
        &identity_key,
        &root.join("bad-digest")
    )
    .is_err());
    assert!(sign_to_file(
        &config,
        SigningKind::Registration,
        &input,
        &hash,
        &transport_key,
        &root.join("wrong-purpose")
    )
    .is_err());
    let mut bad_runtime = config.clone();
    bad_runtime.library_sha256 = "0".repeat(64);
    assert!(sign_to_file(
        &bad_runtime,
        SigningKind::Registration,
        &input,
        &hash,
        &identity_key,
        &root.join("wrong-library")
    )
    .is_err());
    let mut revoke = application.clone();
    revoke["intent"]["action"] = "Revoke".into();
    revoke["intent"]["expected_revision"] = 1.into();
    let (input, hash) = request(&root, &revoke)?;
    let output = root.join("revoke.nuap");
    sign_to_file(
        &config,
        SigningKind::Application,
        &input,
        &hash,
        &identity_key,
        &output,
    )?;
    owner
        .application_grant_verifier_v1()?
        .verify(SignedApplicationGrantV1::decode(&fs::read(output)?)?)?;

    // Exercise the real argument parser and pinned per-process custody, not real F keys.
    let mut child_config = config.clone();
    child_config.credentials_directory = Some(credentials);
    let profile = root.join("test-profile.json");
    fs::write(&profile, serde_json::to_vec(&child_config)?)?;
    for (flag, value, name) in [
        ("--sign-registration", &registration, "cli.nuir"),
        ("--sign-application", &application, "cli.nuap"),
    ] {
        let (input, hash) = request(&root, value)?;
        let output = root.join(name);
        let result = Command::new(&executable)
            .env_clear()
            .args([
                "--profile".as_ref(),
                profile.as_os_str(),
                "--profile-sha256".as_ref(),
                digest(&profile)?.as_ref(),
                flag.as_ref(),
                input.as_os_str(),
                "--request-sha256".as_ref(),
                hash.as_ref(),
                "--identity-key".as_ref(),
                identity_key.as_os_str(),
                "--output".as_ref(),
                output.as_os_str(),
            ])
            .output()?;
        ensure!(result.status.success(), "test CLI signing failed");
        let receipt: Value = serde_json::from_slice(&result.stdout)?;
        assert_eq!(receipt["submitted"], false);
        assert!(receipt.get("public_key").is_none() && receipt.get("signature").is_none());
        let bytes = fs::read(&output)?;
        assert_eq!(receipt["sha256"], digest(&output)?);
        if flag == "--sign-registration" {
            owner.verify_uca_registration_v1(
                SignedUcaRegistrationV1::decode(&bytes)?,
                7,
                [9; 32],
                10,
            )?;
        } else {
            owner
                .application_grant_verifier_v1()?
                .verify(SignedApplicationGrantV1::decode(&bytes)?)?;
        }
        let refused = Command::new(&executable)
            .env_clear()
            .args([
                "--profile".as_ref(),
                profile.as_os_str(),
                "--profile-sha256".as_ref(),
                "0".repeat(64).as_ref(),
                flag.as_ref(),
                input.as_os_str(),
                "--request-sha256".as_ref(),
                hash.as_ref(),
                "--identity-key".as_ref(),
                identity_key.as_os_str(),
                "--output".as_ref(),
                root.join("bad-profile").as_os_str(),
            ])
            .output()?;
        assert!(!refused.status.success());
        assert!(refused.stdout.is_empty());
    }
    assert_eq!([digest(&identity_key)?, digest(&transport_key)?], before);
    Ok(())
}
