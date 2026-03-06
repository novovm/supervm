// Phase 4.2: BFT Core Types
//
// 定义 BFT 共识的核心数据类型

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
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
                write!(
                    f,
                    "Equivocation detected from node {} at height {}",
                    voter, height
                )
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
        use sha2::{Digest, Sha256};
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

/// SVM2026 费率拆分参数（单位：basis points）。
/// 语义对齐 `web30/core/src/mainnet_token.rs::FeeSplit`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeeSplit {
    /// gas base burn 比例。
    pub gas_base_burn_bp: u16,
    /// gas 路由到 node reward 的比例。
    pub gas_to_node_bp: u16,
    /// service fee burn 比例。
    pub service_burn_bp: u16,
    /// service 路由到 provider 的比例。
    pub service_to_provider_bp: u16,
}

/// Fee 路由结果（单位同输入 amount）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeeRoutingOutcome {
    pub provider_amount: u64,
    pub treasury_amount: u64,
    pub burn_amount: u64,
}

/// Token 经济参数（I-TOKEN 最小主链路）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenEconomicsPolicy {
    /// 最大供应量（硬上限）。
    pub max_supply: u64,
    /// 锁仓可铸额度（mint 只能从该额度中释放）。
    pub locked_supply: u64,
    /// 费率拆分参数（treasury=剩余）。
    pub fee_split: FeeSplit,
}

/// AMM 治理参数（I-GOV-02 子域）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmmGovernanceParams {
    /// 交换费率（bps，示例 30=0.3%）。
    pub swap_fee_bp: u16,
    /// LP 分成比例（bps，treasury=剩余）。
    pub lp_fee_share_bp: u16,
}

/// CDP 治理参数（I-GOV-02 子域）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CdpGovernanceParams {
    /// 最低抵押率（bps，15000=150%）。
    pub min_collateral_ratio_bp: u16,
    /// 清算阈值（bps）。
    pub liquidation_threshold_bp: u16,
    /// 清算罚金（bps）。
    pub liquidation_penalty_bp: u16,
    /// 稳定费（年化 bps）。
    pub stability_fee_bp: u16,
    /// 最大杠杆（x100，300=3x）。
    pub max_leverage_x100: u16,
}

/// Bond 治理参数（I-GOV-02 子域）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BondGovernanceParams {
    /// 默认票息（bps）。
    pub coupon_rate_bp: u16,
    /// 最大期限（天）。
    pub max_maturity_days: u16,
    /// 最低发行价（bps，10500=105% NAV）。
    pub min_issue_price_bp: u16,
}

/// 外汇储备治理参数（I-GOV-02 子域）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReserveGovernanceParams {
    /// 最低储备率（bps，5000=50%）。
    pub min_reserve_ratio_bp: u16,
    /// NAV 赎回手续费（bps）。
    pub redemption_fee_bp: u16,
}

/// NAV 治理参数（I-GOV-02 子域）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NavGovernanceParams {
    /// NAV 赎回结算延迟（epoch）。
    pub settlement_delay_epochs: u16,
    /// 每日最大赎回额度（bps）。
    pub max_daily_redemption_bp: u16,
}

/// 回购治理参数（I-GOV-02 子域）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuybackGovernanceParams {
    /// 回购触发折价（bps，相对 NAV）。
    pub trigger_discount_bp: u16,
    /// 每 epoch 最大可用国库预算。
    pub max_treasury_budget_per_epoch: u64,
    /// 回购后销毁比例（bps）。
    pub burn_share_bp: u16,
}

/// 完整经济治理参数族（AMM/CDP/Bond/Reserve/NAV/Buyback）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketGovernancePolicy {
    pub amm: AmmGovernanceParams,
    pub cdp: CdpGovernanceParams,
    pub bond: BondGovernanceParams,
    pub reserve: ReserveGovernanceParams,
    pub nav: NavGovernanceParams,
    pub buyback: BuybackGovernanceParams,
}

/// 治理访问控制策略（链上模型）。
///
/// 语义：
/// - proposer/executor 均采用委员会+阈值（N-of-M）；
/// - timelock 以 epoch 高度差计算；
/// - 提案与执行都要求参与者处于 active validator 集合。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceAccessPolicy {
    /// 可发起提案的委员会成员。
    pub proposer_committee: Vec<NodeId>,
    /// 提案发起阈值（N-of-M）。
    pub proposer_threshold: u32,
    /// 可执行提案的委员会成员。
    pub executor_committee: Vec<NodeId>,
    /// 提案执行阈值（N-of-M）。
    pub executor_threshold: u32,
    /// 执行时间锁（单位：epoch），0 表示无时间锁。
    pub timelock_epochs: u64,
}

