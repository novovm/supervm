//! Offline operator CLI regressions. All allocations, domains and keys below
//! are isolated test material, never an approved deployment configuration.
//! Fixture files and command output are retained, including failed commands.
use ed25519_dalek::SigningKey;
use novovm_node::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use novovm_node::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS;
use novovm_node::native_pipeline::service::config::uca_genesis::{
    UcaGenesisAccountV2, UcaGenesisApprovalV2, UcaGenesisGrantV2, UcaGenesisIdentityV2,
    UcaGenesisManifestV2, UcaInitializationConfigV2, INITIALIZATION_CONTEXT_V2,
    MAX_INITIAL_GRANTS_PER_ACCOUNT, MAX_INITIAL_UCA_ACCOUNTS,
};
use novovm_node::native_pipeline::service::config::{
    protocol_commitment, GenesisAllocation, GenesisConfig, GenesisValidator, NativePeerConfig,
    NodeKeyStorage, ResidentConfig, MAX_STARTUP_DOCUMENT_BYTES,
};
use novovm_protocol::uca_spend::{spend_key_ref_v1, MLDSA65_PUBLIC_KEY_BYTES};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn Error>>;
type FileSnapshot = BTreeMap<PathBuf, Option<[u8; 32]>>;
const LARGE_EXACT_AMOUNT: u128 = (1u128 << 100) + 123;
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    manifest: PathBuf,
    approvals: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Result<Self, Box<dyn Error>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/runtime-rebuild/native-genesis-cli-tests")
            .join(format!(
                "{name}-{}-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        Ok(Self {
            manifest: root.join("public-test-manifest.json"),
            approvals: root.join("public-test-approvals.json"),
            root,
        })
    }

    fn write_manifest(&self, manifest: &UcaGenesisManifestV2) -> TestResult {
        fs::write(&self.manifest, serde_json::to_vec(manifest)?)?;
        Ok(())
    }

    fn write_approvals(&self, approvals: &[UcaGenesisApprovalV2]) -> TestResult {
        fs::write(&self.approvals, serde_json::to_vec(approvals)?)?;
        Ok(())
    }

    fn invoke(&self, label: &str, args: &[String]) -> Result<Output, Box<dyn Error>> {
        let before = snapshot(&self.root)?;
        let result = Command::new(env!("CARGO_BIN_EXE_novovmctl"))
            .arg("native-genesis")
            .args(args)
            .current_dir(&self.root)
            .output()?;
        let after = snapshot(&self.root)?;
        // Persist diagnostics ourselves only AFTER observing the CLI's effects.
        // The operator command may not create a store, key, journal or output.
        fs::write(self.root.join(format!("{label}.stdout")), &result.stdout)?;
        fs::write(self.root.join(format!("{label}.stderr")), &result.stderr)?;
        assert_eq!(before, after, "CLI changed fixture files: {label}");
        Ok(result)
    }

    fn prepare(&self, label: &str) -> Result<Output, Box<dyn Error>> {
        self.invoke(
            label,
            &[
                "prepare".into(),
                "--manifest".into(),
                self.manifest.to_string_lossy().into_owned(),
            ],
        )
    }

    fn prepare_approval(&self, label: &str, index: usize) -> Result<Output, Box<dyn Error>> {
        self.invoke(
            label,
            &[
                "prepare".into(),
                "--manifest".into(),
                self.manifest.to_string_lossy().into_owned(),
                "--approval-index".into(),
                index.to_string(),
            ],
        )
    }

    fn verify(
        &self,
        label: &str,
        domain: [u8; 32],
        library: &Path,
        library_hash: [u8; 32],
    ) -> Result<Output, Box<dyn Error>> {
        self.invoke(
            label,
            &[
                "verify-approvals".into(),
                "--manifest".into(),
                self.manifest.to_string_lossy().into_owned(),
                "--approvals".into(),
                self.approvals.to_string_lossy().into_owned(),
                "--expected-domain".into(),
                hex(&domain),
                "--library".into(),
                library.to_string_lossy().into_owned(),
                "--library-sha256".into(),
                hex(&library_hash),
            ],
        )
    }
}

