use anyhow::Result;
use novovm_adapter_api::{
    default_chain_id, ChainConfig, ChainType, SerializationFormat, StateIR, TxIR, TxType,
};
use novovm_adapter_novovm::{create_native_adapter, supports_native_chain};

fn encode_address(seed: u64) -> Vec<u8> {
    let mut out = vec![0u8; 20];
    out[12..20].copy_from_slice(&seed.to_be_bytes());
    out
}

fn sample_transfer(chain_id: u64, nonce: u64, value: u128) -> TxIR {
    let mut tx = TxIR {
        hash: Vec::new(),
        from: encode_address(1000),
        to: Some(encode_address(2000)),
        value,
        gas_limit: 21_000,
        gas_price: 1,
        nonce,
        data: Vec::new(),
        signature: vec![1u8; 32],
        chain_id,
        tx_type: TxType::Transfer,
        source_chain: None,
        target_chain: None,
    };
    tx.compute_hash();
    tx
}

#[test]
fn native_adapter_executes_transfer_and_updates_state() -> Result<()> {
    let chain_id = default_chain_id(ChainType::NovoVM);
    let mut adapter = create_native_adapter(ChainConfig::novovm(chain_id))?;
    adapter.initialize()?;

    let tx = sample_transfer(chain_id, 0, 7);
    let raw = tx.serialize(SerializationFormat::Bincode)?;
    let parsed = adapter.parse_transaction(&raw)?;
    assert!(adapter.verify_transaction(&parsed)?);

    let mut state = StateIR::new();
    adapter.execute_transaction(&parsed, &mut state)?;
    let root = adapter.state_root()?;
    assert_eq!(root.len(), 32);
    assert_eq!(state.state_root.len(), 32);
    assert_eq!(adapter.get_balance(&encode_address(2000))?, 7);
    assert_eq!(adapter.get_nonce(&encode_address(1000))?, 1);

    adapter.shutdown()?;
    Ok(())
}

#[test]
fn native_adapter_rejects_wrong_chain_id() -> Result<()> {
    let chain_id = default_chain_id(ChainType::NovoVM);
    let mut adapter = create_native_adapter(ChainConfig::novovm(chain_id))?;
    adapter.initialize()?;

    let tx = sample_transfer(chain_id + 1, 0, 3);
    assert!(!adapter.verify_transaction(&tx)?);

    adapter.shutdown()?;
    Ok(())
}

#[test]
fn native_adapter_supports_non_novovm_samples() {
    assert!(supports_native_chain(ChainType::NovoVM));
    assert!(supports_native_chain(ChainType::EVM));
    assert!(supports_native_chain(ChainType::BNB));
    assert!(supports_native_chain(ChainType::Custom));
    assert!(!supports_native_chain(ChainType::Solana));
}

#[test]
fn native_adapter_accepts_evm_chain_config() -> Result<()> {
    let chain_id = default_chain_id(ChainType::EVM);
    let cfg = ChainConfig {
        chain_type: ChainType::EVM,
        chain_id,
        name: "EVM".to_string(),
        enabled: true,
        custom_config: None,
    };
    let mut adapter = create_native_adapter(cfg)?;
    adapter.initialize()?;

    let tx = sample_transfer(chain_id, 0, 9);
    assert!(adapter.verify_transaction(&tx)?);

    let mut state = StateIR::new();
    adapter.execute_transaction(&tx, &mut state)?;
    assert_eq!(adapter.get_balance(&encode_address(2000))?, 9);
    adapter.shutdown()?;
    Ok(())
}

#[test]
fn native_adapter_unified_account_guard_rejects_replay_nonce() -> Result<()> {
    let chain_id = default_chain_id(ChainType::NovoVM);
    let mut adapter = create_native_adapter(ChainConfig::novovm(chain_id))?;
    adapter.initialize()?;

    let mut state = StateIR::new();
    let first = sample_transfer(chain_id, 0, 5);
    adapter.execute_transaction(&first, &mut state)?;

    let replay = sample_transfer(chain_id, 0, 3);
    let err = adapter
        .execute_transaction(&replay, &mut state)
        .expect_err("replay nonce should be rejected by unified account ingress guard");
    assert!(
        err.to_string().contains("nonce rejected"),
        "unexpected replay error: {}",
        err
    );

    adapter.shutdown()?;
    Ok(())
}
