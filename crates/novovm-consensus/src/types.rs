// Phase 4.2: BFT Core Types
//
// 定义 BFT 共识的核心数据类型

use serde::{Deserialize, Serialize};
use std::fmt;

/// 节点 ID（验证者标识）
pub type NodeId = u32;

/// 区块高度
pub type Height = u64;

/// 哈希值（32 字节）
pub type Hash = [u8; 32];

/// BFT 错误类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BFTError {
    /// 无效提案
    InvalidProposal(String),
    
    /// 签名验证失败
    InvalidSignature(String),
    
    /// 未达到法定人数
    InsufficientVotes { required: usize, received: usize },
    
    /// 高度不匹配
    HeightMismatch { expected: Height, got: Height },
    
    /// 未找到前置 QC
    MissingPreviousQC,
    
    /// 节点不是验证者
    NotValidator(NodeId),
    
    /// 重复投票
    DuplicateVote(NodeId),
    
    /// 超时
    Timeout(String),
    
    /// 内部错误
    Internal(String),
}

impl fmt::Display for BFTError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BFTError::InvalidProposal(msg) => write!(f, "Invalid proposal: {}", msg),
            BFTError::InvalidSignature(msg) => write!(f, "Invalid signature: {}", msg),
            BFTError::InsufficientVotes { required, received } => {
                write!(f, "Insufficient votes: {}/{}", received, required)
            }
            BFTError::HeightMismatch { expected, got } => {
                write!(f, "Height mismatch: expected {}, got {}", expected, got)
            }
            BFTError::MissingPreviousQC => write!(f, "Missing previous QC"),
            BFTError::NotValidator(id) => write!(f, "Node {} is not a validator", id),
            BFTError::DuplicateVote(id) => write!(f, "Duplicate vote from node {}", id),
            BFTError::Timeout(msg) => write!(f, "Timeout: {}", msg),
            BFTError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for BFTError {}

/// BFT 结果类型
pub type BFTResult<T> = Result<T, BFTError>;

/// BFT 提案（由 Leader 发起）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BFTProposal {
    /// Epoch ID
    pub epoch_id: u64,
    
    /// 区块高度
    pub height: Height,
    
    /// 提案者 ID
    pub proposer: NodeId,
    
    /// 状态承诺哈希（默认由 batch results 计算；也可由执行层覆盖为更强的状态承诺根）
    pub state_delta_hash: Hash,
    
    /// 前一个 Epoch 的 QC
    pub prev_qc_hash: Hash,
    
    /// 时间戳（毫秒）
    pub timestamp: u64,
}

impl BFTProposal {
    /// 计算提案的哈希
    pub fn hash(&self) -> Hash {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(self.epoch_id.to_le_bytes());
        hasher.update(self.height.to_le_bytes());
        hasher.update(self.proposer.to_le_bytes());
        hasher.update(self.state_delta_hash);
        hasher.update(self.prev_qc_hash);
        hasher.update(self.timestamp.to_le_bytes());
        hasher.finalize().into()
    }
}

/// 投票权重配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSet {
    /// 验证者列表
    pub validators: Vec<NodeId>,
    
    /// 投票权重（默认每个验证者权重为 1）
    pub weights: Vec<u64>,
    
    /// 总权重
    pub total_weight: u64,
}

impl ValidatorSet {
    /// 创建等权重验证者集合
    pub fn new_equal_weight(validators: Vec<NodeId>) -> Self {
        let total = validators.len() as u64;
        let weights = vec![1; validators.len()];
        Self {
            validators,
            weights,
            total_weight: total,
        }
    }
    
    /// 检查是否是验证者
    pub fn is_validator(&self, node_id: NodeId) -> bool {
        self.validators.contains(&node_id)
    }
    
    /// 获取验证者索引
    pub fn get_index(&self, node_id: NodeId) -> Option<usize> {
        self.validators.iter().position(|&id| id == node_id)
    }
    
    /// 计算法定人数（2f+1，容忍 f 个故障）
    ///
    /// **性能警告**：
    /// - BFT 共识适合 4-100 个验证者
    /// - 不推荐大规模场景（1000+ 验证者）：
    ///   - 投票延迟会线性增加
    ///   - 网络带宽消耗大（O(N²) 通信复杂度）
    ///   - 建议使用分层架构或 PoS + 委托机制
    ///
    /// # Examples
    /// ```
    /// # use novovm_consensus::ValidatorSet;
    /// let v4 = ValidatorSet::new_equal_weight(vec![0,1,2,3]);
    /// assert_eq!(v4.quorum_size(), 3);  // 4个验证者需要3票
    ///
    /// let v7 = ValidatorSet::new_equal_weight(vec![0,1,2,3,4,5,6]);
    /// assert_eq!(v7.quorum_size(), 5);  // 7个验证者需要5票
    /// ```
    pub fn quorum_size(&self) -> u64 {
        // 2f + 1 = ceil(2 * total / 3)
        (self.total_weight * 2).div_ceil(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_set_quorum() {
        // 4 个验证者，法定人数 = 3（容忍 1 个故障）
        let validators = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        assert_eq!(validators.total_weight, 4);
        assert_eq!(validators.quorum_size(), 3); // (4*2+2)/3 = 3.33 → 3
        
        // 7 个验证者，法定人数 = 5（容忍 2 个故障）
        let validators = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(validators.total_weight, 7);
        assert_eq!(validators.quorum_size(), 5); // (7*2+2)/3 = 5.33 → 5
    }

    #[test]
    fn test_proposal_hash_deterministic() {
        let proposal = BFTProposal {
            epoch_id: 1,
            height: 100,
            proposer: 0,
            state_delta_hash: [0u8; 32],
            prev_qc_hash: [1u8; 32],
            timestamp: 1234567890,
        };
        
        let hash1 = proposal.hash();
        let hash2 = proposal.hash();
        assert_eq!(hash1, hash2); // 哈希必须确定性
    }
}