fn snapshot(root: &Path) -> Result<FileSnapshot, Box<dyn Error>> {
    fn visit(root: &Path, path: &Path, files: &mut FileSnapshot) -> TestResult {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root)?.to_path_buf();
            if entry.file_type()?.is_dir() {
                files.insert(relative, None);
                visit(root, &path, files)?;
            } else {
                files.insert(relative, Some(Sha256::digest(fs::read(path)?).into()));
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn manifest() -> Result<UcaGenesisManifestV2, Box<dyn Error>> {
    let policy = DirectNovFeePolicy {
        quote_ttl_ms: 60_000,
        policy_version: 1,
        policy_source: "isolated-cli-test-not-deployment".into(),
        resolution_source: "isolated-cli-test".into(),
        reserve_share_bps: 5000,
        fee_share_bps: 3000,
        risk_buffer_share_bps: 2000,
        min_reserve_bucket_nov: 1,
        min_fee_bucket_nov: 1,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: LARGE_EXACT_AMOUNT + 7,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    };
    let mut accounts = Vec::new();
    for index in 0..2u8 {
        let id = format!("uca:isolated-cli-test-{index}");
        let spend_key = vec![index + 11; MLDSA65_PUBLIC_KEY_BYTES];
        // Typed original grant parsing avoids a new policy schema and avoids
        // routing its full-width numbers through a generic JSON Value/f64.
        let grant = serde_json::from_str(&format!(
            r#"{{"account":{account},"asset":1,"key_ref":{key},"authorization_epoch":1,
            "permits_nov_transfer":true,"revoked":false,"not_before_height":1,
            "expires_at_height":100,"max_debit_per_intent":{amount},
            "remaining_debit_budget":{budget}}}"#,
            account = serde_json::to_string(&vec![index + 1; 20])?,
            key = serde_json::to_string(&spend_key_ref_v1(&spend_key)?)?,
            amount = LARGE_EXACT_AMOUNT,
            budget = LARGE_EXACT_AMOUNT + 5,
        ))?;
        accounts.push(UcaGenesisAccountV2 {
            uca_id: id,
            asset_account: vec![index + 1; 20],
            management_public_key: vec![index + 21; MLDSA65_PUBLIC_KEY_BYTES],
            recovery_public_key: Some(vec![index + 31; MLDSA65_PUBLIC_KEY_BYTES]),
            grants: vec![UcaGenesisGrantV2 {
                spend_public_key: spend_key,
                grant,
            }],
        });
    }
    Ok(UcaGenesisManifestV2 {
        genesis: GenesisConfig {
            chain_id: 0x00c0_de77,
            validator_epoch: 1,
            protocol_commitment: protocol_commitment(),
            genesis_config_commitment: [0; 32],
            timestamp_unix_ms: 123,
            validators: (1..=4)
                .map(|seed| GenesisValidator {
                    // Structural prepare test only; no vote/key possession is
                    // claimed. The actual runtime test replaces all four keys.
                    public_key: vec![seed; MLDSA65_PUBLIC_KEY_BYTES],
                    transport_public_key: SigningKey::from_bytes(&[100 + seed; 32])
                        .verifying_key()
                        .to_bytes(),
                    weight: 1,
                })
                .collect(),
            allocations: vec![
                GenesisAllocation {
                    account_hex: "01".repeat(20),
                    amount: (NOV_FIXED_SUPPLY_BASE_UNITS - 2000).to_string(),
                },
                GenesisAllocation {
                    account_hex: "02".repeat(20),
                    amount: "2000".into(),
                },
            ],
            policy,
        },
        identities: (0..2u8)
            .map(|index| UcaGenesisIdentityV2 {
                uca_id: format!("uca:isolated-cli-test-{index}"),
                identity_public_key: vec![index + 41; MLDSA65_PUBLIC_KEY_BYTES],
            })
            .collect(),
        accounts,
    })
}

fn data(output: &Output) -> Result<Value, Box<dyn Error>> {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(envelope["ok"], true);
    Ok(envelope["data"].clone())
}

