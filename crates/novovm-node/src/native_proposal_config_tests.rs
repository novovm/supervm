// Included inside native_block_seal_service_config::tests to reuse its
// read-only, configuration-relative fixture. No ledger or finality is invented.

#[test]
fn native_proposal_config_default_and_explicit_transaction_limits() {
    let mut fixture = Fixture::new();
    assert!(fixture.config.get("proposal_max_transactions").is_none());
    assert_eq!(fixture.load().unwrap().proposal_max_transactions, 16);
    for limit in [1, 16, 17, 64, 1024] {
        fixture.config["proposal_max_transactions"] = json!(limit);
        fixture.write();
        let before = fs::read(fixture.path()).unwrap();
        let loaded = fixture.load().unwrap();
        assert_eq!(loaded.proposal_max_transactions, limit);
        assert_eq!(fs::read(fixture.path()).unwrap(), before);
        assert!(!loaded.seal_store_path.exists());
    }
}

#[test]
fn native_proposal_config_rejects_unbounded_or_malformed_transaction_limits() {
    let mut fixture = Fixture::new();
    for invalid in [
        json!(0),
        json!(1025),
        json!(u64::MAX),
        json!(-1),
        json!(1.5),
        json!("32"),
        json!(true),
        Value::Null,
    ] {
        fixture.config["proposal_max_transactions"] = invalid.clone();
        fixture.write();
        let before = fs::read(fixture.path()).unwrap();
        assert!(fixture.load().is_err(), "accepted limit {invalid}");
        assert_eq!(fs::read(fixture.path()).unwrap(), before);
        assert!(!fixture.root.join("data/seal").exists());
    }
    fixture.config["proposal_max_transactions"] = json!(32);
    fixture.write();
    let valid = fixture.load().unwrap();
    for limit in [0, 1025, usize::MAX] {
        let mut invalid = valid.clone();
        invalid.proposal_max_transactions = limit;
        assert!(
            invalid.validate(22922).is_err(),
            "accepted runtime limit {limit}"
        );
    }
}

#[test]
fn native_proposal_config_collect_defaults_disabled_and_accepts_explicit_bounds() {
    let mut fixture = Fixture::new();
    assert!(fixture.config.get("proposal_collect_ms").is_none());
    assert_eq!(fixture.load().unwrap().proposal_collect, Duration::ZERO);
    fixture.config["round_timeout_ms"] = json!(4000);
    for millis in [0, 1, 250, 1000] {
        fixture.config["proposal_collect_ms"] = json!(millis);
        fixture.write();
        let before = fs::read(fixture.path()).unwrap();
        let loaded = fixture.load().unwrap();
        assert_eq!(loaded.proposal_collect, Duration::from_millis(millis));
        assert_eq!(loaded.round_timeout, Duration::from_secs(4));
        assert_eq!(loaded.poll_interval, Duration::from_millis(100));
        assert_eq!(fs::read(fixture.path()).unwrap(), before);
        assert!(!loaded.seal_store_path.exists());
    }
}

fn assert_proposal_collect_rejected_read_only(fixture: &Fixture) {
    let paths = [
        fixture.path(),
        fixture.root.join("configuration/authority.json"),
        fixture.root.join("keys/validator.hex"),
    ];
    let before = paths
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect::<Vec<_>>();
    assert!(
        fixture.load().is_err(),
        "accepted invalid proposal collection"
    );
    for (path, expected) in paths.iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), expected);
    }
    assert!(fs::read_dir(fixture.root.join("data"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn native_proposal_config_collect_rejects_malformed_and_unbounded_values_read_only() {
    let mut fixture = Fixture::new();
    fixture.config["round_timeout_ms"] = json!(4000);
    for invalid in [
        json!(1001),
        json!(u64::MAX),
        json!(-1),
        json!(1.5),
        json!("250"),
        json!(true),
        Value::Null,
    ] {
        fixture.config["proposal_collect_ms"] = invalid;
        fixture.write();
        assert_proposal_collect_rejected_read_only(&fixture);
    }
    // Exercise JSON integer overflow itself, not a duplicate field or a string.
    fixture
        .config
        .as_object_mut()
        .unwrap()
        .remove("proposal_collect_ms");
    let mut overflow = serde_json::to_string(&fixture.config).unwrap();
    overflow.pop();
    overflow.push_str(",\"proposal_collect_ms\":18446744073709551616}");
    fs::write(fixture.path(), overflow).unwrap();
    assert_proposal_collect_rejected_read_only(&fixture);
}

#[test]
fn native_proposal_config_collect_quarter_round_limit_is_rechecked_at_runtime() {
    let mut fixture = Fixture::new();
    fixture.config["round_timeout_ms"] = json!(1000);
    fixture.config["proposal_collect_ms"] = json!(250);
    fixture.write();
    let valid = fixture.load().unwrap();
    assert_eq!(valid.proposal_collect, Duration::from_millis(250));
    fixture.config["proposal_collect_ms"] = json!(251);
    fixture.write();
    assert_proposal_collect_rejected_read_only(&fixture);
    for duration in [
        Duration::from_millis(251),
        Duration::from_nanos(250_000_001),
    ] {
        let mut invalid = valid.clone();
        invalid.proposal_collect = duration;
        assert!(invalid.validate(22922).is_err());
    }
    let mut longer_round = valid.clone();
    longer_round.round_timeout = Duration::from_secs(4);
    longer_round.proposal_collect = Duration::from_secs(1);
    longer_round.validate(22922).unwrap();
    for duration in [Duration::from_millis(1001), Duration::MAX] {
        let mut invalid = longer_round.clone();
        invalid.proposal_collect = duration;
        assert!(invalid.validate(22922).is_err());
    }
    // Changing only the round timer must also invalidate a previously valid wait.
    longer_round.round_timeout = Duration::from_secs(1);
    assert!(longer_round.validate(22922).is_err());
    longer_round.proposal_collect = Duration::ZERO;
    longer_round.validate(22922).unwrap();
}
