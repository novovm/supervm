#![forbid(unsafe_code)]

mod bincode_compat;

pub mod chain_adapter;
pub mod evm_mirror;
pub mod ir;
pub mod mapped_asset;
pub mod unified_account;

pub use chain_adapter::{default_chain_id, AdapterFactory, ChainAdapter, ChainConfig, ChainType};
pub use evm_mirror::{
    AtomicBroadcastReadyV1, AtomicCrossChainIntentV1, AtomicIntentReceiptV1, AtomicIntentStatus,
    EvmFeeIncomeRecordV1, EvmFeePayoutInstructionV1, EvmFeeSettlementPolicyV1,
    EvmFeeSettlementRecordV1, EvmFeeSettlementResultV1, EvmFeeSettlementSnapshotV1,
    EvmMempoolIngressFrameV1, EvmMirrorNodeAdapterExt, EvmNodeServiceRole,
};
pub use ir::{
    AccountState, BlockIR, EvmAccessListEntryV1, SerializationFormat, StateIR, TxExecutionPolicyV1,
    TxIR, TxType,
};
pub use mapped_asset::{
    MappedAssetLockProof, MappedAssetOperation, MappedAssetOperationKind, MappedAssetRecord,
    MappedAssetSourceChain, MappedAssetStatus, MappedLockProofFormat,
};
pub use unified_account::{
    AccountAction, AccountAuditEvent, AccountPolicy, AccountRole, BindingState, KycPolicyMode,
    NonceScope, PersonaAddress, PersonaType, ProtocolKind, RouteDecision, RouteRequest,
    Type4PolicyMode, UcaKeyAlgo, UcaKeyProofType, UcaPrimaryKeyBinding, UcaStatus,
    UnifiedAccountError, UnifiedAccountRouter,
};
