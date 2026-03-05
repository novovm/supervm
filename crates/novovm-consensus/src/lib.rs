//! Phase 4.2: Multi-Region BFT Consensus
//!
//! # 架构定位
//!
//! Layer 3 (Consensus Layer) - 最终顺序决策与 Finality 保证
//!
//! # 核心职责
//!
//! - Epoch/Block 最终顺序决策
//! - 不可逆性（Finality）保证
//! - 跨区域信任边界
//!
//! # 非职责（明确排除）
//!
//! - ❌ 不关心 OCCC 如何执行
//! - ❌ 不关心分片内并发
//! - ❌ 不对单笔交易进行共识
//!
//! # 核心原则
//!
//! > Execution wants locality and speculation.  
//! > Consensus wants determinism and finality.  
//! > **Never force one to behave like the other.**
//!
//! # 快速开始
//!
//! ```no_run
//! use novovm_consensus::{BFTEngine, BFTConfig, ValidatorSet};
//! use ed25519_dalek::SigningKey;
//! use rand::rngs::OsRng;
//! use std::collections::HashMap;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 创建验证者集合（4 个节点）
//! let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
//!
//! // 生成密钥
//! let signing_key = SigningKey::generate(&mut OsRng);
//! let mut public_keys = HashMap::new();
//! public_keys.insert(0, signing_key.verifying_key());
//!
//! // 创建 BFT 引擎
//! let config = BFTConfig::default();
//! let mut engine = BFTEngine::new(
//!     config,
//!     0,  // 节点 0
//!     signing_key,
//!     validator_set,
//!     public_keys,
//! )?;
//!
//! // 启动 Epoch
//! engine.start_epoch()?;
//!
//! // 添加 Batch
//! engine.add_batch(1, 100)?;
//! engine.add_batch(2, 150)?;
//!
//! // 提出提案（Leader）
//! if engine.is_leader() {
//!     let mut batch_results = HashMap::new();
//!     batch_results.insert(1, [1u8; 32]);
//!     batch_results.insert(2, [2u8; 32]);
//!     
//!     let proposal = engine.propose_epoch(&batch_results)?;
//!     println!("Proposed Epoch {}", proposal.epoch_id);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # 性能目标（Phase 4.2）
//!
//! - 16 分片部署
//! - 100K 跨分片 TPS
//! - P99 Epoch 延迟 ≤ 5 秒
//! - BFT 共识延迟 ≤ 2 秒
//!
//! # 参考资料
//!
//! - HotStuff-2: Improved BFT Consensus (2023)
//! - AptosBFT: Block-STM + BFT Integration (2022)
//! - Phase 4.2 Implementation Plan: `docs/TPS/New_Final/OCCC/PHASE-4.2-IMPLEMENTATION-PLAN.md`

// Temporarily allow missing docs during development
#![allow(missing_docs)]
#![deny(unsafe_code)]

pub mod bft_engine;
pub mod epoch;
pub mod protocol;
pub mod quorum_cert;
pub mod token_runtime;
pub mod types;

pub use bft_engine::{BFTConfig, BFTEngine, CommitQcTimings, CommittedEpoch};
pub use epoch::{Epoch, EpochConfig, EpochManager};
pub use protocol::{HotStuffProtocol, Phase, ProtocolState};
pub use quorum_cert::{QuorumCertificate, Vote};
pub use types::{
    BFTError, BFTProposal, BFTResult, FeeRoutingOutcome, FeeSplit, GovernanceAccessPolicy,
    GovernanceOp, GovernanceProposal, GovernanceVote, Hash, Height, NetworkDosPolicy, NodeId,
    SlashEvidence, SlashExecution, SlashMode, SlashPolicy, TokenEconomicsPolicy, ValidatorSet,
};
