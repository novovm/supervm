use super::*;

fn material(manifest: &UcaGenesisManifestV2, version: u32) -> Result<UcaInitializationConfigV2> {
    Ok(UcaInitializationConfigV2 {
        version,
        identities: manifest.identities.clone(),
        accounts: manifest.accounts.clone(),
        approvals: manifest
            .required_approval_subjects()
            .into_iter()
            .map(|(subject, _)| UcaGenesisApprovalV2 {
                subject,
                // Shape-only fixture. Real verification below uses fresh AOEM signatures.
                signature: vec![0; MLDSA65_SIGNATURE_BYTES],
            })
            .collect(),
        pages: vec![],
        expected_signing_domain: manifest.signing_domain_for_version(version)?,
        expected_initial_root: [7; 32],
        expected_final_commitment: [8; 32],
    })
}

#[test]
fn version_two_keeps_exact_legacy_manifest_and_subject_approval_bytes() -> Result<()> {
    let manifest = tests::manifest();
    let legacy = manifest.canonical_bytes()?;
    assert_eq!(manifest.canonical_bytes_for_version(2)?, legacy);
    let domain: NodeHash = Sha256::digest(&legacy).into();
    let plan = manifest.prepare_approvals_for_version(2)?;
    assert_eq!(plan.domain(), domain);
    assert_eq!(
        plan.signing_context(),
        b"NOVOVM/identity-and-asset/genesis-init/v3"
    );
    for (subject, _) in manifest.required_approval_subjects() {
        let encoded = postcard::to_allocvec(&subject)?;
        let mut expected = b"NOVOVM/identity-and-asset/approval/v3\0".to_vec();
        expected.extend_from_slice(&domain);
        expected.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
        expected.extend_from_slice(&encoded);
        assert_eq!(plan.signing_bytes(&subject)?, expected);
        assert_eq!(manifest.approval_signing_bytes(&subject)?, expected);
        assert_eq!(
            manifest.prepare_approvals()?.signing_bytes(&subject)?,
            expected
        );
    }
    let mut json = serde_json::to_value(material(&manifest, 2)?)?;
    json.as_object_mut().unwrap().remove("version");
    let implicit: UcaInitializationConfigV2 = serde_json::from_value(json)?;
    assert_eq!(implicit.version, 2);
    assert!(!implicit.application_authority_enabled());
    let mut genesis = manifest.genesis.clone();
    genesis.genesis_config_commitment = domain;
    implicit.validate_manifest(&genesis)?;
    Ok(())
}

#[test]
fn version_two_approval_preimage_matches_frozen_identity_bytes() -> Result<()> {
    let subject = UcaGenesisApprovalSubjectV2::Identity("uca:owner".into());
    let plan = UcaGenesisApprovalPlanV3 {
        version: 2,
        domain: [0x5a; 32],
        subjects: [(subject.clone(), vec![7; MLDSA65_PUBLIC_KEY_BYTES])].into(),
    };
    let mut expected = b"NOVOVM/identity-and-asset/approval/v3\0".to_vec();
    expected.extend_from_slice(&[0x5a; 32]);
    expected.extend_from_slice(b"\0\0\0\x0b\0\x09uca:owner");
    assert_eq!(plan.signing_bytes(&subject)?, expected);
    Ok(())
}