fn rejected(output: &Output, expected: &str) {
    assert!(!output.status.success(), "unexpected success");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains(expected), "expected {expected:?}, got {text}");
    assert!(
        !text.contains("\"identity_and_asset_approvals_verified\": true"),
        "failed review emitted successful authorization metadata"
    );
}

fn assert_non_activation(value: &Value) {
    for field in [
        "operator_launch_authorized",
        "existing_uca_authority_verified",
        "database_created",
        "activation_ready",
        "state_published",
        "whole_node_post_quantum_qualified",
    ] {
        assert_eq!(value[field], false, "{field}");
    }
}

fn assert_required_approvals(value: &Value, manifest: &UcaGenesisManifestV2) -> TestResult {
    let transcripts = value["required_approvals"]
        .as_array()
        .ok_or("required approvals missing")?;
    let required = manifest.required_approval_subjects();
    assert_eq!(transcripts.len(), required.len());
    for (index, (actual, (subject, public_key))) in transcripts.iter().zip(required).enumerate() {
        assert_eq!(actual["index"], index);
        assert_eq!(actual["subject"], serde_json::to_value(&subject)?);
        assert_eq!(actual["public_key_hex"], hex(&public_key));
        assert!(actual.get("signing_message_hex").is_none());
    }
    assert_eq!(value["signing_task_selected"], false);
    assert!(value["selected_approval"].is_null());
    Ok(())
}

