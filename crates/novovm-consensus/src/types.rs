// Phase 4.2: BFT Core Types
//
// 定义 BFT 共识的核心数据类型

use serde::{Deserialize, Serialize};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fmt;
use std::collections::HashSet;

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

    /// 检测到同高度双签（可触发 slash）
    EquivocationDetected { voter: NodeId, height: Height },

    /// 已被罚没/禁用的验证者仍尝试参与共识
    SlashedValidator(NodeId),
    
    /// 超时
    Timeout(String),

    /// 治理入口已预留，但当前未启用链上执行
    GovernanceNotEnabled(String),
    
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
            BFTError::EquivocationDetected { voter, height } => {
                write!(f, "Equivocation detected from node {} at height {}", voter, height)
            }
            BFTError::SlashedValidator(id) => {
                write!(f, "Validator {} has been slashed/jailed", id)
            }
            BFTError::Timeout(msg) => write!(f, "Timeout: {}", msg),
            BFTError::GovernanceNotEnabled(msg) => {
                write!(f, "Governance not enabled: {}", msg)
            }
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

    /// 当前 view（用于 leader 轮换与提案冲突隔离）
    pub view: u64,
    
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
        hasher.update(self.view.to_le_bytes());
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

/// 斩罚证据（当前用于记录同高度双签）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlashEvidence {
    pub voter_id: NodeId,
    pub height: Height,
    pub first_proposal_hash: Hash,
    pub second_proposal_hash: Hash,
    pub reason: String,
}

impl SlashEvidence {
    pub fn equivocation(
        voter_id: NodeId,
        height: Height,
        first_proposal_hash: Hash,
        second_proposal_hash: Hash,
    ) -> Self {
        Self {
            voter_id,
            height,
            first_proposal_hash,
            second_proposal_hash,
            reason: "equivocation".to_string(),
        }
    }
}

/// 罚没执行记录（将 evidence 转为可生效状态变更）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlashExecution {
    pub voter_id: NodeId,
    pub height: Height,
    pub reason: String,
    pub weight_before: u64,
    pub weight_after: u64,
    pub jailed: bool,
    /// 当前证据累计计数（按 validator 维度）
    pub evidence_count: u32,
    /// 策略阈值（达到后才允许真正 jail）
    pub threshold: u32,
    /// 执行时策略模式（`enforce` / `observe_only`）
    pub policy_mode: String,
    /// 活跃验证者保护下限
    pub min_active_validators: u32,
    /// 本次 jail 的自动解禁高度（`u64::MAX` 表示不自动解禁）
    pub jailed_until_epoch: Height,
    /// 执行时使用的 cooldown（单位：epoch）
    pub cooldown_epochs: u64,
}

/// 罚没治理模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SlashMode {
    /// 强制执行：达到阈值后生效 jail
    Enforce,
    /// 观察模式：仅记录证据与执行记录，不 jail
    ObserveOnly,
}

impl SlashMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SlashMode::Enforce => "enforce",
            SlashMode::ObserveOnly => "observe_only",
        }
    }
}

/// 罚没治理策略（用于主网参数化与治理收口）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlashPolicy {
    /// 执行模式
    pub mode: SlashMode,
    /// 同一验证者达到该证据计数后，才允许 jail
    pub equivocation_threshold: u32,
    /// jail 后活跃验证者数量不能低于该下限
    pub min_active_validators: u32,
    /// 自动解禁窗口（单位：epoch），`0` 表示不自动解禁（保持旧语义）
    pub cooldown_epochs: u64,
}

/// 网络/DoS 治理策略（第三类参数扩展起点）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkDosPolicy {
    /// 每 IP 每秒请求上限（用于 RPC ingress 限流）。
    pub rpc_rate_limit_per_ip: u32,
    /// peer score 降到该阈值以下时触发 ban。
    pub peer_ban_threshold: i32,
}

/// 治理操作（受限执行面）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GovernanceOp {
    /// 更新 slash policy。
    UpdateSlashPolicy { policy: SlashPolicy },
    /// 更新 mempool fee floor（第二类参数）。
    UpdateMempoolFeeFloor { fee_floor: u64 },
    /// 更新网络/DoS 策略（第三类参数）。
    UpdateNetworkDosPolicy { policy: NetworkDosPolicy },
}

/// 治理提案（最小闭环：仅覆盖 slash policy 更新）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceProposal {
    pub proposal_id: u64,
    pub proposer: NodeId,
    pub created_height: Height,
    pub op: GovernanceOp,
}