/// SVM2026 九席位治理席位。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum GovernanceCouncilSeat {
    Founder,
    TopHolder(u8),
    Team(u8),
    Independent,
}

impl GovernanceCouncilSeat {
    pub fn validate_shape(&self) -> BFTResult<()> {
        match self {
            GovernanceCouncilSeat::TopHolder(idx) if *idx > 4 => Err(BFTError::InvalidProposal(
                "council seat TopHolder index out of range (expected 0..=4)".to_string(),
            )),
            GovernanceCouncilSeat::Team(idx) if *idx > 1 => Err(BFTError::InvalidProposal(
                "council seat Team index out of range (expected 0..=1)".to_string(),
            )),
            _ => Ok(()),
        }
    }

    pub fn voting_weight_bp(&self) -> u16 {
        match self {
            GovernanceCouncilSeat::Founder => 3500,
            GovernanceCouncilSeat::TopHolder(_) => 1000,
            GovernanceCouncilSeat::Team(_) => 500,
            GovernanceCouncilSeat::Independent => 500,
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            GovernanceCouncilSeat::Founder => "Founder",
            GovernanceCouncilSeat::TopHolder(_) => "TopHolder",
            GovernanceCouncilSeat::Team(_) => "Team",
            GovernanceCouncilSeat::Independent => "Independent",
        }
    }
}

/// 九席位治理成员绑定（seat -> node_id）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceCouncilMember {
    pub seat: GovernanceCouncilSeat,
    pub node_id: NodeId,
}

/// 治理提案分类（映射 SVM2026 ProposalType 语义）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GovernanceProposalClass {
    ParameterChange,
    TreasurySpend,
    ProtocolUpgrade,
    EmergencyFreeze,
}

/// 九席位治理规则配置（I-GOV-01/I-GOV-02）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceCouncilPolicy {
    /// 是否启用九席位治理权重规则。
    pub enabled: bool,
    /// 九席位成员绑定（严格要求 9 个 seat 全覆盖）。
    pub members: Vec<GovernanceCouncilMember>,
    /// 参数变更通过阈值（bps）。
    pub parameter_change_threshold_bp: u16,
    /// 国库支出通过阈值（bps）。
    pub treasury_spend_threshold_bp: u16,
    /// 协议升级通过阈值（bps）。
    pub protocol_upgrade_threshold_bp: u16,
    /// 紧急冻结通过阈值（bps）。
    pub emergency_freeze_threshold_bp: u16,
    /// 紧急冻结最小席位类别数。
    pub emergency_min_categories: u8,
}