#[test]
fn maximum_shape_pretty_manifest_uses_text_budget_and_preserves_canonical_bound() -> TestResult {
    let fixture = Fixture::new("maximum-pretty-shape")?;
    let mut input = manifest()?;
    // This fixture measures document shape, not full-width money. Its later
    // explicit JSON Value assembly uses only exactly representable integers;
    // the independent canonical/actual-runtime tests retain LARGE_EXACT_AMOUNT.
    input.genesis.policy.clearing_daily_nov_hard_limit = 1_000_000;
    let metadata_key = |purpose: u8, index: usize| {
        let mut bytes = vec![255; MLDSA65_PUBLIC_KEY_BYTES];
        bytes[0] = purpose;
        bytes[1..9].copy_from_slice(&(index as u64).to_be_bytes());
        bytes
    };
    // Public metadata only. No fabricated signature is treated as approval.
    input.genesis.validators = (0..64)
        .map(|index| GenesisValidator {
            public_key: metadata_key(1, index),
            transport_public_key: SigningKey::from_bytes(&[index as u8 + 100; 32])
                .verifying_key()
                .to_bytes(),
            weight: 1,
        })
        .collect();
    let template = input.accounts[0].clone();
    input.accounts = (0..MAX_INITIAL_UCA_ACCOUNTS)
        .map(|index| {
            let mut account = template.clone();
            account.uca_id = format!("uca:maximum-pretty-fixture-{index}");
            account.asset_account = vec![index as u8 + 1; 20];
            account.management_public_key = metadata_key(2, index);
            account.recovery_public_key = Some(metadata_key(4, index));
            account.grants = (0..MAX_INITIAL_GRANTS_PER_ACCOUNT)
                .map(|grant_index| {
                    let mut entry = template.grants[0].clone();
                    entry.spend_public_key =
                        metadata_key(3, index * MAX_INITIAL_GRANTS_PER_ACCOUNT + grant_index);
                    entry.grant.account = account.asset_account.clone();
                    entry.grant.key_ref = spend_key_ref_v1(&entry.spend_public_key).unwrap();
                    entry.grant.max_debit_per_intent = 80;
                    entry.grant.remaining_debit_budget = 1_000;
                    entry
                })
                .collect();
            account
        })
        .collect();
    input.identities = input
        .accounts
        .iter()
        .enumerate()
        .map(|(index, account)| UcaGenesisIdentityV2 {
            uca_id: account.uca_id.clone(),
            identity_public_key: metadata_key(5, index),
        })
        .collect();
    let canonical = input.canonical_bytes()?;
    assert!(
        canonical.len() <= 1024 * 1024,
        "canonical contract remains bounded independently"
    );
    let pretty = serde_json::to_vec_pretty(&input)?;
    assert!(pretty.len() > 1024 * 1024);
    assert!(pretty.len() < MAX_STARTUP_DOCUMENT_BYTES);
    fs::write(&fixture.manifest, &pretty)?;
    let observed = data(&fixture.prepare("maximum-pretty")?)?;
    assert_eq!(observed["canonical_manifest_bytes"], canonical.len());
    assert_eq!(observed["canonical_manifest_hex"], hex(&canonical));
    assert_required_approvals(&observed, &input)?;
    assert_non_activation(&observed);
    let approvals: Vec<_> = input
        .required_approval_subjects()
        .into_iter()
        .map(|(subject, _)| UcaGenesisApprovalV2 {
            subject,
            signature: vec![255; 3309],
        })
        .collect();
    let initialization = UcaInitializationConfigV2 {
        identities: input.identities.clone(),
        accounts: input.accounts.clone(),
        approvals,
        expected_signing_domain: [1; 32],
        expected_initial_root: [2; 32],
        expected_final_commitment: [3; 32],
    };
    let config = ResidentConfig {
        profile: "native_execution_tick".into(),
        library: fixture.root.join("not-loaded-aoem.dll"),
        database: fixture.root.join("must-not-create.rocksdb"),
        rpc_addr: None,
        signing_key_file: fixture.root.join("not-created-validator.sealed"),
        transport_key_file: fixture.root.join("not-created-transport.sealed"),
        key_storage: NodeKeyStorage::ProtectedV1,
        udp_addr: "127.0.0.1:41050".parse()?,
        peers: input
            .genesis
            .validators
            .iter()
            .skip(1)
            .enumerate()
            .map(|(index, validator)| NativePeerConfig {
                public_key: validator.public_key.clone(),
                address: ([127, 0, 0, 1], 41051 + index as u16).into(),
            })
            .collect(),
        native_client_limit: 1,
        genesis: input.genesis.clone(),
        uca_initialization: Some(initialization),
        workers: 2,
        execution_backend: Default::default(),
        signature_backend: Default::default(),
        batch_size: 64,
    };
    // ResidentConfig deliberately exposes only deserialization. Assemble the
    // actual document explicitly, then decode it through that production type;
    // the fixture does not add a configuration export API to production.
    let mut document = serde_json::json!({
        "profile": config.profile, "library": config.library,
        "database": config.database, "rpc_addr": config.rpc_addr,
        "signing_key_file": config.signing_key_file,
        "transport_key_file": config.transport_key_file,
        "key_storage": config.key_storage, "udp_addr": config.udp_addr,
        "peers": config.peers, "native_client_limit": config.native_client_limit,
        "genesis": config.genesis, "uca_initialization": config.uca_initialization,
        "workers": config.workers, "execution_backend": "cpu_callbacks",
        "batch_size": config.batch_size,
    });
    let with_peers_and_initialization = serde_json::to_vec_pretty(&document)?;
    let decoded: ResidentConfig = serde_json::from_slice(&with_peers_and_initialization)?;
    assert_eq!(decoded.genesis.validators.len(), 64);
    assert_eq!(decoded.peers.len(), 63);
    assert_eq!(
        decoded.uca_initialization.unwrap().accounts.len(),
        MAX_INITIAL_UCA_ACCOUNTS
    );
    document["uca_initialization"] = Value::Null;
    let with_peers = serde_json::to_vec_pretty(&document)?;
    assert!(with_peers_and_initialization.len() < MAX_STARTUP_DOCUMENT_BYTES);
    eprintln!("maximum MLDSA metadata: canonical={} manifest_pretty={} resident64_with_peers={} resident64_with_peers_and_32x8_initialization={}", canonical.len(), pretty.len(), with_peers.len(), with_peers_and_initialization.len());
    // A larger text envelope never bypasses the explicit byte bound or creates a store.
    fs::write(
        &fixture.manifest,
        vec![b' '; MAX_STARTUP_DOCUMENT_BYTES + 1],
    )?;
    rejected(&fixture.prepare("oversized-text")?, "at most");
    assert!(!fixture.root.join("must-not-create.rocksdb").exists());
    Ok(())
}