impl GovernanceProposal {
    pub fn digest(&self) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(b"GOV_PROPOSAL_V1:");
        hasher.update(self.proposal_id.to_le_bytes());
        hasher.update(self.proposer.to_le_bytes());
        hasher.update(self.created_height.to_le_bytes());
        match &self.op {
            GovernanceOp::UpdateSlashPolicy { policy } => {
                hasher.update([1u8]);
                hasher.update([match policy.mode {
                    SlashMode::Enforce => 1u8,
                    SlashMode::ObserveOnly => 2u8,
                }]);
                hasher.update(policy.equivocation_threshold.to_le_bytes());
                hasher.update(policy.min_active_validators.to_le_bytes());
                hasher.update(policy.cooldown_epochs.to_le_bytes());
            }
            GovernanceOp::UpdateMempoolFeeFloor { fee_floor } => {
                hasher.update([2u8]);
                hasher.update(fee_floor.to_le_bytes());
            }
            GovernanceOp::UpdateNetworkDosPolicy { policy } => {
                hasher.update([3u8]);
                hasher.update(policy.rpc_rate_limit_per_ip.to_le_bytes());
                hasher.update(policy.peer_ban_threshold.to_le_bytes());
            }
        }
        hasher.finalize().into()
    }
}

/// 治理投票（对提案支持/反对，含签名）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceVote {
    pub proposal_id: u64,
    pub proposal_height: Height,
    pub proposal_digest: Hash,
    pub voter_id: NodeId,
    pub support: bool,
    pub signature: Vec<u8>,
}

impl GovernanceVote {
    pub fn new(proposal: &GovernanceProposal, voter_id: NodeId, support: bool, signing_key: &SigningKey) -> Self {
        let proposal_digest = proposal.digest();
        let proposal_height = proposal.created_height;
        let message = Self::construct_message(
            proposal.proposal_id,
            proposal_height,
            &proposal_digest,
            support,
        );
        let signature = signing_key.sign(&message).to_bytes().to_vec();
        Self {
            proposal_id: proposal.proposal_id,
            proposal_height,
            proposal_digest,
            voter_id,
            support,
            signature,
        }
    }

    pub fn verify(&self, verifying_key: &VerifyingKey) -> BFTResult<()> {
        let message = Self::construct_message(
            self.proposal_id,
            self.proposal_height,
            &self.proposal_digest,
            self.support,
        );
        let signature = Signature::from_slice(&self.signature)
            .map_err(|e| BFTError::InvalidSignature(format!("Invalid governance vote signature format: {}", e)))?;
        verifying_key
            .verify(&message, &signature)
            .map_err(|e| BFTError::InvalidSignature(format!("Governance vote verification failed: {}", e)))?;
        Ok(())
    }

    fn construct_message(
        proposal_id: u64,
        proposal_height: Height,
        proposal_digest: &Hash,
        support: bool,
    ) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(b"GOV_VOTE_V1:");
        message.extend_from_slice(&proposal_id.to_le_bytes());
        message.extend_from_slice(&proposal_height.to_le_bytes());
        message.extend_from_slice(proposal_digest);
        message.push(if support { 1 } else { 0 });
        message
    }
}