impl GovernanceCouncilPolicy {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            members: Vec::new(),
            parameter_change_threshold_bp: 5000,
            treasury_spend_threshold_bp: 6600,
            protocol_upgrade_threshold_bp: 7500,
            emergency_freeze_threshold_bp: 5000,
            emergency_min_categories: 3,
        }
    }

    fn validate_threshold(name: &str, value: u16) -> BFTResult<()> {
        if value == 0 || value >= 10_000 {
            return Err(BFTError::InvalidProposal(format!(
                "{} must be within [1, 9999] basis points",
                name
            )));
        }
        Ok(())
    }

    pub fn threshold_for(&self, class: GovernanceProposalClass) -> u16 {
        match class {
            GovernanceProposalClass::ParameterChange => self.parameter_change_threshold_bp,
            GovernanceProposalClass::TreasurySpend => self.treasury_spend_threshold_bp,
            GovernanceProposalClass::ProtocolUpgrade => self.protocol_upgrade_threshold_bp,
            GovernanceProposalClass::EmergencyFreeze => self.emergency_freeze_threshold_bp,
        }
    }

    pub fn member_weight_map(&self) -> HashMap<NodeId, (u16, &'static str)> {
        let mut out = HashMap::with_capacity(self.members.len());
        for member in &self.members {
            out.insert(
                member.node_id,
                (member.seat.voting_weight_bp(), member.seat.category()),
            );
        }
        out
    }

    pub fn validate(&self) -> BFTResult<()> {
        Self::validate_threshold(
            "council parameter_change_threshold_bp",
            self.parameter_change_threshold_bp,
        )?;
        Self::validate_threshold(
            "council treasury_spend_threshold_bp",
            self.treasury_spend_threshold_bp,
        )?;
        Self::validate_threshold(
            "council protocol_upgrade_threshold_bp",
            self.protocol_upgrade_threshold_bp,
        )?;
        Self::validate_threshold(
            "council emergency_freeze_threshold_bp",
            self.emergency_freeze_threshold_bp,
        )?;
        if self.emergency_min_categories == 0 || self.emergency_min_categories > 4 {
            return Err(BFTError::InvalidProposal(
                "council emergency_min_categories must be within [1, 4]".to_string(),
            ));
        }

        if !self.enabled {
            return Ok(());
        }

        if self.members.len() != 9 {
            return Err(BFTError::InvalidProposal(format!(
                "council enabled requires exactly 9 members, got {}",
                self.members.len()
            )));
        }

        let mut seat_set = HashSet::with_capacity(self.members.len());
        let mut node_set = HashSet::with_capacity(self.members.len());
        let mut weight_sum = 0u16;
        for member in &self.members {
            member.seat.validate_shape()?;
            if !seat_set.insert(member.seat) {
                return Err(BFTError::InvalidProposal(format!(
                    "council seat {:?} is duplicated",
                    member.seat
                )));
            }
            if !node_set.insert(member.node_id) {
                return Err(BFTError::InvalidProposal(format!(
                    "council node {} is duplicated across seats",
                    member.node_id
                )));
            }
            weight_sum = weight_sum
                .checked_add(member.seat.voting_weight_bp())
                .ok_or_else(|| {
                    BFTError::InvalidProposal("council voting weight overflow".to_string())
                })?;
        }
        if weight_sum != 10_000 {
            return Err(BFTError::InvalidProposal(format!(
                "council voting weight sum must be 10000 bps, got {}",
                weight_sum
            )));
        }

        let required = [
            GovernanceCouncilSeat::Founder,
            GovernanceCouncilSeat::TopHolder(0),
            GovernanceCouncilSeat::TopHolder(1),
            GovernanceCouncilSeat::TopHolder(2),
            GovernanceCouncilSeat::TopHolder(3),
            GovernanceCouncilSeat::TopHolder(4),
            GovernanceCouncilSeat::Team(0),
            GovernanceCouncilSeat::Team(1),
            GovernanceCouncilSeat::Independent,
        ];
        for seat in required {
            if !seat_set.contains(&seat) {
                return Err(BFTError::InvalidProposal(format!(
                    "council enabled is missing required seat: {:?}",
                    seat
                )));
            }
        }

        Ok(())
    }
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
    /// 更新 token 经济参数（第四类参数）。
    UpdateTokenEconomicsPolicy { policy: TokenEconomicsPolicy },
    /// 更新完整经济治理参数族（I-GOV-02）。
    UpdateMarketGovernancePolicy { policy: MarketGovernancePolicy },
    /// 更新治理访问控制策略（委员会阈值 + 时间锁）。
    UpdateGovernanceAccessPolicy { policy: GovernanceAccessPolicy },
    /// 更新治理九席位规则（权重/阈值）。
    UpdateGovernanceCouncilPolicy { policy: GovernanceCouncilPolicy },
    /// 国库支出（由治理执行，资金来源为 treasury 主账户）。
    TreasurySpend {
        to: NodeId,
        amount: u64,
        reason: String,
    },
}