#[test]
fn prepare_matches_original_canonical_domain_without_authorizing_or_writing() -> TestResult {
    let fixture = Fixture::new("prepare")?;
    let input = manifest()?;
    fixture.write_manifest(&input)?;
    let observed = data(&fixture.prepare("prepare")?)?;
    assert_eq!(
        observed["canonical_manifest_hex"],
        hex(&input.canonical_bytes()?)
    );
    assert_eq!(
        observed["observed_signing_domain"],
        hex(&input.signing_domain()?)
    );
    assert_eq!(
        observed["signing_context_hex"],
        hex(INITIALIZATION_CONTEXT_V2)
    );
    assert_eq!(observed["message_is_unframed"], true);
    assert_eq!(observed["signing_algorithm"], "ML-DSA-65");
    assert_eq!(observed["consensus_signature_algorithm"], "ML-DSA-65");
    assert_eq!(observed["transport_signature_algorithm"], "Ed25519");
    assert_eq!(observed["schema_version"], 2);
    assert_eq!(observed["identity_and_asset_approvals_verified"], false);
    assert!(observed.get("signing_message_hex").is_none());
    assert_required_approvals(&observed, &input)?;
    assert_eq!(observed["expected_domain_verified"], false);
    assert!(observed["review_only_startup_fragment"].is_null());
    assert_non_activation(&observed);
    Ok(())
}

#[test]
fn prepare_selects_one_domain_bound_subject_and_rejects_out_of_range() -> TestResult {
    let fixture = Fixture::new("one-approval")?;
    let mut input = manifest()?;
    fixture.write_manifest(&input)?;
    let required = input.required_approval_subjects();
    for (index, (subject, public_key)) in required.iter().enumerate() {
        let observed = data(&fixture.prepare_approval(&format!("subject-{index}"), index)?)?;
        assert_eq!(observed["signing_task_selected"], true);
        let selected = &observed["selected_approval"];
        assert_eq!(selected["index"], index);
        assert_eq!(selected["subject"], serde_json::to_value(subject)?);
        assert_eq!(selected["public_key_hex"], hex(public_key));
        assert_eq!(selected["signing_domain"], hex(&input.signing_domain()?));
        assert_eq!(
            selected["signing_context_hex"],
            hex(INITIALIZATION_CONTEXT_V2)
        );
        let bytes = input.approval_signing_bytes(subject)?;
        assert_eq!(selected["signing_message_hex"], hex(&bytes));
        assert_eq!(selected["signing_message_bytes"], bytes.len());
        assert_ne!(bytes, input.canonical_bytes()?);
        assert_non_activation(&observed);
    }
    rejected(
        &fixture.prepare_approval("out-of-range", required.len())?,
        "approval index is outside",
    );
    let before = data(&fixture.prepare_approval("before-reorder", 0)?)?;
    input.identities.reverse();
    input.accounts.reverse();
    fixture.write_manifest(&input)?;
    let after = data(&fixture.prepare_approval("after-reorder", 0)?)?;
    assert_eq!(before["selected_approval"], after["selected_approval"]);
    input.accounts[0].grants[0].grant.remaining_debit_budget += 1;
    fixture.write_manifest(&input)?;
    let changed = data(&fixture.prepare_approval("different-domain", 0)?)?;
    assert_ne!(
        after["selected_approval"]["signing_domain"],
        changed["selected_approval"]["signing_domain"]
    );
    assert_ne!(
        after["selected_approval"]["signing_message_hex"],
        changed["selected_approval"]["signing_message_hex"]
    );
    Ok(())
}

#[test]
fn reordered_manifest_keeps_original_bytes_and_domain() -> TestResult {
    let fixture = Fixture::new("reordered")?;
    let mut input = manifest()?;
    fixture.write_manifest(&input)?;
    let original = data(&fixture.prepare("ordered")?)?;
    input.accounts.reverse();
    input.identities.reverse();
    input.genesis.validators.reverse();
    input.genesis.allocations.reverse();
    fixture.write_manifest(&input)?;
    let reordered = data(&fixture.prepare("reordered")?)?;
    for field in [
        "canonical_manifest_hex",
        "canonical_manifest_bytes",
        "observed_signing_domain",
        "required_approvals",
    ] {
        assert_eq!(original[field], reordered[field], "{field}");
    }
    Ok(())
}