#[test]
fn version_three_explicitly_binds_application_profile_and_rejects_relabelled_pins() -> Result<()> {
    let manifest = tests::manifest();
    let legacy = manifest.canonical_bytes()?;
    let mut expected = b"NOVOVM/identity-and-asset/initial-manifest/v4\0".to_vec();
    expected.extend_from_slice(&3u32.to_be_bytes());
    expected.extend_from_slice(&4u32.to_be_bytes());
    expected.extend_from_slice(&application_program_id_v1());
    expected.extend_from_slice(&application_receipt_codec_v1());
    expected.extend_from_slice(&application_effect_contract_v1(&manifest.genesis.policy)?);
    expected.extend_from_slice(&(legacy.len() as u32).to_be_bytes());
    expected.extend_from_slice(&legacy);
    assert_eq!(manifest.canonical_bytes_for_version(3)?, expected);
    let plan = manifest.prepare_approvals_for_version(3)?;
    assert_eq!(plan.domain(), <NodeHash>::from(Sha256::digest(expected)));
    assert_eq!(plan.signing_context(), INITIALIZATION_CONTEXT_V3);
    assert_ne!(plan.domain(), manifest.signing_domain()?);
    for (subject, _) in plan.subjects() {
        assert!(plan
            .signing_bytes(subject)?
            .starts_with(b"NOVOVM/identity-and-asset/approval/v4\0"));
        assert_ne!(
            plan.signing_bytes(subject)?,
            manifest.approval_signing_bytes(subject)?
        );
    }
    let explicit = material(&manifest, 3)?;
    assert!(explicit.application_authority_enabled());
    let decoded: UcaInitializationConfigV2 =
        serde_json::from_slice(&serde_json::to_vec(&explicit)?)?;
    assert_eq!(decoded.version, 3);
    let mut genesis = manifest.genesis.clone();
    genesis.genesis_config_commitment = explicit.expected_signing_domain;
    explicit.validate_manifest(&genesis)?;

    let mut relabelled = material(&manifest, 2)?;
    genesis.genesis_config_commitment = relabelled.expected_signing_domain;
    relabelled.version = 3;
    assert!(relabelled.application_authority_enabled());
    assert!(relabelled.validate_manifest(&genesis).is_err());
    for version in [0, 1, 4, u32::MAX] {
        relabelled.version = version;
        assert!(!relabelled.application_authority_enabled());
        assert!(relabelled.validate_manifest(&genesis).is_err());
        assert!(manifest.canonical_bytes_for_version(version).is_err());
        assert!(manifest.prepare_approvals_for_version(version).is_err());
    }
    let mut changed = manifest.clone();
    changed.genesis.policy.quote_ttl_ms += 1;
    assert_ne!(changed.signing_domain_for_version(3)?, plan.domain());
    Ok(())
}