impl SlashPolicy {
    pub fn validate(&self) -> BFTResult<()> {
        if self.equivocation_threshold == 0 {
            return Err(BFTError::Internal(
                "slash policy equivocation_threshold must be > 0".to_string(),
            ));
        }
        if self.min_active_validators < 1 {
            return Err(BFTError::Internal(
                "slash policy min_active_validators must be >= 1".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for SlashPolicy {
    fn default() -> Self {
        Self {
            mode: SlashMode::Enforce,
            equivocation_threshold: 1,
            min_active_validators: 2,
            cooldown_epochs: 0,
        }
    }
}

impl NetworkDosPolicy {
    pub fn validate(&self) -> BFTResult<()> {
        if self.rpc_rate_limit_per_ip == 0 {
            return Err(BFTError::Internal(
                "network dos policy rpc_rate_limit_per_ip must be > 0".to_string(),
            ));
        }
        if self.peer_ban_threshold > 0 {
            return Err(BFTError::Internal(
                "network dos policy peer_ban_threshold must be <= 0".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for NetworkDosPolicy {
    fn default() -> Self {
        Self {
            rpc_rate_limit_per_ip: 128,
            peer_ban_threshold: -3,
        }
    }
}

impl ValidatorSet {
    /// 创建自定义权重验证者集合
    pub fn new_weighted(validators: Vec<(NodeId, u64)>) -> BFTResult<Self> {
        if validators.is_empty() {
            return Err(BFTError::Internal(
                "validator set cannot be empty".to_string(),
            ));
        }

        let mut ids = Vec::with_capacity(validators.len());
        let mut weights = Vec::with_capacity(validators.len());
        let mut seen = HashSet::with_capacity(validators.len());
        let mut total_weight = 0u64;

        for (id, weight) in validators {
            if !seen.insert(id) {
                return Err(BFTError::Internal(format!(
                    "duplicate validator id in set: {}",
                    id
                )));
            }
            if weight == 0 {
                return Err(BFTError::Internal(format!(
                    "validator {} has zero voting weight",
                    id
                )));
            }
            total_weight = total_weight
                .checked_add(weight)
                .ok_or_else(|| BFTError::Internal("validator total weight overflow".to_string()))?;
            ids.push(id);
            weights.push(weight);
        }

        Ok(Self {
            validators: ids,
            weights,
            total_weight,
        })
    }

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

    /// 获取验证者投票权重
    pub fn weight_of(&self, node_id: NodeId) -> Option<u64> {
        self.get_index(node_id).map(|idx| self.weights[idx])
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
    fn test_weighted_validator_set_quorum() {
        let validators = ValidatorSet::new_weighted(vec![(0, 5), (1, 3), (2, 2)]).unwrap();
        assert_eq!(validators.total_weight, 10);
        assert_eq!(validators.quorum_size(), 7); // ceil(20/3)=7
        assert_eq!(validators.weight_of(0), Some(5));
        assert_eq!(validators.weight_of(2), Some(2));
        assert_eq!(validators.weight_of(9), None);
    }

    #[test]
    fn test_weighted_validator_set_rejects_invalid_config() {
        let dup = ValidatorSet::new_weighted(vec![(0, 1), (0, 2)]);
        assert!(dup.is_err());

        let zero = ValidatorSet::new_weighted(vec![(0, 0), (1, 1)]);
        assert!(zero.is_err());

        let empty = ValidatorSet::new_weighted(vec![]);
        assert!(empty.is_err());
    }

    #[test]
    fn test_proposal_hash_deterministic() {
        let proposal = BFTProposal {
            epoch_id: 1,
            view: 2,
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

    #[test]
    fn test_slash_policy_validation() {
        let default_policy = SlashPolicy::default();
        assert!(default_policy.validate().is_ok());
        assert_eq!(default_policy.mode, SlashMode::Enforce);
        assert_eq!(default_policy.equivocation_threshold, 1);
        assert_eq!(default_policy.min_active_validators, 2);
        assert_eq!(default_policy.cooldown_epochs, 0);

        let bad_threshold = SlashPolicy {
            mode: SlashMode::Enforce,
            equivocation_threshold: 0,
            min_active_validators: 2,
            cooldown_epochs: 0,
        };
        assert!(bad_threshold.validate().is_err());

        let bad_floor = SlashPolicy {
            mode: SlashMode::ObserveOnly,
            equivocation_threshold: 1,
            min_active_validators: 0,
            cooldown_epochs: 0,
        };
        assert!(bad_floor.validate().is_err());
    }

    #[test]
    fn test_governance_op_update_slash_policy_shape() {
        let op = GovernanceOp::UpdateSlashPolicy {
            policy: SlashPolicy {
                mode: SlashMode::ObserveOnly,
                equivocation_threshold: 3,
                min_active_validators: 2,
                cooldown_epochs: 7,
            },
        };

        let encoded = serde_json::to_string(&op).expect("serialize governance op");
        let decoded: GovernanceOp =
            serde_json::from_str(&encoded).expect("deserialize governance op");
        assert_eq!(decoded, op);
    }

    #[test]
    fn test_governance_vote_signature_roundtrip() {
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        let proposal = GovernanceProposal {
            proposal_id: 42,
            proposer: 1,
            created_height: 9,
            op: GovernanceOp::UpdateSlashPolicy {
                policy: SlashPolicy {
                    mode: SlashMode::ObserveOnly,
                    equivocation_threshold: 2,
                    min_active_validators: 2,
                    cooldown_epochs: 7,
                },
            },
        };
        let vote = GovernanceVote::new(&proposal, 1, true, &signing_key);
        assert!(vote.verify(&signing_key.verifying_key()).is_ok());

        let wrong = SigningKey::generate(&mut OsRng);
        assert!(vote.verify(&wrong.verifying_key()).is_err());
    }
}