impl GovernanceOp {
    pub fn proposal_class(&self) -> GovernanceProposalClass {
        match self {
            GovernanceOp::TreasurySpend { .. } => GovernanceProposalClass::TreasurySpend,
            GovernanceOp::UpdateGovernanceAccessPolicy { .. }
            | GovernanceOp::UpdateGovernanceCouncilPolicy { .. } => {
                GovernanceProposalClass::ProtocolUpgrade
            }
            _ => GovernanceProposalClass::ParameterChange,
        }
    }
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
            GovernanceOp::UpdateTokenEconomicsPolicy { policy } => {
                hasher.update([4u8]);
                hasher.update(policy.max_supply.to_le_bytes());
                hasher.update(policy.locked_supply.to_le_bytes());
                hasher.update(policy.fee_split.gas_base_burn_bp.to_le_bytes());
                hasher.update(policy.fee_split.gas_to_node_bp.to_le_bytes());
                hasher.update(policy.fee_split.service_burn_bp.to_le_bytes());
                hasher.update(policy.fee_split.service_to_provider_bp.to_le_bytes());
            }
            GovernanceOp::UpdateMarketGovernancePolicy { policy } => {
                hasher.update([5u8]);
                hasher.update(policy.amm.swap_fee_bp.to_le_bytes());
                hasher.update(policy.amm.lp_fee_share_bp.to_le_bytes());
                hasher.update(policy.cdp.min_collateral_ratio_bp.to_le_bytes());
                hasher.update(policy.cdp.liquidation_threshold_bp.to_le_bytes());
                hasher.update(policy.cdp.liquidation_penalty_bp.to_le_bytes());
                hasher.update(policy.cdp.stability_fee_bp.to_le_bytes());
                hasher.update(policy.cdp.max_leverage_x100.to_le_bytes());
                hasher.update(policy.bond.coupon_rate_bp.to_le_bytes());
                hasher.update(policy.bond.max_maturity_days.to_le_bytes());
                hasher.update(policy.bond.min_issue_price_bp.to_le_bytes());
                hasher.update(policy.reserve.min_reserve_ratio_bp.to_le_bytes());
                hasher.update(policy.reserve.redemption_fee_bp.to_le_bytes());
                hasher.update(policy.nav.settlement_delay_epochs.to_le_bytes());
                hasher.update(policy.nav.max_daily_redemption_bp.to_le_bytes());
                hasher.update(policy.buyback.trigger_discount_bp.to_le_bytes());
                hasher.update(policy.buyback.max_treasury_budget_per_epoch.to_le_bytes());
                hasher.update(policy.buyback.burn_share_bp.to_le_bytes());
            }
            GovernanceOp::UpdateGovernanceAccessPolicy { policy } => {
                hasher.update([6u8]);
                hasher.update(policy.timelock_epochs.to_le_bytes());
                hasher.update(policy.proposer_threshold.to_le_bytes());
                hasher.update(policy.executor_threshold.to_le_bytes());
                hasher.update((policy.proposer_committee.len() as u32).to_le_bytes());
                for id in &policy.proposer_committee {
                    hasher.update(id.to_le_bytes());
                }
                hasher.update((policy.executor_committee.len() as u32).to_le_bytes());
                for id in &policy.executor_committee {
                    hasher.update(id.to_le_bytes());
                }
            }
            GovernanceOp::UpdateGovernanceCouncilPolicy { policy } => {
                hasher.update([7u8]);
                hasher.update([if policy.enabled { 1u8 } else { 0u8 }]);
                hasher.update(policy.parameter_change_threshold_bp.to_le_bytes());
                hasher.update(policy.treasury_spend_threshold_bp.to_le_bytes());
                hasher.update(policy.protocol_upgrade_threshold_bp.to_le_bytes());
                hasher.update(policy.emergency_freeze_threshold_bp.to_le_bytes());
                hasher.update(policy.emergency_min_categories.to_le_bytes());
                hasher.update((policy.members.len() as u32).to_le_bytes());
                for member in &policy.members {
                    match member.seat {
                        GovernanceCouncilSeat::Founder => {
                            hasher.update([1u8, 0u8]);
                        }
                        GovernanceCouncilSeat::TopHolder(idx) => {
                            hasher.update([2u8, idx]);
                        }
                        GovernanceCouncilSeat::Team(idx) => {
                            hasher.update([3u8, idx]);
                        }
                        GovernanceCouncilSeat::Independent => {
                            hasher.update([4u8, 0u8]);
                        }
                    }
                    hasher.update(member.node_id.to_le_bytes());
                }
            }
            GovernanceOp::TreasurySpend { to, amount, reason } => {
                hasher.update([8u8]);
                hasher.update(to.to_le_bytes());
                hasher.update(amount.to_le_bytes());
                hasher.update(reason.as_bytes());
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
    pub fn new(
        proposal: &GovernanceProposal,
        voter_id: NodeId,
        support: bool,
        signing_key: &SigningKey,
    ) -> Self {
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
        let signature = Signature::from_slice(&self.signature).map_err(|e| {
            BFTError::InvalidSignature(format!("Invalid governance vote signature format: {}", e))
        })?;
        verifying_key.verify(&message, &signature).map_err(|e| {
            BFTError::InvalidSignature(format!("Governance vote verification failed: {}", e))
        })?;
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

/// 链上治理审计事件（共识状态机内索引，按 seq 递增）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceChainAuditEvent {
    /// 审计序号（单调递增）。
    pub seq: u64,
    /// 记录该事件时的链高度。
    pub height: Height,
    /// 关联提案 ID；`0` 表示非具体提案事件（例如 staged hook）。
    pub proposal_id: u64,
    /// 事件动作：`stage|submit|execute`。
    pub action: String,
    /// 触发节点（可空）。
    pub actor: Option<NodeId>,
    /// 事件结果：`staged_only|accepted|applied`。
    pub outcome: String,
    /// 简要明细（用于审计检索）。
    pub detail: String,
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

impl TokenEconomicsPolicy {
    pub fn validate(&self) -> BFTResult<()> {
        if self.max_supply == 0 {
            return Err(BFTError::Internal(
                "token economics max_supply must be > 0".to_string(),
            ));
        }
        if self.locked_supply == 0 {
            return Err(BFTError::Internal(
                "token economics locked_supply must be > 0".to_string(),
            ));
        }
        if self.locked_supply > self.max_supply {
            return Err(BFTError::Internal(
                "token economics locked_supply cannot exceed max_supply".to_string(),
            ));
        }
        let gas_total = u32::from(self.fee_split.gas_base_burn_bp)
            .saturating_add(u32::from(self.fee_split.gas_to_node_bp));
        if gas_total > 10_000 {
            return Err(BFTError::Internal(format!(
                "gas fee split exceeds 100%: {} bp",
                gas_total
            )));
        }
        let service_total = u32::from(self.fee_split.service_burn_bp)
            .saturating_add(u32::from(self.fee_split.service_to_provider_bp));
        if service_total > 10_000 {
            return Err(BFTError::Internal(format!(
                "service fee split exceeds 100%: {} bp",
                service_total
            )));
        }
        Ok(())
    }
}

impl MarketGovernancePolicy {
    fn ensure_bp_nonzero(name: &str, value: u16) -> BFTResult<()> {
        if value == 0 {
            return Err(BFTError::Internal(format!(
                "{} must be > 0 basis points",
                name
            )));
        }
        Ok(())
    }

    fn ensure_bp_lte_100pct(name: &str, value: u16) -> BFTResult<()> {
        if value > 10_000 {
            return Err(BFTError::Internal(format!(
                "{} must be <= 10000 basis points",
                name
            )));
        }
        Ok(())
    }

    pub fn validate(&self) -> BFTResult<()> {
        Self::ensure_bp_nonzero("market governance amm.swap_fee_bp", self.amm.swap_fee_bp)?;
        Self::ensure_bp_lte_100pct(
            "market governance amm.lp_fee_share_bp",
            self.amm.lp_fee_share_bp,
        )?;

        if !(10_000..=50_000).contains(&self.cdp.min_collateral_ratio_bp) {
            return Err(BFTError::Internal(
                "market governance cdp.min_collateral_ratio_bp must be within [10000, 50000]"
                    .to_string(),
            ));
        }
        if !(10_000..=50_000).contains(&self.cdp.liquidation_threshold_bp) {
            return Err(BFTError::Internal(
                "market governance cdp.liquidation_threshold_bp must be within [10000, 50000]"
                    .to_string(),
            ));
        }
        if self.cdp.liquidation_threshold_bp > self.cdp.min_collateral_ratio_bp {
            return Err(BFTError::Internal(
                "market governance cdp.liquidation_threshold_bp cannot exceed min_collateral_ratio_bp".to_string(),
            ));
        }
        Self::ensure_bp_lte_100pct(
            "market governance cdp.liquidation_penalty_bp",
            self.cdp.liquidation_penalty_bp,
        )?;
        Self::ensure_bp_lte_100pct(
            "market governance cdp.stability_fee_bp",
            self.cdp.stability_fee_bp,
        )?;
        if !(100..=2000).contains(&self.cdp.max_leverage_x100) {
            return Err(BFTError::Internal(
                "market governance cdp.max_leverage_x100 must be within [100, 2000]".to_string(),
            ));
        }

        Self::ensure_bp_lte_100pct(
            "market governance bond.coupon_rate_bp",
            self.bond.coupon_rate_bp,
        )?;
        if !(30..=3650).contains(&self.bond.max_maturity_days) {
            return Err(BFTError::Internal(
                "market governance bond.max_maturity_days must be within [30, 3650]".to_string(),
            ));
        }
        if !(10_000..=20_000).contains(&self.bond.min_issue_price_bp) {
            return Err(BFTError::Internal(
                "market governance bond.min_issue_price_bp must be within [10000, 20000]"
                    .to_string(),
            ));
        }

        if !(1_000..=10_000).contains(&self.reserve.min_reserve_ratio_bp) {
            return Err(BFTError::Internal(
                "market governance reserve.min_reserve_ratio_bp must be within [1000, 10000]"
                    .to_string(),
            ));
        }
        Self::ensure_bp_lte_100pct(
            "market governance reserve.redemption_fee_bp",
            self.reserve.redemption_fee_bp,
        )?;

        if self.nav.settlement_delay_epochs == 0 {
            return Err(BFTError::Internal(
                "market governance nav.settlement_delay_epochs must be > 0".to_string(),
            ));
        }
        Self::ensure_bp_lte_100pct(
            "market governance nav.max_daily_redemption_bp",
            self.nav.max_daily_redemption_bp,
        )?;

        Self::ensure_bp_lte_100pct(
            "market governance buyback.trigger_discount_bp",
            self.buyback.trigger_discount_bp,
        )?;
        if self.buyback.max_treasury_budget_per_epoch == 0 {
            return Err(BFTError::Internal(
                "market governance buyback.max_treasury_budget_per_epoch must be > 0".to_string(),
            ));
        }
        Self::ensure_bp_lte_100pct(
            "market governance buyback.burn_share_bp",
            self.buyback.burn_share_bp,
        )?;

        Ok(())
    }
}

impl GovernanceAccessPolicy {
    pub fn validate(&self) -> BFTResult<()> {
        if self.proposer_committee.is_empty() {
            return Err(BFTError::Internal(
                "governance access proposer_committee cannot be empty".to_string(),
            ));
        }
        if self.executor_committee.is_empty() {
            return Err(BFTError::Internal(
                "governance access executor_committee cannot be empty".to_string(),
            ));
        }
        if self.proposer_threshold == 0 {
            return Err(BFTError::Internal(
                "governance access proposer_threshold must be > 0".to_string(),
            ));
        }
        if self.executor_threshold == 0 {
            return Err(BFTError::Internal(
                "governance access executor_threshold must be > 0".to_string(),
            ));
        }
        if self.proposer_threshold as usize > self.proposer_committee.len() {
            return Err(BFTError::Internal(format!(
                "governance access proposer_threshold {} exceeds proposer committee size {}",
                self.proposer_threshold,
                self.proposer_committee.len()
            )));
        }
        if self.executor_threshold as usize > self.executor_committee.len() {
            return Err(BFTError::Internal(format!(
                "governance access executor_threshold {} exceeds executor committee size {}",
                self.executor_threshold,
                self.executor_committee.len()
            )));
        }

        let proposer_unique: HashSet<NodeId> = self.proposer_committee.iter().copied().collect();
        if proposer_unique.len() != self.proposer_committee.len() {
            return Err(BFTError::Internal(
                "governance access proposer_committee contains duplicate members".to_string(),
            ));
        }
        let executor_unique: HashSet<NodeId> = self.executor_committee.iter().copied().collect();
        if executor_unique.len() != self.executor_committee.len() {
            return Err(BFTError::Internal(
                "governance access executor_committee contains duplicate members".to_string(),
            ));
        }

        Ok(())
    }

    pub fn for_validators(validators: &[NodeId]) -> BFTResult<Self> {
        if validators.is_empty() {
            return Err(BFTError::Internal(
                "governance access requires at least one validator".to_string(),
            ));
        }
        let policy = Self {
            proposer_committee: validators.to_vec(),
            proposer_threshold: 1,
            executor_committee: validators.to_vec(),
            executor_threshold: 1,
            timelock_epochs: 0,
        };
        policy.validate()?;
        Ok(policy)
    }
}

impl Default for TokenEconomicsPolicy {
    fn default() -> Self {
        Self {
            max_supply: 1_000_000_000,
            locked_supply: 600_000_000,
            fee_split: FeeSplit {
                // 对齐 SVM2026 示例：gas burn=20%, gas->node=30%, treasury=50%
                gas_base_burn_bp: 2_000,
                gas_to_node_bp: 3_000,
                // 对齐 SVM2026 示例：service burn=10%, service->provider=40%, treasury=50%
                service_burn_bp: 1_000,
                service_to_provider_bp: 4_000,
            },
        }
    }
}

impl Default for MarketGovernancePolicy {
    fn default() -> Self {
        Self {
            amm: AmmGovernanceParams {
                swap_fee_bp: 30,
                lp_fee_share_bp: 8_333,
            },
            cdp: CdpGovernanceParams {
                min_collateral_ratio_bp: 15_000,
                liquidation_threshold_bp: 12_500,
                liquidation_penalty_bp: 1_000,
                stability_fee_bp: 200,
                max_leverage_x100: 300,
            },
            bond: BondGovernanceParams {
                coupon_rate_bp: 500,
                max_maturity_days: 365,
                min_issue_price_bp: 10_500,
            },
            reserve: ReserveGovernanceParams {
                min_reserve_ratio_bp: 5_000,
                redemption_fee_bp: 50,
            },
            nav: NavGovernanceParams {
                settlement_delay_epochs: 7,
                max_daily_redemption_bp: 1_000,
            },
            buyback: BuybackGovernanceParams {
                trigger_discount_bp: 500,
                max_treasury_budget_per_epoch: 100_000,
                burn_share_bp: 5_000,
            },
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
    fn test_governance_op_update_token_economics_policy_shape() {
        let op = GovernanceOp::UpdateTokenEconomicsPolicy {
            policy: TokenEconomicsPolicy {
                max_supply: 1_000_000,
                locked_supply: 300_000,
                fee_split: FeeSplit {
                    gas_base_burn_bp: 2_000,
                    gas_to_node_bp: 3_000,
                    service_burn_bp: 1_000,
                    service_to_provider_bp: 4_000,
                },
            },
        };

        let encoded = serde_json::to_string(&op).expect("serialize governance op");
        let decoded: GovernanceOp =
            serde_json::from_str(&encoded).expect("deserialize governance op");
        assert_eq!(decoded, op);
    }

    #[test]
    fn test_governance_op_update_market_governance_policy_shape() {
        let op = GovernanceOp::UpdateMarketGovernancePolicy {
            policy: MarketGovernancePolicy::default(),
        };

        let encoded = serde_json::to_string(&op).expect("serialize governance op");
        let decoded: GovernanceOp =
            serde_json::from_str(&encoded).expect("deserialize governance op");
        assert_eq!(decoded, op);
    }

    #[test]
    fn test_governance_op_treasury_spend_shape() {
        let op = GovernanceOp::TreasurySpend {
            to: 42,
            amount: 777,
            reason: "ecosystem_grant".to_string(),
        };

        let encoded = serde_json::to_string(&op).expect("serialize governance op");
        let decoded: GovernanceOp =
            serde_json::from_str(&encoded).expect("deserialize governance op");
        assert_eq!(decoded, op);
    }

    #[test]
    fn test_governance_op_update_governance_access_policy_shape() {
        let op = GovernanceOp::UpdateGovernanceAccessPolicy {
            policy: GovernanceAccessPolicy {
                proposer_committee: vec![0, 1],
                proposer_threshold: 2,
                executor_committee: vec![1, 2],
                executor_threshold: 2,
                timelock_epochs: 3,
            },
        };

        let encoded = serde_json::to_string(&op).expect("serialize governance op");
        let decoded: GovernanceOp =
            serde_json::from_str(&encoded).expect("deserialize governance op");
        assert_eq!(decoded, op);
    }

    #[test]
    fn test_governance_op_update_governance_council_policy_shape() {
        let op = GovernanceOp::UpdateGovernanceCouncilPolicy {
            policy: GovernanceCouncilPolicy {
                enabled: true,
                members: vec![
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::Founder,
                        node_id: 0,
                    },
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::TopHolder(0),
                        node_id: 1,
                    },
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::TopHolder(1),
                        node_id: 2,
                    },
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::TopHolder(2),
                        node_id: 3,
                    },
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::TopHolder(3),
                        node_id: 4,
                    },
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::TopHolder(4),
                        node_id: 5,
                    },
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::Team(0),
                        node_id: 6,
                    },
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::Team(1),
                        node_id: 7,
                    },
                    GovernanceCouncilMember {
                        seat: GovernanceCouncilSeat::Independent,
                        node_id: 8,
                    },
                ],
                parameter_change_threshold_bp: 5000,
                treasury_spend_threshold_bp: 6600,
                protocol_upgrade_threshold_bp: 7500,
                emergency_freeze_threshold_bp: 5000,
                emergency_min_categories: 3,
            },
        };

        let encoded = serde_json::to_string(&op).expect("serialize governance op");
        let decoded: GovernanceOp =
            serde_json::from_str(&encoded).expect("deserialize governance op");
        assert_eq!(decoded, op);
    }

    #[test]
    fn test_token_economics_policy_validation() {
        assert!(TokenEconomicsPolicy::default().validate().is_ok());

        let bad_total = TokenEconomicsPolicy {
            fee_split: FeeSplit {
                gas_base_burn_bp: 7_000,
                gas_to_node_bp: 3_001,
                service_burn_bp: 1_000,
                service_to_provider_bp: 4_000,
            },
            ..TokenEconomicsPolicy::default()
        };
        assert!(bad_total.validate().is_err());

        let bad_supply = TokenEconomicsPolicy {
            max_supply: 100,
            locked_supply: 101,
            ..TokenEconomicsPolicy::default()
        };
        assert!(bad_supply.validate().is_err());
    }

    #[test]
    fn test_market_governance_policy_validation() {
        assert!(MarketGovernancePolicy::default().validate().is_ok());

        let mut bad_cdp = MarketGovernancePolicy::default();
        bad_cdp.cdp.liquidation_threshold_bp = 16_000;
        bad_cdp.cdp.min_collateral_ratio_bp = 15_000;
        assert!(bad_cdp.validate().is_err());

        let mut bad_buyback = MarketGovernancePolicy::default();
        bad_buyback.buyback.burn_share_bp = 10_001;
        assert!(bad_buyback.validate().is_err());

        let mut bad_nav = MarketGovernancePolicy::default();
        bad_nav.nav.settlement_delay_epochs = 0;
        assert!(bad_nav.validate().is_err());
    }

    #[test]
    fn test_governance_access_policy_validation() {
        let ok = GovernanceAccessPolicy {
            proposer_committee: vec![0, 1],
            proposer_threshold: 2,
            executor_committee: vec![1, 2],
            executor_threshold: 1,
            timelock_epochs: 2,
        };
        assert!(ok.validate().is_ok());

        let bad_empty = GovernanceAccessPolicy {
            proposer_committee: vec![],
            proposer_threshold: 1,
            executor_committee: vec![1],
            executor_threshold: 1,
            timelock_epochs: 0,
        };
        assert!(bad_empty.validate().is_err());

        let bad_threshold = GovernanceAccessPolicy {
            proposer_committee: vec![0],
            proposer_threshold: 2,
            executor_committee: vec![1],
            executor_threshold: 1,
            timelock_epochs: 0,
        };
        assert!(bad_threshold.validate().is_err());
    }

    #[test]
    fn test_governance_council_policy_validation() {
        let disabled = GovernanceCouncilPolicy::disabled();
        assert!(disabled.validate().is_ok());

        let enabled = GovernanceCouncilPolicy {
            enabled: true,
            members: vec![
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::Founder,
                    node_id: 0,
                },
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::TopHolder(0),
                    node_id: 1,
                },
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::TopHolder(1),
                    node_id: 2,
                },
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::TopHolder(2),
                    node_id: 3,
                },
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::TopHolder(3),
                    node_id: 4,
                },
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::TopHolder(4),
                    node_id: 5,
                },
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::Team(0),
                    node_id: 6,
                },
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::Team(1),
                    node_id: 7,
                },
                GovernanceCouncilMember {
                    seat: GovernanceCouncilSeat::Independent,
                    node_id: 8,
                },
            ],
            parameter_change_threshold_bp: 5000,
            treasury_spend_threshold_bp: 6600,
            protocol_upgrade_threshold_bp: 7500,
            emergency_freeze_threshold_bp: 5000,
            emergency_min_categories: 3,
        };
        assert!(enabled.validate().is_ok());

        let mut bad_duplicate_seat = enabled.clone();
        bad_duplicate_seat.members[8].seat = GovernanceCouncilSeat::TopHolder(4);
        assert!(bad_duplicate_seat.validate().is_err());

        let mut bad_member_count = enabled.clone();
        bad_member_count.members.pop();
        assert!(bad_member_count.validate().is_err());

        let mut bad_threshold = enabled;
        bad_threshold.protocol_upgrade_threshold_bp = 10000;
        assert!(bad_threshold.validate().is_err());
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
