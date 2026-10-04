//! Configuration/compiler regression only: no native execution or TPS claim.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use crate::native_pipeline::business::nov_transfer_batch::NovTransferBody;
use crate::native_pipeline::ingress::batch::authenticate_batch_for_proof;
use crate::native_pipeline::ingress::wire::{
    encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use ed25519_dalek::Signer;
use novovm_network::duplex::product_relay_client::{
    ProductRelayClientConfigV1, ProductRelayTlsTrustV1,
};

pub(super) fn signed_raw(index: u64, nonce: u64) -> Vec<u8> {
    let mut seed = [0x5a; 32];
    seed[..8].copy_from_slice(&index.to_le_bytes());
    let key = SigningKey::from_bytes(&seed);
    let mut recipient = vec![0xee; 20];
    recipient[..8].copy_from_slice(&index.to_le_bytes());
    let mut transaction = TransferV3 {
        chain_id: 71,
        from: key.verifying_key().to_bytes().to_vec(),
        to: recipient,
        asset: "NOV".into(),
        amount: 1,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 500,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    let signature = key.sign(&signing_message(&transaction).unwrap());
    transaction
        .signature
        .extend_from_slice(key.verifying_key().as_bytes());
    transaction
        .signature
        .extend_from_slice(&signature.to_bytes());
    encode_transfer_v3(&transaction).unwrap()
}

fn config(size: usize) -> ResidentConfig {
    let policy = DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    };
    ResidentConfig {
        profile: config::PROFILE.into(),
        library: "not-opened-library".into(),
        database: "not-opened-database".into(),
        signing_key_file: "not-read-test-key".into(),
        rpc_addr: "127.0.0.1:0".parse().unwrap(),
        workers: 1,
        batch_size: size,
        proof: None,
        relay: ProductRelayClientConfigV1 {
            endpoint: "wss://127.0.0.1:9/not-opened".into(),
            expected_relay_peer_id: "not-connected-test-peer".into(),
            connect_timeout_ms: 100,
            read_timeout_ms: 10,
            tls_trust: ProductRelayTlsTrustV1::NativeWebPki,
        },
        genesis: GenesisConfig {
            chain_id: 71,
            validator_epoch: 1,
            protocol_commitment: config::protocol_commitment(),
            genesis_config_commitment: [1; 32],
            timestamp_unix_ms: 123,
            validators: Vec::new(),
            allocations: Vec::new(),
            policy,
        },
    }
}

#[test]
fn rpc_pipeline_budget_covers_real_independent_accounts_not_only_shared_receiver() -> Result<()> {
    for size in [1, 128, 1024] {
        let config = config(size);
        let pipeline = pipeline_config(&config);
        let raw: Vec<_> = (0..size).map(|index| signed_raw(index as u64, 0)).collect();
        let authenticate =
            || authenticate_batch_for_proof(71, raw.clone(), pipeline.authentication);
        let body = NovTransferBody::prepare(
            authenticate()?,
            config.genesis.policy.clone(),
            pipeline.plan,
        )?;
        let context = BatchContext {
            chain_id: 71,
            genesis_config_commitment: [1; 32],
            protocol_commitment: config.genesis.protocol_commitment,
            business_program: program_id(),
            semantic_version: SEMANTIC_VERSION,
            effect_contract: effect_contract(&config.genesis.policy)?,
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: empty_root(),
            parent_receipt_root: empty_root(),
            parent_state_version: 0,
            receipt_codec: receipt_codec(),
            height: 1,
            slot: 1,
            timestamp_unix_ms: 124,
        };
        let bound = body.bind(context)?;
        let actual_keys = bound.plan().declared_access().len();
        assert_eq!(
            actual_keys,
            3 * size + 42,
            "compiler-derived complete access set"
        );
        assert!(actual_keys <= pipeline.plan.access_keys);
        assert!(actual_keys <= pipeline.capture.keys);
        assert!(pipeline.plan.access_keys <= 4096);
        assert!(raw.iter().map(Vec::len).sum::<usize>() <= body_byte_limit(size));
        if size >= 128 {
            let old = PlanBudget {
                access_keys: 2 * size + 128,
                ..pipeline.plan
            };
            let error =
                NovTransferBody::prepare(authenticate()?, config.genesis.policy.clone(), old)
                    .err()
                    .expect("old shared-receiver budget must reject arbitrary RPC batch");
            assert!(error.to_string().contains("access budget"));
        }
    }
    Ok(())
}
