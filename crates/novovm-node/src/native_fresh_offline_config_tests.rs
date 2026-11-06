// Offline candidate/ledger fixtures retain the existing explicit configuration
// validator. They never open a WSS transport or run the retired seal service.
fn prepare_offline_candidate_config(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    id: [u8; 32],
    parent: Option<[u8; 32]>,
) {
    use crate::native_block_seal::service_config::NovNativeSealServiceConfigV1 as Config;
    use crate::native_block_seal_overlay::{
        NovNativeSealEpochAuthorityV1 as Authority,
        NovNativeSealValidatorTransportBindingV1 as Binding,
    };
    let chain = compiled.identity().chain_id();
    let pin = compiled.config_commitment();
    let artifact = workspace::load_block_artifact_v1(chain, id, params)
        .unwrap()
        .unwrap();
    let bindings = compiled
        .validator_set()
        .validators
        .iter()
        .map(|validator| Binding {
            validator_id: validator.validator_id,
            transport_peer_id: crate::product_relay_client::component_transport_peer(
                &validator.public_key,
            ),
        })
        .collect();
    let derive = |view: &crate::native_block_ledger::NovNativeBlockLedgerV1| {
        let (config, _) = view.fresh_genesis_seal_config_v1(chain)?.unwrap();
        Authority::derive_operator_pinned_fresh_genesis_epoch(config, pin, bindings)
    };
    let authority = match parent {
        Some(parent) => {
            workspace::with_verified_finalized_successor_v1(chain, parent, id, pin, params, derive)
        }
        None => workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, derive),
    }
    .unwrap();
    let height = artifact.block().header.height;
    let leader = authority.expected_leader(height, 0).unwrap();
    let public_key = authority
        .validator_set
        .validator(leader)
        .unwrap()
        .public_key;
    let key = (1..=4)
        .map(|seed| ed25519_dalek::SigningKey::from_bytes(&[seed; 32]))
        .find(|key| key.verifying_key().to_bytes() == public_key)
        .unwrap();
    let root = path.with_extension(format!("fresh-service-{height}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("authority.json"),
        serde_json::to_vec(&authority).unwrap(),
    )
    .unwrap();
    fs::write(root.join("signer.hex"), to_hex(&key.to_bytes())).unwrap();
    let config_path = root.join("service.json");
    let config = serde_json::json!({
        "schema":"novovm-native-seal-service/v1", "enabled":true, "decision_v3_enabled":true,
        "fresh_genesis_config_commitment":to_hex(&pin), "isolated_workspace_id":to_hex(&id),
        "finalized_parent_workspace_id":parent.map(|id| to_hex(&id)),
        "chain_id":chain, "height":height, "block_hash":to_hex(&artifact.block().header.block_hash),
        "authority_path":"authority.json", "signer_key_path":"signer.hex", "seal_store_path":"seal",
        "round_timeout_ms":300000, "poll_interval_ms":100,
        "ingress_per_source_per_second":16, "ingress_per_poll":32,
    });
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    Config::load(&config_path, chain).unwrap();
}