#[test]
fn prepare_preserves_full_width_values_instead_of_rounding_through_json_value() -> TestResult {
    let fixture = Fixture::new("wide-integers")?;
    let mut input = manifest()?;
    fixture.write_manifest(&input)?;
    let first = data(&fixture.prepare("first")?)?;
    assert_eq!(
        first["canonical_manifest_hex"],
        hex(&input.canonical_bytes()?)
    );
    input.accounts[0].grants[0].grant.remaining_debit_budget += 1;
    fixture.write_manifest(&input)?;
    let changed = data(&fixture.prepare("one-base-unit-change")?)?;
    assert_eq!(
        changed["canonical_manifest_hex"],
        hex(&input.canonical_bytes()?)
    );
    assert_ne!(
        first["observed_signing_domain"],
        changed["observed_signing_domain"]
    );
    Ok(())
}

#[test]
fn prepare_rejects_unknown_nested_grant_field() -> TestResult {
    let fixture = Fixture::new("unknown-grant-field")?;
    let raw = serde_json::to_string(&manifest()?)?;
    let changed = raw.replacen(
        "\"remaining_debit_budget\":",
        "\"phone_can_raise_limit\":true,\"remaining_debit_budget\":",
        1,
    );
    assert_ne!(raw, changed);
    fs::write(&fixture.manifest, changed)?;
    rejected(
        &fixture.prepare("unknown-grant-field")?,
        "unknown UCA grant field",
    );
    Ok(())
}

#[test]
fn prepare_rejects_duplicate_top_level_and_grant_fields() -> TestResult {
    let fixture = Fixture::new("duplicate-fields")?;
    let input = manifest()?;
    let raw = serde_json::to_string(&input)?;
    let duplicate_top = format!(
        "{{\"genesis\":{},{}",
        serde_json::to_string(&input.genesis)?,
        &raw[1..]
    );
    fs::write(&fixture.manifest, duplicate_top)?;
    rejected(&fixture.prepare("duplicate-top")?, "duplicate field");
    let duplicate_grant = raw.replacen(
        "\"authorization_epoch\":1",
        "\"authorization_epoch\":1,\"authorization_epoch\":2",
        1,
    );
    assert_ne!(raw, duplicate_grant);
    fs::write(&fixture.manifest, duplicate_grant)?;
    rejected(&fixture.prepare("duplicate-grant")?, "duplicate field");
    Ok(())
}

#[test]
fn prepare_rejects_oversized_and_trailing_json() -> TestResult {
    let fixture = Fixture::new("document-bounds")?;
    let raw = serde_json::to_vec(&manifest()?)?;
    let mut oversized = vec![b' '; MAX_STARTUP_DOCUMENT_BYTES + 1];
    oversized.extend_from_slice(&raw);
    fs::write(&fixture.manifest, oversized)?;
    let output = fixture.prepare("oversized")?;
    assert_eq!(output.status.code(), Some(3));
    rejected(
        &output,
        &format!("must be a nonempty regular file of at most {MAX_STARTUP_DOCUMENT_BYTES} bytes"),
    );
    let mut trailing = raw;
    trailing.extend_from_slice(b"{}");
    fs::write(&fixture.manifest, trailing)?;
    rejected(&fixture.prepare("trailing")?, "trailing characters");
    Ok(())
}

#[test]
fn wrong_domain_rejects_before_approvals_or_library_access() -> TestResult {
    let fixture = Fixture::new("domain-before-load")?;
    let input = manifest()?;
    fixture.write_manifest(&input)?;
    // Neither this approvals path nor the library exists. Domain mismatch must
    // win before either is opened, not merely fail incidentally at the loader.
    let mut wrong = input.signing_domain()?;
    wrong[0] ^= 1;
    let output = fixture.verify(
        "wrong-domain",
        wrong,
        &fixture.root.join("absent.so"),
        [0; 32],
    )?;
    rejected(
        &output,
        "manifest differs from independently reviewed signing domain",
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("original AOEM loader"));
    Ok(())
}

