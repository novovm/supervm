#![forbid(unsafe_code)]

pub mod chain_adapter;
pub mod ir;
pub mod unified_account;

pub use chain_adapter::{default_chain_id, AdapterFactory, ChainAdapter, ChainConfig, ChainType};
pub use ir::{AccountState, BlockIR, SerializationFormat, StateIR, TxIR, TxType};
pub use unified_account::{
    AccountAction, AccountAuditEvent, AccountPolicy, AccountRole, BindingState, NonceScope,
    PersonaAddress, PersonaType, ProtocolKind, RouteDecision, RouteRequest, UcaStatus,
    UnifiedAccountError, UnifiedAccountRouter,
};