#[test]
fn paging_preserves_initialization_version_and_pins_not_container_version() -> Result<()> {
    let manifest = tests::manifest();
    let directory = std::env::temp_dir().join(format!(
        "novovm-application-init-pages-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    for version in [2, 3] {
        let original = material(&manifest, version)?;
        let mut genesis = manifest.genesis.clone();
        genesis.genesis_config_commitment = original.expected_signing_domain;
        let base = directory.join(version.to_string());
        let mut header = original.write_pages_with_genesis(&mut genesis, &base, 2)?;
        assert_eq!(header.version, version);
        assert_eq!(
            header.expected_signing_domain,
            original.expected_signing_domain
        );
        assert_eq!(header.expected_initial_root, original.expected_initial_root);
        assert_eq!(
            header.expected_final_commitment,
            original.expected_final_commitment
        );
        let mut implicit_json = serde_json::to_value(&header)?;
        implicit_json.as_object_mut().unwrap().remove("version");
        let implicit: UcaInitializationConfigV2 = serde_json::from_value(implicit_json)?;
        assert_eq!(implicit.version, 2);
        header.hydrate_pages_with_genesis(&base, &mut genesis)?;
        header.validate_manifest(&genesis)?;
        assert_eq!(header.version, version);
        assert_eq!(header.application_authority_enabled(), version == 3);
        header.version = if version == 2 { 3 } else { 2 };
        assert!(header.validate_manifest(&genesis).is_err());
    }
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
#[ignore = "requires trusted NOVOVM_AOEM_TEST_LIBRARY; fresh isolated ML-DSA approvals, no deployment"]
fn actual_application_initialization_requires_fresh_approvals_and_all_three_pins() -> Result<()> {
    let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted AOEM library required")?;
    let runtime = AoemExecFacade::open(
        std::path::Path::new(&library),
        novovm_exec::AoemExecOpenOptions::default(),
    )?;
    let keygen = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let signer = runtime.mldsa_signer_v1(AoemMldsaParameterSet::MlDsa65)?;
    let mut manifest = tests::manifest();
    manifest.identities.truncate(1);
    manifest.accounts.truncate(1);
    manifest.accounts[0].grants.clear();
    let identity = keygen.generate_keypair()?;
    let management = keygen.generate_keypair()?;
    let recovery = keygen.generate_keypair()?;
    manifest.identities[0].identity_public_key = identity.public_key.clone();
    manifest.accounts[0].management_public_key = management.public_key.clone();
    manifest.accounts[0].recovery_public_key = Some(recovery.public_key.clone());
    let keys = [identity, management, recovery];
    let approve = |version| -> Result<Vec<UcaGenesisApprovalV2>> {
        let plan = manifest.prepare_approvals_for_version(version)?;
        plan.subjects()
            .map(|(subject, public)| {
                let key = keys
                    .iter()
                    .find(|key| key.public_key == public)
                    .context("missing isolated subject key")?;
                Ok(UcaGenesisApprovalV2 {
                    subject: subject.clone(),
                    signature: signer.sign(
                        public,
                        &key.expanded_secret_key,
                        &plan.signing_bytes(subject)?,
                        plan.signing_context(),
                    )?,
                })
            })
            .collect()
    };
    let old_approvals = approve(2)?;
    let old_domain = manifest.signing_domain()?;
    let old = manifest.compile(&runtime, old_domain, &old_approvals)?;
    let explicit_old = manifest.compile_for_version(2, &runtime, old_domain, &old_approvals)?;
    assert_eq!(old.state().root(), explicit_old.state().root());
    assert_eq!(old.final_commitment(), explicit_old.final_commitment());
    let mut legacy_final = Sha256::new();
    legacy_final.update(b"NOVOVM/identity-and-asset/initial-final-commitment/v3\0");
    legacy_final.update(old_domain);
    legacy_final.update(old.state().root());
    assert_eq!(
        old.final_commitment(),
        <NodeHash>::from(legacy_final.finalize())
    );

    let new_domain = manifest.signing_domain_for_version(3)?;
    assert!(manifest
        .compile_for_version(3, &runtime, old_domain, &old_approvals)
        .is_err());
    assert!(manifest
        .compile_for_version(3, &runtime, new_domain, &old_approvals)
        .is_err());
    let new_approvals = approve(3)?;
    let new = manifest.compile_for_version(3, &runtime, new_domain, &new_approvals)?;
    assert_ne!(old.state().root(), new.state().root());
    assert_ne!(old.final_commitment(), new.final_commitment());
    assert!(manifest
        .compile(&runtime, old_domain, &new_approvals)
        .is_err());
    let mut explicit = material(&manifest, 3)?;
    explicit.approvals = new_approvals;
    explicit.expected_initial_root = new.state().root();
    explicit.expected_final_commitment = new.final_commitment();
    let mut genesis = manifest.genesis.clone();
    genesis.genesis_config_commitment = new_domain;
    explicit.verify(&genesis, &runtime)?;
    // A new-domain approval cannot accept legacy roots/finals, nor can a page
    // container's format version authorize a change of initialization profile.
    let mut mixed = explicit.clone();
    mixed.expected_initial_root = old.state().root();
    assert!(mixed.verify(&genesis, &runtime).is_err());
    mixed = explicit.clone();
    mixed.expected_final_commitment = old.final_commitment();
    assert!(mixed.verify(&genesis, &runtime).is_err());
    let directory = std::env::temp_dir().join(format!(
        "novovm-approved-application-init-pages-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let mut paged_genesis = genesis.clone();
    let mut paged = explicit.write_pages_with_genesis(&mut paged_genesis, &directory, 2)?;
    assert_eq!(paged.version, 3);
    paged.hydrate_pages_with_genesis(&directory, &mut paged_genesis)?;
    paged.verify(&paged_genesis, &runtime)?;
    paged.approvals = old_approvals;
    assert!(paged.verify(&paged_genesis, &runtime).is_err());
    std::fs::remove_dir_all(&directory)?;
    for pin in 0..3 {
        let mut altered = explicit.clone();
        match pin {
            0 => altered.expected_signing_domain[0] ^= 1,
            1 => altered.expected_initial_root[0] ^= 1,
            _ => altered.expected_final_commitment[0] ^= 1,
        }
        assert!(altered.verify(&genesis, &runtime).is_err(), "pin {pin}");
    }
    let mut missing = explicit.clone();
    missing.approvals.pop();
    assert!(missing.verify(&genesis, &runtime).is_err());
    let mut forged = explicit.clone();
    forged.approvals[0].signature[0] ^= 1;
    assert!(forged.verify(&genesis, &runtime).is_err());
    Ok(())
}