#[test]
fn wrong_library_hash_rejects_before_loading_invalid_library() -> TestResult {
    let fixture = Fixture::new("hash-before-load")?;
    let input = manifest()?;
    fixture.write_manifest(&input)?;
    fixture.write_approvals(&[])?;
    let library = fixture
        .root
        .join("intentionally-not-an-executable-library.so");
    fs::write(&library, b"not a dynamic library; must never load")?;
    let output = fixture.verify("wrong-hash", input.signing_domain()?, &library, [0; 32])?;
    rejected(&output, "AOEM library SHA-256 mismatch before load");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("original AOEM loader"));
    Ok(())
}

#[derive(Deserialize)]
struct VerifiedEnvelope {
    ok: bool,
    data: VerifiedData,
}

#[derive(Deserialize)]
struct VerifiedData {
    identity_and_asset_approvals_verified: bool,
    expected_domain_verified: bool,
    operator_launch_authorized: bool,
    activation_ready: bool,
    database_created: bool,
    state_published: bool,
    observed_signing_domain: String,
    observed_initial_root: String,
    observed_final_commitment: String,
    review_only_startup_fragment: VerifiedFragment,
}

#[derive(Deserialize)]
struct VerifiedFragment {
    genesis: GenesisConfig,
    uca_initialization: UcaInitializationConfigV2,
}

