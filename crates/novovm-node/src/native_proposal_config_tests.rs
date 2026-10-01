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
