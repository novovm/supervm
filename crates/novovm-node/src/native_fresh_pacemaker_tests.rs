#[test]
fn candidate_less_pacemaker_quorum_transport_restart_and_no_execution() {
    with_parent_round_fixture_at(1, |path, params, fixture| {
        use crate::native_block_seal::service_config::NovNativeSealServiceConfigV1 as Config;
        use std::time::Duration;
        let authority = fixture.4[0].authority.clone();
        let pin = workspace::load_block_artifact_v1(authority.chain_id, fixture.2[0], params)
            .unwrap()
            .unwrap()
            .fresh_genesis_identity()
            .unwrap()
            .config_commitment();
        let configs = (1..=4)
            .map(|seed| {
                let signer = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
                let local_validator_id = crate::native_block_seal::NovNativeSealValidatorV1::new(
                    signer.verifying_key().to_bytes(),
                    1,
                )
                .unwrap()
                .validator_id;
                Config {
                    propose_successors: true,
                    receive_successors: true,
                    follow_finalized_tip: true,
                    fresh_genesis_config_commitment: Some(pin),
                    finalized_parent_workspace_id: None,
                    isolated_workspace_id: Some(fixture.2[0]),
                    decision_v3_enabled: true,
                    commit_v2_enabled: false,
                    chain_id: authority.chain_id,
                    height: 1,
                    block_hash: fixture.3[0].header.block_hash,
                    justify_qc_hash: None,
                    authority: authority.clone(),
                    signer,
                    local_validator_id,
                    seal_store_path: path.with_extension(format!("pacemaker-{seed}")),
                    protected_paths: ["fixture-config", "fixture-authority", "fixture-signer"]
                        .into_iter()
                        .map(|extension| path.with_extension(extension))
                        .collect(),
                    round_timeout: Duration::from_secs(2),
                    poll_interval: Duration::from_millis(100),
                    ingress_per_source_per_second: 32,
                    ingress_per_poll: 64,
                }
            })
            .collect::<Vec<_>>();
        let before = workspace::list_v1(authority.chain_id, params).unwrap();
        crate::native_block_seal::service::exercise_parent_pacemaker(
            &configs,
            params,
            &path.with_extension("independent-seal-0"),
            || {
                workspace::corrupt_execution_output_for_test_v1(
                    authority.chain_id,
                    fixture.2[0],
                    params,
                )
                .unwrap();
            },
        );
        assert_eq!(
            workspace::list_v1(authority.chain_id, params).unwrap(),
            before
        );
        assert_eq!(
            workspace::load_finalized_genesis_parent_v1(
                authority.chain_id,
                fixture.2[0],
                pin,
                params,
            )
            .unwrap()
            .block(),
            &fixture.3[0]
        );
    });
}
