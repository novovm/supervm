use anyhow::Result;

use novovm_adapter_api::ir::{AccountState, BlockIR, SerializationFormat, StateIR, TxIR, TxType};

#[test]
fn tx_hash_and_bincode_roundtrip() -> Result<()> {
    let mut tx = TxIR::transfer(vec![1u8; 20], vec![2u8; 20], 1000, 0, 1);
    tx.compute_hash();

    assert!(!tx.hash.is_empty());
    assert_eq!(tx.tx_type, TxType::Transfer);

    let encoded = tx.serialize(SerializationFormat::Bincode)?;
    let decoded = TxIR::deserialize(&encoded, SerializationFormat::Bincode)?;

    assert_eq!(decoded.from, vec![1u8; 20]);
    assert_eq!(decoded.to, Some(vec![2u8; 20]));
    assert_eq!(decoded.value, 1000);

    Ok(())
}

#[test]
fn state_set_get_and_bincode_roundtrip() -> Result<()> {
    let mut state = StateIR::new();

    let address = vec![0xAB; 20];
    state.set_account(
        address.clone(),
        AccountState {
            balance: 42,
            nonce: 7,
            code_hash: None,
            storage_root: vec![0; 32],
        },
    );

    let account = state.get_account(&address).expect("account must exist");
    assert_eq!(account.balance, 42);
    assert_eq!(account.nonce, 7);

    state.set_storage(address.clone(), b"k".to_vec(), b"v".to_vec());
    assert_eq!(
        state.get_storage(&address, b"k").map(Vec::as_slice),
        Some(&b"v"[..])
    );

    let encoded = state.serialize(SerializationFormat::Bincode)?;
    let decoded = StateIR::deserialize(&encoded, SerializationFormat::Bincode)?;

    assert_eq!(
        decoded
            .get_account(&address)
            .expect("decoded account must exist")
            .balance,
        42
    );

    Ok(())
}

#[test]
fn block_genesis_roundtrip() -> Result<()> {
    let block = BlockIR::genesis(1);
    assert_eq!(block.number, 0);
    assert!(block.transactions.is_empty());

    let encoded = block.serialize(SerializationFormat::Bincode)?;
    let decoded = BlockIR::deserialize(&encoded, SerializationFormat::Bincode)?;
    assert_eq!(decoded.number, 0);

    Ok(())
}

#[cfg(feature = "serde_json")]
#[test]
fn tx_json_roundtrip() -> Result<()> {
    let mut tx = TxIR::cross_chain_transfer(vec![1u8; 20], vec![2u8; 20], 9, 3, 1, 56);
    tx.compute_hash();

    let encoded = tx.serialize(SerializationFormat::Json)?;
    let decoded = TxIR::deserialize(&encoded, SerializationFormat::Json)?;

    assert_eq!(decoded.tx_type, TxType::CrossChainTransfer);
    assert_eq!(decoded.source_chain, Some(1));
    assert_eq!(decoded.target_chain, Some(56));

    Ok(())
}
