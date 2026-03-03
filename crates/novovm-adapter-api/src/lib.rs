#![forbid(unsafe_code)]

pub mod chain_adapter;
pub mod ir;

pub use chain_adapter::{default_chain_id, AdapterFactory, ChainAdapter, ChainConfig, ChainType};
pub use ir::{AccountState, BlockIR, SerializationFormat, StateIR, TxIR, TxType};