#[test]
#[ignore = "requires trusted NOVOVM_AOEM_TEST_LIBRARY; uses real AOEM test keys only in memory, never activates a chain"]
fn actual_aoem_approvals_match_original_startup_and_reject_forgery() -> TestResult {
    use novovm_exec::pq_signature::AoemMldsaParameterSet;
    use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};

    let library = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .ok_or("explicit trusted NOVOVM_AOEM_TEST_LIBRARY required")?,
    )
    .canonicalize()?;
    assert_eq!(
        library,
        aoem_bindings::default_host_dll_path().canonicalize()?,
        "key generation and CLI must use the same trusted library"
    );
    let runtime = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
    let signer = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let keys = (0..8)
        .map(|_| signer.generate_keypair())
        .collect::<Result<Vec<_>, _>>()?;
    let fixture = Fixture::new("actual-aoem")?;
    let mut input = manifest()?;
    for (index, account) in input.accounts.iter_mut().enumerate() {
        account.management_public_key = keys[index].public_key.clone();
        account.grants[0].spend_public_key = keys[index + 2].public_key.clone();
        account.grants[0].grant.key_ref = spend_key_ref_v1(&keys[index + 2].public_key)?;
        account.recovery_public_key = Some(keys[index + 4].public_key.clone());
        input.identities[index].identity_public_key = keys[index + 6].public_key.clone();
    }
    for validator in &mut input.genesis.validators {
        validator.public_key = signer.generate_keypair()?.public_key;
    }
    let mut approvals = Vec::new();
    for (subject, public_key) in input.required_approval_subjects() {
        let key = keys
            .iter()
            .find(|key| key.public_key == public_key)
            .ok_or("missing local purpose key")?;
        let message = input.approval_signing_bytes(&subject)?;
        approvals.push(UcaGenesisApprovalV2 {
            subject,
            signature: signer.sign(
                &key.public_key,
                &key.expanded_secret_key,
                &message,
                INITIALIZATION_CONTEXT_V2,
            )?,
        });
    }
    let domain = input.signing_domain()?;
    let library_hash = Sha256::digest(fs::read(&library)?).into();
    fixture.write_manifest(&input)?;
    fixture.write_approvals(&approvals)?;
    let success = fixture.verify("verified", domain, &library, library_hash)?;
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stdout)
    );
    // Deserialize the fragment directly into original types: using Value first
    // could silently round its u128 policy/grant numbers in the test itself.
    let observed: VerifiedEnvelope = serde_json::from_slice(&success.stdout)?;
    assert!(observed.ok);
    let observed = observed.data;
    assert!(observed.identity_and_asset_approvals_verified && observed.expected_domain_verified);
    assert!(!observed.operator_launch_authorized && !observed.activation_ready);
    assert!(!observed.database_created && !observed.state_published);
    assert_eq!(observed.observed_signing_domain, hex(&domain));
    let fragment = observed.review_only_startup_fragment;
    assert_eq!(fragment.genesis.genesis_config_commitment, domain);
    assert_eq!(
        fragment.genesis.policy.clearing_daily_nov_hard_limit,
        LARGE_EXACT_AMOUNT + 7
    );
    assert_eq!(
        fragment.genesis.allocations[0].amount,
        (NOV_FIXED_SUPPLY_BASE_UNITS - 2000).to_string()
    );
    assert_eq!(
        fragment.uca_initialization.accounts[0].grants[0]
            .grant
            .max_debit_per_intent,
        LARGE_EXACT_AMOUNT
    );
    assert_eq!(
        fragment.uca_initialization.accounts[0].grants[0]
            .grant
            .remaining_debit_budget,
        LARGE_EXACT_AMOUNT + 5
    );
    let checked = fragment
        .uca_initialization
        .verify(&fragment.genesis, &runtime)?;
    let original = input.compile(&runtime, domain, &approvals)?;
    assert_eq!(checked.state().root(), original.state().root());
    assert_eq!(checked.final_commitment(), original.final_commitment());
    assert_eq!(
        observed.observed_initial_root,
        hex(&original.state().root())
    );
    assert_eq!(
        observed.observed_final_commitment,
        hex(&original.final_commitment())
    );

    fixture.write_approvals(&approvals[..1])?;
    rejected(
        &fixture.verify("missing", domain, &library, library_hash)?,
        "missing/extra identity or asset approval",
    );
    let mut duplicate = approvals.clone();
    duplicate[1] = duplicate[0].clone();
    fixture.write_approvals(&duplicate)?;
    rejected(
        &fixture.verify("duplicate", domain, &library, library_hash)?,
        "duplicate, unknown or malformed identity/asset approval",
    );
    let mut wrong = approvals.clone();
    wrong[0].signature = signer.sign(
        &keys[1].public_key,
        &keys[1].expanded_secret_key,
        &input.approval_signing_bytes(&wrong[0].subject)?,
        INITIALIZATION_CONTEXT_V2,
    )?;
    fixture.write_approvals(&wrong)?;
    rejected(
        &fixture.verify("wrong-signer", domain, &library, library_hash)?,
        "original genesis compiler",
    );
    let mut corrupt = approvals.clone();
    corrupt[0].signature[0] ^= 1;
    fixture.write_approvals(&corrupt)?;
    rejected(
        &fixture.verify("corrupt-signature", domain, &library, library_hash)?,
        "original genesis compiler",
    );
    let mut reused = approvals.clone();
    reused[2].signature = reused[0].signature.clone();
    fixture.write_approvals(&reused)?;
    rejected(
        &fixture.verify(
            "identity-is-not-asset-approval",
            domain,
            &library,
            library_hash,
        )?,
        "original genesis compiler",
    );
    let mut unbound = approvals.clone();
    let public_key = input.required_approval_subjects()[0].1.clone();
    let key = keys
        .iter()
        .find(|key| key.public_key == public_key)
        .ok_or("missing local key")?;
    unbound[0].signature = signer.sign(
        &key.public_key,
        &key.expanded_secret_key,
        &input.canonical_bytes()?,
        INITIALIZATION_CONTEXT_V2,
    )?;
    fixture.write_approvals(&unbound)?;
    rejected(
        &fixture.verify(
            "manifest-only-not-subject-approved",
            domain,
            &library,
            library_hash,
        )?,
        "original genesis compiler",
    );
    fixture.write_approvals(&approvals)?;
    input.accounts[0].grants[0].grant.remaining_debit_budget += 1;
    fixture.write_manifest(&input)?;
    // Pass the changed domain deliberately so stale signatures, rather than a
    // preliminary domain mismatch, must reject the altered budget.
    rejected(
        &fixture.verify(
            "altered-budget",
            input.signing_domain()?,
            &library,
            library_hash,
        )?,
        "original genesis compiler",
    );
    Ok(())
}
