// Phase 4.2: HotStuff-2 Protocol Implementation
//
// 简化版 HotStuff-2 协议（3-chain rule）
// Propose → Vote → PreCommit → Commit

use crate::account_index::UnifiedAccountIndex;
use crate::epoch::Epoch;
use crate::governance_verifier::{
    Ed25519GovernanceVoteVerifier, GovernanceVoteVerificationInput,
    GovernanceVoteVerificationReport, GovernanceVoteVerifier, GovernanceVoteVerifierScheme,
    GOVERNANCE_VOTE_VERIFY_BATCH_MIN,
};
use crate::market_engine::{Web30MarketEngine, Web30MarketEngineSnapshot};
use crate::quorum_cert::{QuorumCertificate, Vote};
use crate::token_runtime::Web30TokenRuntime;
use crate::types::{
    BFTError, BFTProposal, BFTResult, FeeRoutingOutcome, GovernanceAccessPolicy,
    GovernanceChainAuditEvent, GovernanceCouncilPolicy, GovernanceOp, GovernanceProposal,
    GovernanceProposalClass, GovernanceVote, Hash, Height, MarketGovernancePolicy,
    NetworkDosPolicy, NodeId, SlashEvidence, SlashExecution, SlashMode, SlashPolicy,
    TokenEconomicsPolicy, ValidatorSet,
};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JailRecord {
    jailed_at_epoch: Height,
    jailed_until_epoch: Height,
}

/// HotStuff 协议阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// 提案阶段（Leader 提出新 Epoch）
    Propose,

    /// 投票阶段（Validators 投票）
    Vote,

    /// 预提交阶段（形成 QC）
    PreCommit,

    /// 提交阶段（最终确认）
    Commit,
}

/// HotStuff 协议状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolState {
    /// 当前阶段
    pub phase: Phase,

    /// 当前高度
    pub height: Height,

    /// 当前 view（用于超时换主）
    pub view: u64,

    /// 当前 Leader ID
    pub leader_id: NodeId,

    /// 活跃的提案
    pub active_proposal: Option<BFTProposal>,

    /// 收集的投票
    pub votes: Vec<Vote>,

    /// 最新的 QC
    pub last_qc: Option<QuorumCertificate>,
}

impl ProtocolState {
    /// 创建新的协议状态
    pub fn new(initial_leader: NodeId) -> Self {
        Self {
            phase: Phase::Propose,
            height: 0,
            view: 0,
            leader_id: initial_leader,
            active_proposal: None,
            votes: Vec::new(),
            last_qc: None,
        }
    }

    /// 进入下一个高度
    pub fn advance_height(&mut self, new_leader: NodeId) {
        self.height += 1;
        self.view = 0;
        self.phase = Phase::Propose;
        self.leader_id = new_leader;
        self.active_proposal = None;
        self.votes.clear();
    }

    /// 同高度下触发 view-change（超时换主）
    pub fn advance_view(&mut self, new_leader: NodeId) {
        self.view = self.view.saturating_add(1);
        self.phase = Phase::Propose;
        self.leader_id = new_leader;
        self.active_proposal = None;
        self.votes.clear();
    }
}

/// HotStuff-2 协议实现
pub struct HotStuffProtocol {
    /// 验证者集合
    validator_set: ValidatorSet,

    /// 本节点 ID
    self_id: NodeId,

    /// 协议状态
    state: ProtocolState,

    /// 观测到的投票（height + voter -> proposal_hash），用于检测同高度双签
    observed_votes: HashMap<(Height, NodeId), Hash>,

    /// slash 证据（当前仅记录 equivocation）
    slash_evidences: Vec<SlashEvidence>,

    /// 已执行的罚没记录（可作为状态机输入）
    slash_executions: Vec<SlashExecution>,

    /// jailed 验证者（含 jail 生效高度 + 自动解禁高度）
    jailed_validators: HashMap<NodeId, JailRecord>,

    /// 罚没治理策略（参数化）
    slash_policy: SlashPolicy,

    /// 累计证据计数（validator -> count）
    slash_counters: HashMap<NodeId, u32>,

    /// 已接收的治理操作（staged-only，不执行状态变更）
    governance_staged_ops: Vec<GovernanceOp>,

    /// 是否启用治理执行（默认关闭；未开启时仅允许 staged-only 挂点）。
    governance_execution_enabled: bool,

    /// 治理访问控制（委员会阈值 + 时间锁）。
    governance_access_policy: GovernanceAccessPolicy,
    /// 治理投票规则（SVM2026 九席位权重规则，可选启用）。
    governance_council_policy: GovernanceCouncilPolicy,

    /// 治理提案池（最小实现：内存态）。
    governance_proposals: HashMap<u64, GovernanceProposal>,

    /// 自增提案 ID。
    next_governance_proposal_id: u64,

    /// 链上治理审计事件（共识状态机内索引，按 seq 递增）。
    governance_chain_audit_events: Vec<GovernanceChainAuditEvent>,

    /// 治理审计自增序号。
    next_governance_chain_audit_seq: u64,

    /// 治理链审计根（确定性哈希，供重启恢复与一致性校验）。
    governance_chain_audit_root: Hash,

    /// 治理参数（第二类参数扩展起点）：mempool fee floor。
    governance_mempool_fee_floor: u64,

    /// 治理参数（第三类参数扩展起点）：network dos policy。
    governance_network_dos_policy: NetworkDosPolicy,

    /// 治理参数（第四类参数扩展起点）：token economics policy。
    governance_token_economics_policy: TokenEconomicsPolicy,
    /// 治理参数（第五类参数扩展）：AMM/CDP/Bond/Reserve/NAV/Buyback。
    governance_market_policy: MarketGovernancePolicy,
    /// 基于 SVM2026 `web30-core` 的 token/economics 运行时。
    token_runtime: Web30TokenRuntime,
    /// 跨模块统一账户索引服务（当前用于分红账户快照路径）。
    account_index: UnifiedAccountIndex,
    /// 基于 SVM2026 `web30-core` 的 market engine（AMM/CDP/Bond/NAV）。
    market_engine: Web30MarketEngine,
    /// 治理投票签名校验器（默认 ed25519，可注入其他方案）。
    governance_vote_verifier: Arc<dyn GovernanceVoteVerifier>,
}

impl HotStuffProtocol {
    /// 创建新的协议实例
    pub fn new(validator_set: ValidatorSet, self_id: NodeId) -> BFTResult<Self> {
        if !validator_set.is_validator(self_id) {
            return Err(BFTError::NotValidator(self_id));
        }

        let initial_leader = validator_set.validators[0];
        let governance_access_policy =
            GovernanceAccessPolicy::for_validators(&validator_set.validators)?;
        let governance_council_policy = GovernanceCouncilPolicy::disabled();

        let governance_token_economics_policy = TokenEconomicsPolicy::default();
        let governance_market_policy = MarketGovernancePolicy::default();
        let token_runtime = Web30TokenRuntime::from_policy(&governance_token_economics_policy)?;
        let mut account_index = UnifiedAccountIndex::new(100);
        account_index.refresh_from_token_runtime(&token_runtime);
        let mut market_engine = Web30MarketEngine::from_policy(&governance_market_policy)?;
        market_engine.set_dividend_account_index_snapshot(account_index.dividend_snapshot());
        Ok(Self {
            validator_set,
            self_id,
            state: ProtocolState::new(initial_leader),
            observed_votes: HashMap::new(),
            slash_evidences: Vec::new(),
            slash_executions: Vec::new(),
            jailed_validators: HashMap::new(),
            slash_policy: SlashPolicy::default(),
            slash_counters: HashMap::new(),
            governance_staged_ops: Vec::new(),
            governance_execution_enabled: false,
            governance_access_policy,
            governance_council_policy,
            governance_proposals: HashMap::new(),
            next_governance_proposal_id: 1,
            governance_chain_audit_events: Vec::new(),
            next_governance_chain_audit_seq: 0,
            governance_chain_audit_root: Self::compute_governance_chain_audit_root(&[]),
            governance_mempool_fee_floor: 1,
            governance_network_dos_policy: NetworkDosPolicy::default(),
            governance_token_economics_policy,
            governance_market_policy,
            token_runtime,
            account_index,
            market_engine,
            governance_vote_verifier: Arc::new(Ed25519GovernanceVoteVerifier),
        })
    }

    /// 设置治理投票签名校验器（I-GOV-04 execute-hook 预留）。
    pub fn set_governance_vote_verifier(&mut self, verifier: Arc<dyn GovernanceVoteVerifier>) {
        self.governance_vote_verifier = verifier;
    }

    /// 当前治理投票签名校验器名称（用于审计/调试）。
    pub fn governance_vote_verifier_name(&self) -> &'static str {
        self.governance_vote_verifier.name()
    }

    /// 当前治理投票签名校验器方案（用于能力判定）。
    pub fn governance_vote_verifier_scheme(&self) -> GovernanceVoteVerifierScheme {
        self.governance_vote_verifier.scheme()
    }

    fn record_governance_chain_audit_event(
        &mut self,
        action: &str,
        proposal_id: u64,
        actor: Option<NodeId>,
        outcome: &str,
        detail: impl Into<String>,
    ) {
        const GOVERNANCE_CHAIN_AUDIT_MAX_EVENTS: usize = 4096;

        self.next_governance_chain_audit_seq =
            self.next_governance_chain_audit_seq.saturating_add(1);
        self.governance_chain_audit_events
            .push(GovernanceChainAuditEvent {
                seq: self.next_governance_chain_audit_seq,
                height: self.state.height,
                proposal_id,
                action: action.to_string(),
                actor,
                outcome: outcome.to_string(),
                detail: detail.into(),
            });

        if self.governance_chain_audit_events.len() > GOVERNANCE_CHAIN_AUDIT_MAX_EVENTS {
            let overflow = self
                .governance_chain_audit_events
                .len()
                .saturating_sub(GOVERNANCE_CHAIN_AUDIT_MAX_EVENTS);
            self.governance_chain_audit_events.drain(0..overflow);
        }
        self.governance_chain_audit_root =
            Self::compute_governance_chain_audit_root(&self.governance_chain_audit_events);
    }

    fn hash_chain_audit_string(hasher: &mut Sha256, value: &str) {
        let bytes = value.as_bytes();
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }

    fn compute_governance_chain_audit_root(events: &[GovernanceChainAuditEvent]) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(b"NOVOVM_GOV_CHAIN_AUDIT_ROOT_V1");
        hasher.update((events.len() as u64).to_le_bytes());
        for event in events {
            hasher.update(event.seq.to_le_bytes());
            hasher.update(event.height.to_le_bytes());
            hasher.update(event.proposal_id.to_le_bytes());
            match event.actor {
                Some(actor) => {
                    hasher.update([1u8]);
                    hasher.update(actor.to_le_bytes());
                }
                None => hasher.update([0u8]),
            }
            Self::hash_chain_audit_string(&mut hasher, &event.action);
            Self::hash_chain_audit_string(&mut hasher, &event.outcome);
            Self::hash_chain_audit_string(&mut hasher, &event.detail);
        }
        hasher.finalize().into()
    }

    fn validate_governance_op(op: &GovernanceOp) -> BFTResult<()> {
        match op {
            GovernanceOp::UpdateSlashPolicy { policy } => policy.validate(),
            GovernanceOp::UpdateMempoolFeeFloor { fee_floor } => {
                if *fee_floor == 0 {
                    return Err(BFTError::InvalidProposal(
                        "mempool fee floor must be > 0".to_string(),
                    ));
                }
                Ok(())
            }
            GovernanceOp::UpdateNetworkDosPolicy { policy } => policy.validate(),
            GovernanceOp::UpdateTokenEconomicsPolicy { policy } => policy.validate(),
            GovernanceOp::UpdateMarketGovernancePolicy { policy } => policy.validate(),
            GovernanceOp::UpdateGovernanceAccessPolicy { policy } => policy.validate(),
            GovernanceOp::UpdateGovernanceCouncilPolicy { policy } => policy.validate(),
            GovernanceOp::TreasurySpend {
                to: _,
                amount,
                reason,
            } => {
                if *amount == 0 {
                    return Err(BFTError::InvalidProposal(
                        "treasury spend amount must be > 0".to_string(),
                    ));
                }
                let reason = reason.trim();
                if reason.is_empty() {
                    return Err(BFTError::InvalidProposal(
                        "treasury spend reason cannot be empty".to_string(),
                    ));
                }
                if reason.len() > 128 {
                    return Err(BFTError::InvalidProposal(
                        "treasury spend reason too long (max 128)".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }

    /// 检查是否是当前 Leader
    pub fn is_leader(&self) -> bool {
        self.state.leader_id == self.self_id && self.is_active_validator(self.self_id)
    }

    /// 获取当前高度
    pub fn current_height(&self) -> Height {
        self.state.height
    }

    /// 获取当前 view
    pub fn current_view(&self) -> u64 {
        self.state.view
    }

    /// 获取当前 leader id
    pub fn current_leader(&self) -> NodeId {
        self.state.leader_id
    }

    /// 获取当前阶段
    pub fn current_phase(&self) -> Phase {
        self.state.phase
    }

    fn is_jailed_validator(&self, node_id: NodeId) -> bool {
        self.jailed_validators
            .get(&node_id)
            .map(|rec| self.state.height < rec.jailed_until_epoch)
            .unwrap_or(false)
    }

    fn is_active_validator(&self, node_id: NodeId) -> bool {
        self.validator_set.is_validator(node_id) && !self.is_jailed_validator(node_id)
    }

    fn active_weight_of(&self, node_id: NodeId) -> Option<u64> {
        if self.is_jailed_validator(node_id) {
            return Some(0);
        }
        self.validator_set.weight_of(node_id)
    }

    fn active_total_weight(&self) -> u64 {
        self.validator_set
            .validators
            .iter()
            .copied()
            .map(|id| self.active_weight_of(id).unwrap_or(0))
            .sum()
    }

    fn effective_quorum_size(&self) -> u64 {
        let total = self.active_total_weight();
        if total == 0 {
            return 0;
        }
        (total * 2).div_ceil(3)
    }

    fn active_validator_count(&self) -> usize {
        self.validator_set
            .validators
            .iter()
            .copied()
            .filter(|id| self.is_active_validator(*id))
            .count()
    }

    fn leader_for_round(&self, height: Height, view: u64) -> NodeId {
        let count = self.validator_set.validators.len() as u64;
        if count == 0 {
            return self.state.leader_id;
        }
        let mut idx = (height + view) % count;
        for _ in 0..count {
            let candidate = self.validator_set.validators[idx as usize];
            if self.is_active_validator(candidate) {
                return candidate;
            }
            idx = (idx + 1) % count;
        }
        self.validator_set.validators[idx as usize]
    }

    fn execute_slash(&mut self, evidence: &SlashEvidence) -> SlashExecution {
        let weight_before = self.validator_set.weight_of(evidence.voter_id).unwrap_or(0);
        let already_jailed = self.is_jailed_validator(evidence.voter_id);
        let current_count = self
            .slash_counters
            .get(&evidence.voter_id)
            .copied()
            .unwrap_or(0);
        let evidence_count = current_count.saturating_add(1);
        self.slash_counters
            .insert(evidence.voter_id, evidence_count);

        let threshold = self.slash_policy.equivocation_threshold.max(1);
        let threshold_reached = evidence_count >= threshold;
        let enforce_mode = self.slash_policy.mode == SlashMode::Enforce;
        let min_active = self.slash_policy.min_active_validators.max(1) as usize;
        let cooldown_epochs = self.slash_policy.cooldown_epochs;
        // jail 后仍需满足最小活跃验证者下限（防止误伤导致活性丢失）。
        let preserve_active_floor = self.active_validator_count() > min_active;
        let can_jail = enforce_mode
            && threshold_reached
            && self.validator_set.is_validator(evidence.voter_id)
            && !already_jailed
            && preserve_active_floor;
        let jailed_until_epoch = if cooldown_epochs == 0 {
            u64::MAX
        } else {
            evidence.height.saturating_add(cooldown_epochs)
        };

        if can_jail {
            self.jailed_validators.insert(
                evidence.voter_id,
                JailRecord {
                    jailed_at_epoch: evidence.height,
                    jailed_until_epoch,
                },
            );
            // 罚没后不再计入当前轮投票，避免 equivocation 票继续贡献法定权重。
            self.state.votes.retain(|v| v.voter_id != evidence.voter_id);
        }

        // 如果当前 leader 被罚没，立即切到当前 height/view 对应的下一个可用 leader。
        if can_jail && self.state.leader_id == evidence.voter_id {
            self.state.leader_id = self.leader_for_round(self.state.height, self.state.view);
        }

        let execution = SlashExecution {
            voter_id: evidence.voter_id,
            height: evidence.height,
            reason: evidence.reason.clone(),
            weight_before,
            weight_after: if can_jail { 0 } else { weight_before },
            jailed: can_jail,
            evidence_count,
            threshold,
            policy_mode: self.slash_policy.mode.as_str().to_string(),
            min_active_validators: self.slash_policy.min_active_validators.max(1),
            jailed_until_epoch: if can_jail { jailed_until_epoch } else { 0 },
            cooldown_epochs,
        };
        self.slash_executions.push(execution.clone());
        execution
    }

    /// Propose 阶段：Leader 提出新提案
    pub fn propose(&mut self, epoch: &Epoch) -> BFTResult<BFTProposal> {
        if self.is_jailed_validator(self.self_id) {
            return Err(BFTError::SlashedValidator(self.self_id));
        }
        if !self.is_leader() {
            return Err(BFTError::Internal("Only leader can propose".to_string()));
        }

        if self.state.phase != Phase::Propose {
            return Err(BFTError::Internal(format!(
                "Cannot propose in phase {:?}",
                self.state.phase
            )));
        }

        // 获取前一个 QC 的哈希
        let prev_qc_hash = self
            .state
            .last_qc
            .as_ref()
            .map(|qc| qc.hash())
            .unwrap_or([0u8; 32]);

        // 创建提案
        let proposal = BFTProposal {
            epoch_id: epoch.id,
            view: self.state.view,
            height: self.state.height,
            proposer: self.self_id,
            state_delta_hash: epoch.state_root,
            prev_qc_hash,
            timestamp: epoch.start_time,
        };

        self.state.active_proposal = Some(proposal.clone());
        self.state.phase = Phase::Vote;

        Ok(proposal)
    }

    /// Vote 阶段：验证者对提案投票
    pub fn vote(&mut self, proposal: &BFTProposal, signing_key: &SigningKey) -> BFTResult<Vote> {
        if self.is_jailed_validator(self.self_id) {
            return Err(BFTError::SlashedValidator(self.self_id));
        }
        if self.state.phase != Phase::Vote {
            return Err(BFTError::Internal(format!(
                "Cannot vote in phase {:?}",
                self.state.phase
            )));
        }

        // 验证提案
        self.validate_proposal(proposal)?;

        // 创建投票
        let proposal_hash = proposal.hash();
        let vote = Vote::new(self.self_id, proposal_hash, proposal.height, signing_key);

        Ok(vote)
    }

    /// 收集投票（Leader 执行）
    pub fn collect_vote(&mut self, vote: Vote) -> BFTResult<Option<QuorumCertificate>> {
        if self.is_jailed_validator(vote.voter_id) {
            return Err(BFTError::SlashedValidator(vote.voter_id));
        }
        // 必须来自验证者
        if !self.validator_set.is_validator(vote.voter_id) {
            return Err(BFTError::NotValidator(vote.voter_id));
        }

        // 同高度同节点但不同提案 = equivocation，可触发 slash。
        let observed_key = (vote.height, vote.voter_id);
        if let Some(previous_hash) = self.observed_votes.get(&observed_key) {
            if *previous_hash != vote.proposal_hash {
                let evidence = SlashEvidence::equivocation(
                    vote.voter_id,
                    vote.height,
                    *previous_hash,
                    vote.proposal_hash,
                );
                self.slash_evidences.push(evidence.clone());
                self.execute_slash(&evidence);
                return Err(BFTError::EquivocationDetected {
                    voter: vote.voter_id,
                    height: vote.height,
                });
            }
        }

        // 检查投票是否针对当前提案
        let proposal = self
            .state
            .active_proposal
            .as_ref()
            .ok_or_else(|| BFTError::Internal("No active proposal".to_string()))?;

        let proposal_hash = proposal.hash();
        if vote.proposal_hash != proposal_hash {
            return Err(BFTError::InvalidProposal(
                "Vote for wrong proposal".to_string(),
            ));
        }

        // 检查是否已投票
        if self.state.votes.iter().any(|v| v.voter_id == vote.voter_id) {
            return Err(BFTError::DuplicateVote(vote.voter_id));
        }

        self.observed_votes.insert(observed_key, vote.proposal_hash);
        self.state.votes.push(vote);

        // 检查是否达到法定人数（按权重）
        let mut collected_weight = 0u64;
        for item in &self.state.votes {
            let weight = self
                .active_weight_of(item.voter_id)
                .ok_or(BFTError::NotValidator(item.voter_id))?;
            collected_weight = collected_weight.saturating_add(weight);
        }

        let quorum_size = self.effective_quorum_size();
        if collected_weight >= quorum_size {
            // 形成 QC
            let mut qc = QuorumCertificate::new(proposal_hash, proposal.height);
            for vote in &self.state.votes {
                let weight = self
                    .active_weight_of(vote.voter_id)
                    .ok_or(BFTError::NotValidator(vote.voter_id))?;
                qc.add_vote(vote.clone(), weight);
            }

            self.state.phase = Phase::PreCommit;
            self.state.last_qc = Some(qc.clone());

            Ok(Some(qc))
        } else {
            Ok(None)
        }
    }

    /// PreCommit 阶段：验证 QC 并准备提交
    pub fn pre_commit(&mut self, qc: &QuorumCertificate) -> BFTResult<()> {
        if self.state.phase != Phase::PreCommit {
            return Err(BFTError::Internal(format!(
                "Cannot pre-commit in phase {:?}",
                self.state.phase
            )));
        }

        // 验证 QC（简化版：假设已验证）
        if qc.height != self.state.height {
            return Err(BFTError::HeightMismatch {
                expected: self.state.height,
                got: qc.height,
            });
        }

        self.state.phase = Phase::Commit;
        Ok(())
    }

    /// Commit 阶段：最终提交
    pub fn commit(&mut self) -> BFTResult<()> {
        if self.state.phase != Phase::Commit {
            return Err(BFTError::Internal(format!(
                "Cannot commit in phase {:?}",
                self.state.phase
            )));
        }

        // 选择下一个 Leader（Round-robin，跳过 jailed 验证者）
        let next_height = self.state.height.saturating_add(1);
        let next_leader = self.leader_for_round(next_height, 0);

        self.state.advance_height(next_leader);
        self.observed_votes.clear();

        Ok(())
    }

    fn leader_for_view(&self, view: u64) -> NodeId {
        self.leader_for_round(self.state.height, view)
    }

    /// 超时换主：同高度推进 view，并轮换 leader。
    pub fn trigger_view_change(&mut self) -> BFTResult<NodeId> {
        let next_view = self.state.view.saturating_add(1);
        let next_leader = self.leader_for_view(next_view);
        self.state.advance_view(next_leader);
        self.observed_votes.clear();
        Ok(next_leader)
    }

    /// Fork-choice：在候选 QC 中选择“最高高度 -> 更高权重 -> 更大哈希”。
    pub fn select_fork_choice(
        &self,
        candidates: &[QuorumCertificate],
    ) -> BFTResult<QuorumCertificate> {
        let first = candidates.first().ok_or_else(|| {
            BFTError::Internal("fork choice candidates cannot be empty".to_string())
        })?;
        let mut best = first.clone();

        for qc in candidates.iter().skip(1) {
            if qc.height > best.height {
                best = qc.clone();
                continue;
            }
            if qc.height == best.height && qc.total_weight > best.total_weight {
                best = qc.clone();
                continue;
            }
            if qc.height == best.height
                && qc.total_weight == best.total_weight
                && qc.hash() > best.hash()
            {
                best = qc.clone();
            }
        }
        Ok(best)
    }

    /// 验证提案
    fn validate_proposal(&self, proposal: &BFTProposal) -> BFTResult<()> {
        // 检查高度
        if proposal.height != self.state.height {
            return Err(BFTError::HeightMismatch {
                expected: self.state.height,
                got: proposal.height,
            });
        }

        // 检查 Epoch（当前迁移实现要求 epoch_id 与 height 对齐递增）
        if proposal.epoch_id != self.state.height {
            return Err(BFTError::InvalidProposal(format!(
                "Epoch mismatch: expected {}, got {}",
                self.state.height, proposal.epoch_id
            )));
        }

        // 检查 view
        if proposal.view != self.state.view {
            return Err(BFTError::InvalidProposal(format!(
                "View mismatch: expected {}, got {}",
                self.state.view, proposal.view
            )));
        }

        if self.is_jailed_validator(proposal.proposer) {
            return Err(BFTError::InvalidProposal(format!(
                "Proposal from slashed validator: {}",
                proposal.proposer
            )));
        }

        // 检查 Leader
        if proposal.proposer != self.state.leader_id {
            return Err(BFTError::InvalidProposal(format!(
                "Proposal from wrong leader: expected {}, got {}",
                self.state.leader_id, proposal.proposer
            )));
        }

        // 检查前置 QC
        if let Some(last_qc) = &self.state.last_qc {
            if proposal.prev_qc_hash != last_qc.hash() {
                return Err(BFTError::MissingPreviousQC);
            }
        }

        Ok(())
    }

    /// 获取当前的 QC（如果已达法定人数）
    pub fn get_quorum_certificate(&self) -> Option<QuorumCertificate> {
        self.state.last_qc.clone()
    }

    /// 获取验证者集合
    pub fn validator_set(&self) -> &ValidatorSet {
        &self.validator_set
    }

    /// 同步协议状态（用于测试/模拟）
    pub fn sync_state(&mut self, state: ProtocolState) {
        self.state = state;
        self.observed_votes.clear();
    }

    /// 获取协议状态（用于测试/模拟）
    pub fn get_state(&self) -> ProtocolState {
        self.state.clone()
    }

    /// 获取已记录的 slash 证据（当前仅 equivocation）
    pub fn slash_evidences(&self) -> &[SlashEvidence] {
        &self.slash_evidences
    }

    /// 获取已执行的 slash 记录（evidence -> jail/weight 生效）
    pub fn slash_executions(&self) -> &[SlashExecution] {
        &self.slash_executions
    }

    /// 查询验证者是否已被 jailed
    pub fn is_validator_jailed(&self, node_id: NodeId) -> bool {
        self.is_jailed_validator(node_id)
    }

    /// 当前活跃验证者法定权重（会扣除已 jailed 验证者）
    pub fn active_quorum_size(&self) -> u64 {
        self.effective_quorum_size()
    }

    /// 更新罚没治理策略（运行期可由治理层下发）。
    pub fn set_slash_policy(&mut self, policy: SlashPolicy) -> BFTResult<()> {
        policy.validate()?;
        self.slash_policy = policy;
        Ok(())
    }

    /// 获取当前罚没治理策略快照。
    pub fn slash_policy(&self) -> SlashPolicy {
        self.slash_policy.clone()
    }

    fn sync_market_dividend_account_index(&mut self) {
        self.account_index
            .refresh_from_token_runtime(&self.token_runtime);
        self.market_engine
            .set_dividend_account_index_snapshot(self.account_index.dividend_snapshot());
    }

    /// 更新 token economics policy（运行期可由治理层下发）。
    pub fn set_token_economics_policy(&mut self, policy: TokenEconomicsPolicy) -> BFTResult<()> {
        policy.validate()?;
        self.token_runtime.reconfigure(&policy)?;
        self.sync_market_dividend_account_index();
        self.governance_token_economics_policy = policy;
        Ok(())
    }

    /// 读取当前 token economics policy。
    pub fn governance_token_economics_policy(&self) -> TokenEconomicsPolicy {
        self.governance_token_economics_policy.clone()
    }

    /// 更新完整经济治理参数（AMM/CDP/Bond/Reserve/NAV/Buyback）。
    pub fn set_market_governance_policy(
        &mut self,
        policy: MarketGovernancePolicy,
    ) -> BFTResult<()> {
        policy.validate()?;
        self.sync_market_dividend_account_index();
        self.market_engine.reconfigure(&policy)?;
        self.governance_market_policy = policy;
        Ok(())
    }

    /// 配置 NAV 估值源为 external feed（source_name 仅用于审计标识）。
    pub fn set_market_nav_valuation_source_external(&mut self, source_name: &str) -> BFTResult<()> {
        self.market_engine
            .set_nav_valuation_source_external(source_name)
    }

    /// 配置 NAV external feed 最新报价（bp，10000=1.0）。
    pub fn set_market_nav_external_price_bp(&mut self, price_bp: u32) -> BFTResult<()> {
        self.market_engine.set_nav_external_price_bp(price_bp)
    }

    /// 配置外币汇率源名称（用于审计标识）。
    pub fn set_market_foreign_rate_source_name(&mut self, source_name: &str) -> BFTResult<()> {
        self.market_engine.set_foreign_rate_source_name(source_name)
    }

    /// 应用外币汇率 quote spec（`BTC:rate:slippage,ETH:rate:slippage,USDT:rate:slippage`）。
    pub fn apply_market_foreign_quote_spec(&mut self, quote_spec: &str) -> BFTResult<()> {
        self.market_engine.apply_foreign_quote_spec(quote_spec)
    }

    /// 读取完整经济治理参数快照。
    pub fn governance_market_policy(&self) -> MarketGovernancePolicy {
        self.governance_market_policy.clone()
    }

    /// 读取 market runtime 生效快照（用于门禁/审计）。
    pub fn governance_market_engine_snapshot(&self) -> Web30MarketEngineSnapshot {
        self.market_engine.snapshot()
    }

    /// Compatibility shim: kept for existing gate/scripts and will be removed after profile update.
    pub fn governance_market_runtime_snapshot(&self) -> Web30MarketEngineSnapshot {
        self.governance_market_engine_snapshot()
    }

    /// mint：amount>0、不超过 locked_supply 剩余额度、且不突破 max_supply。
    pub fn mint_tokens(&mut self, account: NodeId, amount: u64) -> BFTResult<()> {
        self.token_runtime.mint(account, amount)?;
        self.sync_market_dividend_account_index();
        Ok(())
    }

    /// burn：先扣余额，再销毁总量。
    pub fn burn_tokens(&mut self, account: NodeId, amount: u64) -> BFTResult<()> {
        self.token_runtime.burn(account, amount)?;
        self.sync_market_dividend_account_index();
        Ok(())
    }

    /// gas fee 路由：provider / treasury / burn。
    pub fn charge_gas_fee(&mut self, payer: NodeId, amount: u64) -> BFTResult<FeeRoutingOutcome> {
        let outcome = self.token_runtime.charge_gas_fee(payer, amount)?;
        self.sync_market_dividend_account_index();
        Ok(outcome)
    }

    /// service fee 路由：provider / treasury / burn。
    pub fn charge_service_fee(
        &mut self,
        payer: NodeId,
        amount: u64,
    ) -> BFTResult<FeeRoutingOutcome> {
        let outcome = self.token_runtime.charge_service_fee(payer, amount)?;
        self.sync_market_dividend_account_index();
        Ok(outcome)
    }

    /// treasury spend（治理执行面）：从 treasury 主账户转账给目标账户。
    pub fn spend_treasury_tokens(
        &mut self,
        to: NodeId,
        amount: u64,
        _reason: &str,
    ) -> BFTResult<()> {
        self.token_runtime.spend_treasury(to, amount)?;
        self.sync_market_dividend_account_index();
        Ok(())
    }

    pub fn token_balance(&self, account: NodeId) -> u64 {
        self.token_runtime.balance(account).unwrap_or(0)
    }

    pub fn token_total_supply(&self) -> u64 {
        self.token_runtime.total_supply().unwrap_or(0)
    }

    pub fn token_locked_minted(&self) -> u64 {
        self.token_runtime.locked_minted_total()
    }

    pub fn token_treasury_balance(&self) -> u64 {
        self.token_runtime.treasury_balance().unwrap_or(0)
    }

    pub fn token_burned_total(&self) -> u64 {
        self.token_runtime.burned_total()
    }

    pub fn token_treasury_spent_total(&self) -> u64 {
        self.token_runtime.treasury_spent_total()
    }

    pub fn token_gas_provider_fee_pool(&self) -> u64 {
        self.token_runtime.gas_provider_pool_balance().unwrap_or(0)
    }

    pub fn token_service_provider_fee_pool(&self) -> u64 {
        self.token_runtime
            .service_provider_pool_balance()
            .unwrap_or(0)
    }

    /// 接收治理操作挂点（当前仅做结构校验与留痕，不启用执行）。
    pub fn stage_governance_op(&mut self, op: GovernanceOp) -> BFTResult<()> {
        Self::validate_governance_op(&op)?;
        self.record_governance_chain_audit_event(
            "stage",
            0,
            Some(self.self_id),
            "staged_only",
            "governance_op_staged_only",
        );
        self.governance_staged_ops.push(op);
        Err(BFTError::GovernanceNotEnabled(
            "governance_op_staged_only".to_string(),
        ))
    }

    /// 返回 staged 治理操作快照（用于门禁/审计）。
    pub fn staged_governance_ops(&self) -> &[GovernanceOp] {
        &self.governance_staged_ops
    }

    /// 设置治理执行开关（默认关闭，显式开启后才允许执行提案）。
    pub fn set_governance_execution_enabled(&mut self, enabled: bool) {
        self.governance_execution_enabled = enabled;
    }

    /// 读取治理执行开关状态。
    pub fn governance_execution_enabled(&self) -> bool {
        self.governance_execution_enabled
    }

    /// 更新治理访问策略（委员会阈值 + 时间锁）。
    pub fn set_governance_access_policy(
        &mut self,
        policy: GovernanceAccessPolicy,
    ) -> BFTResult<()> {
        policy.validate()?;
        self.governance_access_policy = policy;
        Ok(())
    }

    /// 读取治理访问策略快照。
    pub fn governance_access_policy(&self) -> GovernanceAccessPolicy {
        self.governance_access_policy.clone()
    }

    /// 更新治理九席位规则。
    pub fn set_governance_council_policy(
        &mut self,
        policy: GovernanceCouncilPolicy,
    ) -> BFTResult<()> {
        policy.validate()?;
        self.validate_council_policy_members(&policy)?;
        self.governance_council_policy = policy;
        Ok(())
    }

    /// 读取治理九席位规则快照。
    pub fn governance_council_policy(&self) -> GovernanceCouncilPolicy {
        self.governance_council_policy.clone()
    }

    fn validate_committee_approvals(
        &self,
        approvals: &[NodeId],
        committee: &[NodeId],
        threshold: u32,
        phase: &str,
    ) -> BFTResult<()> {
        if approvals.is_empty() {
            return Err(BFTError::InvalidProposal(format!(
                "{} approvals cannot be empty",
                phase
            )));
        }
        let committee_set: HashSet<NodeId> = committee.iter().copied().collect();
        let mut unique = HashSet::with_capacity(approvals.len());
        for member in approvals {
            if !unique.insert(*member) {
                return Err(BFTError::DuplicateVote(*member));
            }
            if !committee_set.contains(member) {
                return Err(BFTError::InvalidProposal(format!(
                    "{} approval member {} is not in committee",
                    phase, member
                )));
            }
            if !self.is_active_validator(*member) {
                return if self.validator_set.is_validator(*member) {
                    Err(BFTError::SlashedValidator(*member))
                } else {
                    Err(BFTError::NotValidator(*member))
                };
            }
        }

        if unique.len() < threshold as usize {
            return Err(BFTError::InsufficientVotes {
                required: threshold as usize,
                received: unique.len(),
            });
        }
        Ok(())
    }

    fn validate_council_member(&self, node_id: NodeId) -> BFTResult<()> {
        let member_map = self.governance_council_policy.member_weight_map();
        if !member_map.contains_key(&node_id) {
            return Err(BFTError::InvalidProposal(format!(
                "governance council voter {} is not a council member",
                node_id
            )));
        }
        Ok(())
    }

    fn validate_council_policy_members(&self, policy: &GovernanceCouncilPolicy) -> BFTResult<()> {
        if !policy.enabled {
            return Ok(());
        }
        for member in &policy.members {
            if !self.validator_set.is_validator(member.node_id) {
                return Err(BFTError::NotValidator(member.node_id));
            }
        }
        Ok(())
    }

    /// 提交治理提案（最小闭环：仅支持 UpdateSlashPolicy）。
    pub fn submit_governance_proposal(
        &mut self,
        proposer: NodeId,
        op: GovernanceOp,
    ) -> BFTResult<GovernanceProposal> {
        self.submit_governance_proposal_with_approvals(proposer, &[proposer], op)
    }

    /// 提交治理提案（委员会阈值模型）。
    pub fn submit_governance_proposal_with_approvals(
        &mut self,
        proposer: NodeId,
        proposer_approvals: &[NodeId],
        op: GovernanceOp,
    ) -> BFTResult<GovernanceProposal> {
        if !self.is_active_validator(proposer) {
            return if self.validator_set.is_validator(proposer) {
                Err(BFTError::SlashedValidator(proposer))
            } else {
                Err(BFTError::NotValidator(proposer))
            };
        }
        self.validate_committee_approvals(
            proposer_approvals,
            &self.governance_access_policy.proposer_committee,
            self.governance_access_policy.proposer_threshold,
            "governance proposer",
        )?;
        if !proposer_approvals.contains(&proposer) {
            return Err(BFTError::InvalidProposal(format!(
                "governance proposer {} is not in proposer_approvals",
                proposer
            )));
        }
        if self.governance_council_policy.enabled {
            self.validate_council_member(proposer)?;
        }

        Self::validate_governance_op(&op)?;

        let proposal = GovernanceProposal {
            proposal_id: self.next_governance_proposal_id,
            proposer,
            created_height: self.state.height,
            op,
        };
        self.next_governance_proposal_id = self.next_governance_proposal_id.saturating_add(1);
        self.governance_proposals
            .insert(proposal.proposal_id, proposal.clone());
        let proposal_class = proposal.op.proposal_class();
        self.record_governance_chain_audit_event(
            "submit",
            proposal.proposal_id,
            Some(proposer),
            "accepted",
            format!(
                "class={:?}; proposer_approvals={}",
                proposal_class,
                proposer_approvals.len()
            ),
        );
        Ok(proposal)
    }

    /// 执行治理提案（投票签名 + weighted quorum 验证通过后生效）。
    pub fn execute_governance_proposal(
        &mut self,
        proposal_id: u64,
        votes: &[GovernanceVote],
        public_keys: &HashMap<NodeId, VerifyingKey>,
    ) -> BFTResult<bool> {
        self.execute_governance_proposal_with_executor_approvals(
            proposal_id,
            votes,
            public_keys,
            &[self.self_id],
        )
    }

    /// 执行治理提案（委员会阈值 + 时间锁 + 投票签名）。
    pub fn execute_governance_proposal_with_executor_approvals(
        &mut self,
        proposal_id: u64,
        votes: &[GovernanceVote],
        public_keys: &HashMap<NodeId, VerifyingKey>,
        executor_approvals: &[NodeId],
    ) -> BFTResult<bool> {
        if !self.governance_execution_enabled {
            return Err(BFTError::GovernanceNotEnabled(
                "governance_execution_disabled".to_string(),
            ));
        }
        let proposal = self
            .governance_proposals
            .get(&proposal_id)
            .cloned()
            .ok_or_else(|| {
                BFTError::Internal(format!("governance proposal not found: {}", proposal_id))
            })?;
        self.validate_committee_approvals(
            executor_approvals,
            &self.governance_access_policy.executor_committee,
            self.governance_access_policy.executor_threshold,
            "governance executor",
        )?;
        let earliest_execute_height = proposal
            .created_height
            .saturating_add(self.governance_access_policy.timelock_epochs);
        if self.state.height < earliest_execute_height {
            return Err(BFTError::InvalidProposal(format!(
                "governance timelock not satisfied: current_height={} required_height={}",
                self.state.height, earliest_execute_height
            )));
        }
        let proposal_digest = proposal.digest();
        let council_member_map = if self.governance_council_policy.enabled {
            Some(self.governance_council_policy.member_weight_map())
        } else {
            None
        };
        let active_verifier_name = self.governance_vote_verifier.name();
        let active_verifier_scheme = self.governance_vote_verifier.scheme();

        let mut seen = HashSet::with_capacity(votes.len());
        let mut validated_votes = Vec::with_capacity(votes.len());
        for vote in votes {
            if vote.proposal_id != proposal_id {
                return Err(BFTError::InvalidProposal(format!(
                    "governance vote proposal mismatch: expected={} got={}",
                    proposal_id, vote.proposal_id
                )));
            }
            if vote.proposal_height != proposal.created_height {
                return Err(BFTError::InvalidProposal(format!(
                    "governance vote proposal_height mismatch: expected={} got={}",
                    proposal.created_height, vote.proposal_height
                )));
            }
            if vote.proposal_digest != proposal_digest {
                return Err(BFTError::InvalidProposal(
                    "governance vote proposal_digest mismatch".to_string(),
                ));
            }
            if !seen.insert(vote.voter_id) {
                return Err(BFTError::DuplicateVote(vote.voter_id));
            }
            if !self.is_active_validator(vote.voter_id) {
                return if self.validator_set.is_validator(vote.voter_id) {
                    Err(BFTError::SlashedValidator(vote.voter_id))
                } else {
                    Err(BFTError::NotValidator(vote.voter_id))
                };
            }
            if let Some(member_map) = council_member_map.as_ref() {
                if !member_map.contains_key(&vote.voter_id) {
                    return Err(BFTError::InvalidProposal(format!(
                        "governance council vote from non-member node {}",
                        vote.voter_id
                    )));
                }
            }
            let key = public_keys
                .get(&vote.voter_id)
                .ok_or(BFTError::NotValidator(vote.voter_id))?;
            validated_votes.push((vote, key));
        }

        let verification_reports = if validated_votes.len() < GOVERNANCE_VOTE_VERIFY_BATCH_MIN {
            let mut reports = Vec::with_capacity(validated_votes.len());
            for (vote, key) in &validated_votes {
                match self.governance_vote_verifier.verify_with_report(vote, key) {
                    Ok(report) => reports.push(report),
                    Err(err) => {
                        let reason = err.to_string().replace('\n', " ");
                        self.record_governance_chain_audit_event(
                            "execute",
                            proposal_id,
                            Some(vote.voter_id),
                            "reject",
                            format!(
                                "vote_verifier_reject voter={} verifier={} signature_scheme={} reason={}",
                                vote.voter_id,
                                active_verifier_name,
                                active_verifier_scheme.as_str(),
                                reason
                            ),
                        );
                        return Err(err);
                    }
                }
            }
            reports
        } else {
            let verify_inputs: Vec<GovernanceVoteVerificationInput<'_>> = validated_votes
                .iter()
                .map(|(vote, key)| GovernanceVoteVerificationInput { vote, key })
                .collect();
            match self
                .governance_vote_verifier
                .verify_batch_with_report(&verify_inputs)
            {
                Ok(reports) => reports,
                Err(batch_err) => {
                    // Fallback to per-vote verification to retain deterministic reject diagnostics.
                    for (vote, key) in &validated_votes {
                        match self.governance_vote_verifier.verify_with_report(vote, key) {
                            Ok(_) => {}
                            Err(err) => {
                                let reason = err.to_string().replace('\n', " ");
                                self.record_governance_chain_audit_event(
                                    "execute",
                                    proposal_id,
                                    Some(vote.voter_id),
                                    "reject",
                                    format!(
                                        "vote_verifier_reject voter={} verifier={} signature_scheme={} reason={}",
                                        vote.voter_id,
                                        active_verifier_name,
                                        active_verifier_scheme.as_str(),
                                        reason
                                    ),
                                );
                                return Err(err);
                            }
                        }
                    }
                    let reason = batch_err.to_string().replace('\n', " ");
                    self.record_governance_chain_audit_event(
                        "execute",
                        proposal_id,
                        None,
                        "reject",
                        format!(
                            "vote_verifier_batch_reject verifier={} signature_scheme={} reason={}",
                            active_verifier_name,
                            active_verifier_scheme.as_str(),
                            reason
                        ),
                    );
                    return Err(batch_err);
                }
            }
        };
        if verification_reports.len() != validated_votes.len() {
            return Err(BFTError::Internal(format!(
                "governance vote verifier report size mismatch: expected={} got={}",
                validated_votes.len(),
                verification_reports.len()
            )));
        }

        let mut support_weight = 0u64;
        let mut council_support_bp = 0u16;
        let mut council_categories = HashSet::new();
        let mut verification_report: Option<GovernanceVoteVerificationReport> = None;
        for (idx, (vote, _)) in validated_votes.iter().enumerate() {
            if verification_report.is_none() {
                verification_report = Some(verification_reports[idx].clone());
            }
            if vote.support {
                support_weight = support_weight
                    .checked_add(self.active_weight_of(vote.voter_id).unwrap_or(0))
                    .ok_or_else(|| {
                        BFTError::Internal("governance support weight overflow".to_string())
                    })?;
                if let Some(member_map) = council_member_map.as_ref() {
                    let (seat_weight_bp, category) =
                        member_map.get(&vote.voter_id).copied().ok_or_else(|| {
                            BFTError::InvalidProposal(format!(
                                "governance council vote from non-member node {}",
                                vote.voter_id
                            ))
                        })?;
                    council_support_bp = council_support_bp
                        .checked_add(seat_weight_bp)
                        .ok_or_else(|| {
                            BFTError::Internal(
                                "governance council support basis points overflow".to_string(),
                            )
                        })?;
                    council_categories.insert(category);
                }
            }
        }

        if self.governance_council_policy.enabled {
            let proposal_class = proposal.op.proposal_class();
            let threshold_bp = self.governance_council_policy.threshold_for(proposal_class);
            if council_support_bp <= threshold_bp {
                return Err(BFTError::InsufficientVotes {
                    required: (threshold_bp as usize).saturating_add(1),
                    received: council_support_bp as usize,
                });
            }
            if proposal_class == GovernanceProposalClass::EmergencyFreeze
                && council_categories.len()
                    < self.governance_council_policy.emergency_min_categories as usize
            {
                return Err(BFTError::InvalidProposal(format!(
                    "governance emergency diversity not satisfied: required_categories={} got={}",
                    self.governance_council_policy.emergency_min_categories,
                    council_categories.len()
                )));
            }
        } else {
            let quorum = self.effective_quorum_size();
            if support_weight < quorum {
                return Err(BFTError::InsufficientVotes {
                    required: quorum as usize,
                    received: support_weight as usize,
                });
            }
        }

        match proposal.op.clone() {
            GovernanceOp::UpdateSlashPolicy { policy } => self.set_slash_policy(policy)?,
            GovernanceOp::UpdateMempoolFeeFloor { fee_floor } => {
                self.governance_mempool_fee_floor = fee_floor;
            }
            GovernanceOp::UpdateNetworkDosPolicy { policy } => {
                self.governance_network_dos_policy = policy;
            }
            GovernanceOp::UpdateTokenEconomicsPolicy { policy } => {
                self.set_token_economics_policy(policy)?;
            }
            GovernanceOp::UpdateMarketGovernancePolicy { policy } => {
                self.set_market_governance_policy(policy)?;
            }
            GovernanceOp::UpdateGovernanceAccessPolicy { policy } => {
                self.set_governance_access_policy(policy)?;
            }
            GovernanceOp::UpdateGovernanceCouncilPolicy { policy } => {
                self.set_governance_council_policy(policy)?;
            }
            GovernanceOp::TreasurySpend { to, amount, reason } => {
                self.spend_treasury_tokens(to, amount, &reason)?;
            }
        }
        self.governance_proposals.remove(&proposal_id);
        let verification_detail = if let Some(report) = verification_report {
            format!(
                "support_votes={} verifier={} signature_scheme={}",
                seen.len(),
                report.verifier_name,
                report.scheme.as_str()
            )
        } else {
            format!(
                "support_votes={} verifier={} signature_scheme={}",
                seen.len(),
                active_verifier_name,
                active_verifier_scheme.as_str()
            )
        };
        self.record_governance_chain_audit_event(
            "execute",
            proposal_id,
            executor_approvals.first().copied(),
            "applied",
            verification_detail,
        );
        Ok(true)
    }

    /// 读取治理参数：mempool fee floor。
    pub fn governance_mempool_fee_floor(&self) -> u64 {
        self.governance_mempool_fee_floor
    }

    /// 读取治理参数：network dos policy。
    pub fn governance_network_dos_policy(&self) -> NetworkDosPolicy {
        self.governance_network_dos_policy.clone()
    }

    /// 查询单个待执行治理提案。
    pub fn governance_pending_proposal(&self, proposal_id: u64) -> Option<GovernanceProposal> {
        self.governance_proposals.get(&proposal_id).cloned()
    }

    /// 查询全部待执行治理提案快照。
    pub fn governance_pending_proposals(&self) -> Vec<GovernanceProposal> {
        let mut items: Vec<_> = self.governance_proposals.values().cloned().collect();
        items.sort_by_key(|p| p.proposal_id);
        items
    }

    /// 查询链上治理审计事件快照（seq 递增）。
    pub fn governance_chain_audit_events(&self) -> Vec<GovernanceChainAuditEvent> {
        self.governance_chain_audit_events.clone()
    }

    /// 查询治理链审计根（确定性哈希）。
    pub fn governance_chain_audit_root(&self) -> Hash {
        self.governance_chain_audit_root
    }

    /// 从外部持久化快照恢复治理链审计事件（重启恢复入口）。
    pub fn restore_governance_chain_audit_events(
        &mut self,
        mut events: Vec<GovernanceChainAuditEvent>,
    ) {
        const GOVERNANCE_CHAIN_AUDIT_MAX_EVENTS: usize = 4096;

        events.sort_by_key(|event| event.seq);
        events.dedup_by_key(|event| event.seq);
        if events.len() > GOVERNANCE_CHAIN_AUDIT_MAX_EVENTS {
            let start = events
                .len()
                .saturating_sub(GOVERNANCE_CHAIN_AUDIT_MAX_EVENTS);
            events = events[start..].to_vec();
        }
        self.next_governance_chain_audit_seq = events.last().map(|event| event.seq).unwrap_or(0);
        self.governance_chain_audit_events = events;
        self.governance_chain_audit_root =
            Self::compute_governance_chain_audit_root(&self.governance_chain_audit_events);
    }

    /// 查询验证者的 jail 自动解禁高度（仅当当前仍处于 jailed 状态时返回）。
    pub fn validator_jailed_until_epoch(&self, node_id: NodeId) -> Option<Height> {
        self.jailed_validators.get(&node_id).and_then(|rec| {
            (self.state.height < rec.jailed_until_epoch).then_some(rec.jailed_until_epoch)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epoch::Epoch;
    use crate::types::{GovernanceCouncilMember, GovernanceCouncilSeat};
    use ed25519_dalek::VerifyingKey;
    use rand::rngs::OsRng;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct RejectAllGovernanceVoteVerifier;

    impl GovernanceVoteVerifier for RejectAllGovernanceVoteVerifier {
        fn name(&self) -> &'static str {
            "test_reject_all"
        }

        fn scheme(&self) -> GovernanceVoteVerifierScheme {
            GovernanceVoteVerifierScheme::Ed25519
        }

        fn verify(&self, _vote: &GovernanceVote, _key: &VerifyingKey) -> BFTResult<()> {
            Err(BFTError::InvalidSignature(
                "test governance verifier rejected vote".to_string(),
            ))
        }
    }

    fn generate_keys(count: usize) -> (Vec<SigningKey>, HashMap<NodeId, VerifyingKey>) {
        let signing_keys: Vec<_> = (0..count)
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();
        let public_keys: HashMap<_, _> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();
        (signing_keys, public_keys)
    }

    #[test]
    fn test_protocol_full_round() {
        // 4 个验证者
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);

        // 生成密钥
        let signing_keys: Vec<_> = (0..4).map(|_| SigningKey::generate(&mut OsRng)).collect();

        // 创建协议实例（节点 0 是初始 Leader）
        let mut leader_protocol = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();
        assert!(leader_protocol.is_leader());

        // 创建 Epoch
        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 100);
        epoch.add_batch(2, 150);

        let mut batch_results = HashMap::new();
        batch_results.insert(1, [1u8; 32]);
        batch_results.insert(2, [2u8; 32]);
        epoch.compute_state_root(&batch_results).unwrap();

        // Leader 提出提案
        let proposal = leader_protocol.propose(&epoch).unwrap();
        assert_eq!(leader_protocol.current_phase(), Phase::Vote);

        // 其他验证者投票
        let mut votes = Vec::new();
        for i in 0..4 {
            let mut voter_protocol = HotStuffProtocol::new(validator_set.clone(), i).unwrap();
            voter_protocol.state = leader_protocol.state.clone(); // 同步状态

            let vote = voter_protocol
                .vote(&proposal, &signing_keys[i as usize])
                .unwrap();
            votes.push(vote);
        }

        // Leader 收集投票
        let mut qc_opt = None;
        for vote in votes {
            if let Some(qc) = leader_protocol.collect_vote(vote).unwrap() {
                qc_opt = Some(qc);
                break;
            }
        }

        assert!(qc_opt.is_some());
        assert_eq!(leader_protocol.current_phase(), Phase::PreCommit);

        // PreCommit
        let qc = qc_opt.unwrap();
        leader_protocol.pre_commit(&qc).unwrap();
        assert_eq!(leader_protocol.current_phase(), Phase::Commit);

        // Commit
        leader_protocol.commit().unwrap();
        assert_eq!(leader_protocol.current_height(), 1);
        assert_eq!(leader_protocol.current_phase(), Phase::Propose);
    }

    #[test]
    fn test_weighted_quorum_is_based_on_stake_weight() {
        // two validators with asymmetric stake: total=10, quorum=7.
        let validator_set = ValidatorSet::new_weighted(vec![(0, 6), (1, 4)]).unwrap();
        let signing_keys: Vec<_> = (0..2).map(|_| SigningKey::generate(&mut OsRng)).collect();

        let mut leader_protocol = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();
        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);

        let proposal = leader_protocol.propose(&epoch).unwrap();
        let mut voter0 = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();
        voter0.sync_state(leader_protocol.get_state());
        let vote0 = voter0.vote(&proposal, &signing_keys[0]).unwrap();
        let qc_none = leader_protocol.collect_vote(vote0).unwrap();
        assert!(qc_none.is_none()); // 6 < 7

        let mut voter1 = HotStuffProtocol::new(validator_set, 1).unwrap();
        voter1.sync_state(leader_protocol.get_state());
        let vote1 = voter1.vote(&proposal, &signing_keys[1]).unwrap();
        let qc = leader_protocol.collect_vote(vote1).unwrap();
        assert!(qc.is_some()); // 6 + 4 >= 7
        assert_eq!(qc.unwrap().total_weight, 10);
    }

    #[test]
    fn test_equivocation_detection_records_slash_evidence() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
        let mut leader_protocol = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 8);
        let proposal = leader_protocol.propose(&epoch).unwrap();

        let mut voter = HotStuffProtocol::new(validator_set, 1).unwrap();
        voter.sync_state(leader_protocol.get_state());
        let first_vote = voter.vote(&proposal, &signing_keys[1]).unwrap();
        let _ = leader_protocol.collect_vote(first_vote).unwrap();

        let mut conflicted_hash = proposal.hash();
        conflicted_hash[0] ^= 0xFF;
        let conflicting_vote = Vote::new(1, conflicted_hash, proposal.height, &signing_keys[1]);
        let err = leader_protocol.collect_vote(conflicting_vote).unwrap_err();
        match err {
            BFTError::EquivocationDetected { voter, height } => {
                assert_eq!(voter, 1);
                assert_eq!(height, proposal.height);
            }
            _ => panic!("expected EquivocationDetected error"),
        }
        assert_eq!(leader_protocol.slash_evidences().len(), 1);
        assert_eq!(leader_protocol.slash_evidences()[0].reason, "equivocation");
        assert_eq!(leader_protocol.slash_executions().len(), 1);
        assert!(leader_protocol.slash_executions()[0].jailed);
        assert!(leader_protocol.is_validator_jailed(1));
        assert_eq!(leader_protocol.active_quorum_size(), 2);
    }

    #[test]
    fn test_slashed_validator_rejected_and_quorum_recomputed() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).unwrap();

        // voter-1 first vote accepted
        let mut voter1 = HotStuffProtocol::new(validator_set.clone(), 1).unwrap();
        voter1.sync_state(leader_protocol.get_state());
        let vote1 = voter1.vote(&proposal, &signing_keys[1]).unwrap();
        assert!(leader_protocol.collect_vote(vote1).unwrap().is_none());

        // conflicting vote from voter-1 -> slash execution
        let mut other_hash = proposal.hash();
        other_hash[0] ^= 0x11;
        let conflicting = Vote::new(1, other_hash, proposal.height, &signing_keys[1]);
        assert!(matches!(
            leader_protocol.collect_vote(conflicting),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        ));
        assert!(leader_protocol.is_validator_jailed(1));
        assert_eq!(leader_protocol.active_quorum_size(), 2);

        // slashed voter cannot vote anymore
        let mut voter1_again = HotStuffProtocol::new(validator_set.clone(), 1).unwrap();
        voter1_again.sync_state(leader_protocol.get_state());
        let vote1_again = voter1_again.vote(&proposal, &signing_keys[1]).unwrap();
        assert!(matches!(
            leader_protocol.collect_vote(vote1_again),
            Err(BFTError::SlashedValidator(1))
        ));

        // non-slashed validators can still close QC with recomputed quorum (2/2)
        let mut voter0 = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();
        voter0.sync_state(leader_protocol.get_state());
        let vote0 = voter0.vote(&proposal, &signing_keys[0]).unwrap();
        assert!(leader_protocol.collect_vote(vote0).unwrap().is_none());

        let mut voter2 = HotStuffProtocol::new(validator_set, 2).unwrap();
        voter2.sync_state(leader_protocol.get_state());
        let vote2 = voter2.vote(&proposal, &signing_keys[2]).unwrap();
        let qc = leader_protocol.collect_vote(vote2).unwrap();
        assert!(qc.is_some());
    }

    #[test]
    fn test_view_change_rotates_leader_and_resets_phase() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();

        assert_eq!(protocol.current_height(), 0);
        assert_eq!(protocol.current_view(), 0);
        assert_eq!(protocol.current_leader(), 0);

        let next = protocol.trigger_view_change().unwrap();
        assert_eq!(next, 1);
        assert_eq!(protocol.current_view(), 1);
        assert_eq!(protocol.current_leader(), 1);
        assert_eq!(protocol.current_phase(), Phase::Propose);

        let next2 = protocol.trigger_view_change().unwrap();
        assert_eq!(next2, 2);
        assert_eq!(protocol.current_view(), 2);
        assert_eq!(protocol.current_leader(), 2);
    }

    #[test]
    fn test_fork_choice_prefers_height_then_weight() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let protocol = HotStuffProtocol::new(validator_set, 0).unwrap();

        let mut low = QuorumCertificate::new([1u8; 32], 10);
        low.total_weight = 4;
        let mut high = QuorumCertificate::new([2u8; 32], 11);
        high.total_weight = 3;
        let best = protocol
            .select_fork_choice(&[low.clone(), high.clone()])
            .unwrap();
        assert_eq!(best.height, 11);

        let mut same_height_low_weight = QuorumCertificate::new([3u8; 32], 12);
        same_height_low_weight.total_weight = 3;
        let mut same_height_high_weight = QuorumCertificate::new([4u8; 32], 12);
        same_height_high_weight.total_weight = 4;
        let best2 = protocol
            .select_fork_choice(&[same_height_low_weight, same_height_high_weight])
            .unwrap();
        assert_eq!(best2.height, 12);
        assert_eq!(best2.total_weight, 4);
    }

    #[test]
    fn test_slash_policy_threshold_requires_multiple_evidences() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();
        leader_protocol
            .set_slash_policy(SlashPolicy {
                mode: SlashMode::Enforce,
                equivocation_threshold: 2,
                min_active_validators: 2,
                cooldown_epochs: 0,
            })
            .unwrap();

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).unwrap();

        let mut voter1 = HotStuffProtocol::new(validator_set, 1).unwrap();
        voter1.sync_state(leader_protocol.get_state());
        let vote1 = voter1.vote(&proposal, &signing_keys[1]).unwrap();
        assert!(leader_protocol.collect_vote(vote1).unwrap().is_none());

        let mut hash_a = proposal.hash();
        hash_a[0] ^= 0x33;
        assert!(matches!(
            leader_protocol.collect_vote(Vote::new(1, hash_a, proposal.height, &signing_keys[1])),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        ));
        assert!(!leader_protocol.is_validator_jailed(1));
        assert_eq!(leader_protocol.slash_executions().len(), 1);
        assert!(!leader_protocol.slash_executions()[0].jailed);
        assert_eq!(leader_protocol.slash_executions()[0].evidence_count, 1);
        assert_eq!(leader_protocol.slash_executions()[0].threshold, 2);

        let mut hash_b = proposal.hash();
        hash_b[0] ^= 0x55;
        assert!(matches!(
            leader_protocol.collect_vote(Vote::new(1, hash_b, proposal.height, &signing_keys[1])),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        ));
        assert!(leader_protocol.is_validator_jailed(1));
        assert_eq!(leader_protocol.slash_executions().len(), 2);
        assert!(leader_protocol.slash_executions()[1].jailed);
        assert_eq!(leader_protocol.slash_executions()[1].evidence_count, 2);
        assert_eq!(leader_protocol.slash_executions()[1].policy_mode, "enforce");
    }

    #[test]
    fn test_slash_policy_observe_only_never_jails() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();
        leader_protocol
            .set_slash_policy(SlashPolicy {
                mode: SlashMode::ObserveOnly,
                equivocation_threshold: 1,
                min_active_validators: 2,
                cooldown_epochs: 0,
            })
            .unwrap();

        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 10);
        let proposal = leader_protocol.propose(&epoch).unwrap();
        let mut voter1 = HotStuffProtocol::new(validator_set, 1).unwrap();
        voter1.sync_state(leader_protocol.get_state());
        let vote1 = voter1.vote(&proposal, &signing_keys[1]).unwrap();
        assert!(leader_protocol.collect_vote(vote1).unwrap().is_none());

        let mut hash_a = proposal.hash();
        hash_a[0] ^= 0x11;
        assert!(matches!(
            leader_protocol.collect_vote(Vote::new(1, hash_a, proposal.height, &signing_keys[1])),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        ));
        assert!(!leader_protocol.is_validator_jailed(1));
        assert_eq!(leader_protocol.active_quorum_size(), 2);
        assert_eq!(leader_protocol.slash_executions().len(), 1);
        assert!(!leader_protocol.slash_executions()[0].jailed);
        assert_eq!(
            leader_protocol.slash_executions()[0].policy_mode,
            "observe_only"
        );
    }

    #[test]
    fn test_slash_policy_cooldown_auto_unjail() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, _) = generate_keys(3);
        let mut leader_protocol = HotStuffProtocol::new(validator_set, 2).unwrap();
        leader_protocol
            .set_slash_policy(SlashPolicy {
                mode: SlashMode::Enforce,
                equivocation_threshold: 1,
                min_active_validators: 2,
                cooldown_epochs: 2,
            })
            .unwrap();
        let _ = leader_protocol.trigger_view_change().unwrap();
        let _ = leader_protocol.trigger_view_change().unwrap();
        assert_eq!(leader_protocol.current_leader(), 2);

        let mut epoch0 = Epoch::new(0, 0, 0);
        epoch0.add_batch(1, 10);
        let proposal0 = leader_protocol.propose(&epoch0).unwrap();

        let vote1 = Vote::new(1, proposal0.hash(), proposal0.height, &signing_keys[1]);
        assert!(leader_protocol.collect_vote(vote1).unwrap().is_none());
        let mut conflicting_hash = proposal0.hash();
        conflicting_hash[0] ^= 0x42;
        assert!(matches!(
            leader_protocol.collect_vote(Vote::new(
                1,
                conflicting_hash,
                proposal0.height,
                &signing_keys[1]
            )),
            Err(BFTError::EquivocationDetected { voter: 1, .. })
        ));
        assert!(leader_protocol.is_validator_jailed(1));
        assert_eq!(leader_protocol.validator_jailed_until_epoch(1), Some(2));
        assert_eq!(leader_protocol.active_quorum_size(), 2);

        // height=0 收敛并提交到 height=1（仍在 cooldown）。
        assert!(leader_protocol
            .collect_vote(Vote::new(
                2,
                proposal0.hash(),
                proposal0.height,
                &signing_keys[2]
            ))
            .unwrap()
            .is_none());
        let qc0 = leader_protocol
            .collect_vote(Vote::new(
                0,
                proposal0.hash(),
                proposal0.height,
                &signing_keys[0],
            ))
            .unwrap()
            .expect("height0 should form qc");
        leader_protocol.pre_commit(&qc0).unwrap();
        leader_protocol.commit().unwrap();
        assert_eq!(leader_protocol.current_height(), 1);
        assert!(leader_protocol.is_validator_jailed(1));
        assert_eq!(leader_protocol.validator_jailed_until_epoch(1), Some(2));

        // height=1 未到期，仍应拒绝被 jailed 验证者投票。
        let mut epoch1 = Epoch::new(1, 1, 0);
        epoch1.add_batch(1, 11);
        let proposal1 = leader_protocol.propose(&epoch1).unwrap();
        assert!(matches!(
            leader_protocol.collect_vote(Vote::new(
                1,
                proposal1.hash(),
                proposal1.height,
                &signing_keys[1]
            )),
            Err(BFTError::SlashedValidator(1))
        ));

        // 再提交一个高度到 height=2，触发自动解禁。
        assert!(leader_protocol
            .collect_vote(Vote::new(
                2,
                proposal1.hash(),
                proposal1.height,
                &signing_keys[2]
            ))
            .unwrap()
            .is_none());
        let qc1 = leader_protocol
            .collect_vote(Vote::new(
                0,
                proposal1.hash(),
                proposal1.height,
                &signing_keys[0],
            ))
            .unwrap()
            .expect("height1 should form qc");
        leader_protocol.pre_commit(&qc1).unwrap();
        leader_protocol.commit().unwrap();

        assert_eq!(leader_protocol.current_height(), 2);
        assert!(!leader_protocol.is_validator_jailed(1));
        assert_eq!(leader_protocol.validator_jailed_until_epoch(1), None);

        let mut epoch2 = Epoch::new(2, 2, 0);
        epoch2.add_batch(1, 12);
        let proposal2 = leader_protocol.propose(&epoch2).unwrap();
        let accepted = leader_protocol
            .collect_vote(Vote::new(
                1,
                proposal2.hash(),
                proposal2.height,
                &signing_keys[1],
            ))
            .unwrap();
        assert!(accepted.is_none());
    }

    #[test]
    fn test_governance_update_slash_policy_is_staged_only() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        let before = protocol.slash_policy();
        let candidate = SlashPolicy {
            mode: SlashMode::ObserveOnly,
            equivocation_threshold: 3,
            min_active_validators: 2,
            cooldown_epochs: 5,
        };

        let result = protocol.stage_governance_op(GovernanceOp::UpdateSlashPolicy {
            policy: candidate.clone(),
        });
        assert!(matches!(result, Err(BFTError::GovernanceNotEnabled(_))));

        let staged = protocol.staged_governance_ops();
        assert_eq!(staged.len(), 1);
        assert_eq!(
            staged[0],
            GovernanceOp::UpdateSlashPolicy { policy: candidate }
        );
        // 未启用治理执行链路时，运行期策略应保持不变。
        assert_eq!(protocol.slash_policy(), before);
    }

    #[test]
    fn test_governance_execute_update_slash_policy_with_quorum() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let candidate = SlashPolicy {
            mode: SlashMode::ObserveOnly,
            equivocation_threshold: 3,
            min_active_validators: 2,
            cooldown_epochs: 9,
        };
        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateSlashPolicy {
                    policy: candidate.clone(),
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(executed);
        assert_eq!(protocol.slash_policy(), candidate);
        assert!(protocol.governance_proposals.is_empty());
    }

    #[test]
    fn test_governance_execute_rejected_when_disabled() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateSlashPolicy {
                    policy: SlashPolicy {
                        mode: SlashMode::ObserveOnly,
                        equivocation_threshold: 2,
                        min_active_validators: 2,
                        cooldown_epochs: 3,
                    },
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let result =
            protocol.execute_governance_proposal(proposal.proposal_id, &votes, &public_keys);
        assert!(matches!(result, Err(BFTError::GovernanceNotEnabled(_))));
    }

    #[test]
    fn test_governance_execute_update_mempool_fee_floor() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let proposal = protocol
            .submit_governance_proposal(0, GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 9 })
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(executed);
        assert_eq!(protocol.governance_mempool_fee_floor(), 9);
    }

    #[test]
    fn test_governance_chain_audit_records_submit_and_execute() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let proposal = protocol
            .submit_governance_proposal(0, GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 13 })
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(executed);

        let events = protocol.governance_chain_audit_events();
        let submit = events
            .iter()
            .find(|event| event.proposal_id == proposal.proposal_id && event.action == "submit")
            .expect("missing submit chain audit event");
        assert_eq!(submit.outcome, "accepted");
        let execute = events
            .iter()
            .find(|event| event.proposal_id == proposal.proposal_id && event.action == "execute")
            .expect("missing execute chain audit event");
        assert_eq!(execute.outcome, "applied");
        assert!(execute.seq > submit.seq);
        let root = protocol.governance_chain_audit_root();
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn test_restore_governance_chain_audit_events_restores_head_seq() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.restore_governance_chain_audit_events(vec![
            GovernanceChainAuditEvent {
                seq: 3,
                height: 2,
                proposal_id: 9,
                action: "execute".to_string(),
                actor: Some(0),
                outcome: "applied".to_string(),
                detail: "ok".to_string(),
            },
            GovernanceChainAuditEvent {
                seq: 1,
                height: 1,
                proposal_id: 9,
                action: "submit".to_string(),
                actor: Some(0),
                outcome: "accepted".to_string(),
                detail: "ok".to_string(),
            },
            GovernanceChainAuditEvent {
                seq: 1,
                height: 1,
                proposal_id: 9,
                action: "submit".to_string(),
                actor: Some(0),
                outcome: "accepted".to_string(),
                detail: "dup".to_string(),
            },
        ]);
        let events = protocol.governance_chain_audit_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].seq, 3);
        assert_eq!(events[1].action, "execute");
        assert_ne!(protocol.governance_chain_audit_root(), [0u8; 32]);
    }

    #[test]
    fn test_restore_governance_chain_audit_events_keeps_same_root() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set.clone(), 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let proposal = protocol
            .submit_governance_proposal(0, GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 15 })
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(executed);
        let events = protocol.governance_chain_audit_events();
        let root = protocol.governance_chain_audit_root();

        let mut restored = HotStuffProtocol::new(validator_set, 0).unwrap();
        restored.restore_governance_chain_audit_events(events);
        assert_eq!(restored.governance_chain_audit_root(), root);
    }

    #[test]
    fn test_governance_execute_update_network_dos_policy() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let target = NetworkDosPolicy {
            rpc_rate_limit_per_ip: 96,
            peer_ban_threshold: -6,
        };
        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateNetworkDosPolicy {
                    policy: target.clone(),
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(executed);
        assert_eq!(protocol.governance_network_dos_policy(), target);
    }

    #[test]
    fn test_governance_execute_update_token_economics_policy() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let target = TokenEconomicsPolicy {
            max_supply: 2_000_000,
            locked_supply: 1_000_000,
            fee_split: crate::types::FeeSplit {
                gas_base_burn_bp: 2_000,
                gas_to_node_bp: 3_000,
                service_burn_bp: 1_000,
                service_to_provider_bp: 4_000,
            },
        };
        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateTokenEconomicsPolicy {
                    policy: target.clone(),
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(executed);
        assert_eq!(protocol.governance_token_economics_policy(), target);
    }

    #[test]
    fn test_governance_execute_update_market_governance_policy() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let target = MarketGovernancePolicy {
            amm: crate::types::AmmGovernanceParams {
                swap_fee_bp: 45,
                lp_fee_share_bp: 8_200,
            },
            cdp: crate::types::CdpGovernanceParams {
                min_collateral_ratio_bp: 16_000,
                liquidation_threshold_bp: 13_000,
                liquidation_penalty_bp: 1_200,
                stability_fee_bp: 250,
                max_leverage_x100: 350,
            },
            bond: crate::types::BondGovernanceParams {
                coupon_rate_bp: 650,
                max_maturity_days: 540,
                min_issue_price_bp: 10_600,
            },
            reserve: crate::types::ReserveGovernanceParams {
                min_reserve_ratio_bp: 5_200,
                redemption_fee_bp: 80,
            },
            nav: crate::types::NavGovernanceParams {
                settlement_delay_epochs: 5,
                max_daily_redemption_bp: 1_200,
            },
            buyback: crate::types::BuybackGovernanceParams {
                trigger_discount_bp: 600,
                max_treasury_budget_per_epoch: 250_000,
                burn_share_bp: 6_000,
            },
        };
        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateMarketGovernancePolicy {
                    policy: target.clone(),
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(executed);
        assert_eq!(protocol.governance_market_policy(), target);
        let snap = protocol.governance_market_engine_snapshot();
        assert_eq!(snap.amm_swap_fee_bp, 45);
        assert_eq!(snap.cdp_min_collateral_ratio_bp, 16_000);
        assert_eq!(snap.bond_one_year_coupon_bp, 650);
        assert_eq!(snap.reserve_min_reserve_ratio_bp, 5_200);
        assert_eq!(snap.nav_settlement_delay_epochs, 5);
        assert_eq!(snap.buyback_trigger_discount_bp, 600);
    }

    #[test]
    fn test_market_policy_reconfigure_syncs_dividend_runtime_balances() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.mint_tokens(42, 500).unwrap();
        protocol
            .set_market_governance_policy(MarketGovernancePolicy::default())
            .unwrap();
        let snap = protocol.governance_market_engine_snapshot();
        assert!(snap.dividend_runtime_balance_accounts >= 1);
        assert!(snap.dividend_eligible_accounts >= 1);
    }

    #[test]
    fn test_governance_execute_treasury_spend() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        protocol
            .set_token_economics_policy(TokenEconomicsPolicy {
                max_supply: 1_000,
                locked_supply: 600,
                fee_split: crate::types::FeeSplit {
                    gas_base_burn_bp: 2_000,
                    gas_to_node_bp: 3_000,
                    service_burn_bp: 1_000,
                    service_to_provider_bp: 4_000,
                },
            })
            .unwrap();

        protocol.mint_tokens(42, 500).unwrap();
        protocol.charge_gas_fee(42, 100).unwrap();
        protocol.charge_service_fee(42, 100).unwrap();
        assert_eq!(protocol.token_treasury_balance(), 100);

        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::TreasurySpend {
                    to: 7,
                    amount: 80,
                    reason: "ecosystem_grant".to_string(),
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(executed);
        assert_eq!(protocol.token_treasury_balance(), 20);
        assert_eq!(protocol.token_balance(7), 80);
        assert_eq!(protocol.token_treasury_spent_total(), 80);

        let overspend = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::TreasurySpend {
                    to: 8,
                    amount: 999,
                    reason: "overspend_reject".to_string(),
                },
            )
            .unwrap();
        let overspend_votes = vec![
            GovernanceVote::new(&overspend, 0, true, &signing_keys[0]),
            GovernanceVote::new(&overspend, 1, true, &signing_keys[1]),
        ];
        assert!(protocol
            .execute_governance_proposal(overspend.proposal_id, &overspend_votes, &public_keys)
            .is_err());
    }

    #[test]
    fn test_token_mint_burn_and_fee_routing_rules() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol
            .set_token_economics_policy(TokenEconomicsPolicy {
                max_supply: 1_000,
                locked_supply: 600,
                fee_split: crate::types::FeeSplit {
                    gas_base_burn_bp: 2_000,
                    gas_to_node_bp: 3_000,
                    service_burn_bp: 1_000,
                    service_to_provider_bp: 4_000,
                },
            })
            .unwrap();

        assert!(protocol.mint_tokens(42, 0).is_err());
        protocol.mint_tokens(42, 500).unwrap();
        assert_eq!(protocol.token_total_supply(), 500);
        assert_eq!(protocol.token_locked_minted(), 500);
        assert_eq!(protocol.token_balance(42), 500);
        assert!(protocol.mint_tokens(42, 200).is_err()); // exceed locked remaining (100)

        let gas = protocol.charge_gas_fee(42, 100).unwrap();
        assert_eq!(gas.provider_amount, 30);
        assert_eq!(gas.treasury_amount, 50);
        assert_eq!(gas.burn_amount, 20);
        assert_eq!(protocol.token_balance(42), 400);
        assert_eq!(protocol.token_treasury_balance(), 50);
        assert_eq!(protocol.token_gas_provider_fee_pool(), 30);
        assert_eq!(protocol.token_burned_total(), 20);
        assert_eq!(protocol.token_total_supply(), 480);

        let service = protocol.charge_service_fee(42, 100).unwrap();
        assert_eq!(service.provider_amount, 40);
        assert_eq!(service.treasury_amount, 50);
        assert_eq!(service.burn_amount, 10);
        assert_eq!(protocol.token_balance(42), 300);
        assert_eq!(protocol.token_treasury_balance(), 100);
        assert_eq!(protocol.token_service_provider_fee_pool(), 40);
        assert_eq!(protocol.token_burned_total(), 30);
        assert_eq!(protocol.token_total_supply(), 470);

        protocol.burn_tokens(42, 100).unwrap();
        assert_eq!(protocol.token_balance(42), 200);
        assert_eq!(protocol.token_total_supply(), 370);
        assert_eq!(protocol.token_burned_total(), 130);
        assert!(protocol.burn_tokens(42, 300).is_err());
    }

    #[test]
    fn test_governance_execute_rejects_invalid_signature_and_duplicate_vote() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateSlashPolicy {
                    policy: SlashPolicy {
                        mode: SlashMode::ObserveOnly,
                        equivocation_threshold: 3,
                        min_active_validators: 2,
                        cooldown_epochs: 4,
                    },
                },
            )
            .unwrap();

        let bad_sig_votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[2]),
        ];
        let bad_sig = protocol.execute_governance_proposal(
            proposal.proposal_id,
            &bad_sig_votes,
            &public_keys,
        );
        assert!(matches!(bad_sig, Err(BFTError::InvalidSignature(_))));

        let dup_votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
        ];
        let dup =
            protocol.execute_governance_proposal(proposal.proposal_id, &dup_votes, &public_keys);
        assert!(matches!(dup, Err(BFTError::DuplicateVote(0))));
    }

    #[test]
    fn test_governance_execute_uses_configurable_vote_verifier_hook() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        assert_eq!(protocol.governance_vote_verifier_name(), "ed25519");
        assert_eq!(
            protocol.governance_vote_verifier_scheme(),
            GovernanceVoteVerifierScheme::Ed25519
        );
        protocol.set_governance_vote_verifier(Arc::new(RejectAllGovernanceVoteVerifier));
        assert_eq!(protocol.governance_vote_verifier_name(), "test_reject_all");
        assert_eq!(
            protocol.governance_vote_verifier_scheme(),
            GovernanceVoteVerifierScheme::Ed25519
        );

        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateSlashPolicy {
                    policy: SlashPolicy {
                        mode: SlashMode::ObserveOnly,
                        equivocation_threshold: 3,
                        min_active_validators: 2,
                        cooldown_epochs: 4,
                    },
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];

        let result =
            protocol.execute_governance_proposal(proposal.proposal_id, &votes, &public_keys);
        assert!(matches!(result, Err(BFTError::InvalidSignature(_))));
    }

    #[test]
    fn test_governance_execute_replay_rejected_after_first_apply() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateSlashPolicy {
                    policy: SlashPolicy {
                        mode: SlashMode::ObserveOnly,
                        equivocation_threshold: 2,
                        min_active_validators: 2,
                        cooldown_epochs: 5,
                    },
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let first = protocol
            .execute_governance_proposal(proposal.proposal_id, &votes, &public_keys)
            .unwrap();
        assert!(first);
        let replay =
            protocol.execute_governance_proposal(proposal.proposal_id, &votes, &public_keys);
        assert!(matches!(replay, Err(BFTError::Internal(_))));
    }

    #[test]
    fn test_governance_vote_domain_separation_rejects_height_or_digest_mismatch() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        let proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateSlashPolicy {
                    policy: SlashPolicy {
                        mode: SlashMode::ObserveOnly,
                        equivocation_threshold: 2,
                        min_active_validators: 2,
                        cooldown_epochs: 6,
                    },
                },
            )
            .unwrap();

        let mut bad_height_vote = GovernanceVote::new(&proposal, 0, true, &signing_keys[0]);
        bad_height_vote.proposal_height = bad_height_vote.proposal_height.saturating_add(1);
        let bad_height_votes = vec![
            bad_height_vote,
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let bad_height = protocol.execute_governance_proposal(
            proposal.proposal_id,
            &bad_height_votes,
            &public_keys,
        );
        assert!(matches!(bad_height, Err(BFTError::InvalidProposal(_))));

        let mut bad_digest_vote = GovernanceVote::new(&proposal, 0, true, &signing_keys[0]);
        bad_digest_vote.proposal_digest[0] ^= 0xAB;
        let bad_digest_votes = vec![
            bad_digest_vote,
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let bad_digest = protocol.execute_governance_proposal(
            proposal.proposal_id,
            &bad_digest_votes,
            &public_keys,
        );
        assert!(matches!(bad_digest, Err(BFTError::InvalidProposal(_))));
    }

    #[test]
    fn test_governance_access_policy_multisig_and_timelock() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let (signing_keys, public_keys) = generate_keys(3);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        protocol
            .set_governance_access_policy(GovernanceAccessPolicy {
                proposer_committee: vec![0, 1],
                proposer_threshold: 2,
                executor_committee: vec![1, 2],
                executor_threshold: 2,
                timelock_epochs: 2,
            })
            .unwrap();

        // proposer multisig must satisfy threshold=2
        let submit_fail = protocol.submit_governance_proposal_with_approvals(
            0,
            &[0],
            GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 9 },
        );
        assert!(submit_fail.is_err());

        let proposal = protocol
            .submit_governance_proposal_with_approvals(
                0,
                &[0, 1],
                GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 9 },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];

        // timelock blocks execution at current height
        let timelock_fail = protocol.execute_governance_proposal_with_executor_approvals(
            proposal.proposal_id,
            &votes,
            &public_keys,
            &[1, 2],
        );
        assert!(matches!(timelock_fail, Err(BFTError::InvalidProposal(_))));

        // relax timelock, keep executor multisig
        protocol
            .set_governance_access_policy(GovernanceAccessPolicy {
                proposer_committee: vec![0, 1],
                proposer_threshold: 2,
                executor_committee: vec![1, 2],
                executor_threshold: 2,
                timelock_epochs: 0,
            })
            .unwrap();

        let execute_fail = protocol.execute_governance_proposal_with_executor_approvals(
            proposal.proposal_id,
            &votes,
            &public_keys,
            &[1],
        );
        assert!(execute_fail.is_err());

        let execute_ok = protocol
            .execute_governance_proposal_with_executor_approvals(
                proposal.proposal_id,
                &votes,
                &public_keys,
                &[1, 2],
            )
            .unwrap();
        assert!(execute_ok);
        assert_eq!(protocol.governance_mempool_fee_floor(), 9);
    }

    #[test]
    fn test_governance_council_weighted_thresholds() {
        let validator_set = ValidatorSet::new_equal_weight((0..9).collect());
        let (signing_keys, public_keys) = generate_keys(9);
        let mut protocol = HotStuffProtocol::new(validator_set, 0).unwrap();
        protocol.set_governance_execution_enabled(true);
        protocol
            .set_governance_access_policy(GovernanceAccessPolicy {
                proposer_committee: (0..9).collect(),
                proposer_threshold: 1,
                executor_committee: (0..9).collect(),
                executor_threshold: 1,
                timelock_epochs: 0,
            })
            .unwrap();
        protocol
            .set_governance_council_policy(GovernanceCouncilPolicy {
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
            })
            .unwrap();

        // parameter_change threshold: >5000
        let parameter_proposal = protocol
            .submit_governance_proposal(0, GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 17 })
            .unwrap();
        let parameter_low_votes = vec![
            GovernanceVote::new(&parameter_proposal, 0, true, &signing_keys[0]), // 3500
            GovernanceVote::new(&parameter_proposal, 1, true, &signing_keys[1]), // +1000 => 4500
        ];
        let parameter_low = protocol.execute_governance_proposal(
            parameter_proposal.proposal_id,
            &parameter_low_votes,
            &public_keys,
        );
        assert!(matches!(
            parameter_low,
            Err(BFTError::InsufficientVotes { .. })
        ));
        let parameter_ok_votes = vec![
            GovernanceVote::new(&parameter_proposal, 0, true, &signing_keys[0]), // 3500
            GovernanceVote::new(&parameter_proposal, 1, true, &signing_keys[1]), // +1000
            GovernanceVote::new(&parameter_proposal, 2, true, &signing_keys[2]), // +1000 => 5500
        ];
        let parameter_ok = protocol
            .execute_governance_proposal(
                parameter_proposal.proposal_id,
                &parameter_ok_votes,
                &public_keys,
            )
            .unwrap();
        assert!(parameter_ok);
        assert_eq!(protocol.governance_mempool_fee_floor(), 17);

        // protocol_upgrade threshold: >7500
        let target_access = GovernanceAccessPolicy {
            proposer_committee: vec![0, 1],
            proposer_threshold: 2,
            executor_committee: vec![0, 1, 2],
            executor_threshold: 2,
            timelock_epochs: 1,
        };
        let upgrade_proposal = protocol
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateGovernanceAccessPolicy {
                    policy: target_access.clone(),
                },
            )
            .unwrap();
        let upgrade_low_votes = vec![
            GovernanceVote::new(&upgrade_proposal, 0, true, &signing_keys[0]), // 3500
            GovernanceVote::new(&upgrade_proposal, 1, true, &signing_keys[1]), // +1000
            GovernanceVote::new(&upgrade_proposal, 2, true, &signing_keys[2]), // +1000
            GovernanceVote::new(&upgrade_proposal, 3, true, &signing_keys[3]), // +1000 => 6500
        ];
        let upgrade_low = protocol.execute_governance_proposal(
            upgrade_proposal.proposal_id,
            &upgrade_low_votes,
            &public_keys,
        );
        assert!(matches!(
            upgrade_low,
            Err(BFTError::InsufficientVotes { .. })
        ));
        let upgrade_ok_votes = vec![
            GovernanceVote::new(&upgrade_proposal, 0, true, &signing_keys[0]), // 3500
            GovernanceVote::new(&upgrade_proposal, 1, true, &signing_keys[1]), // +1000
            GovernanceVote::new(&upgrade_proposal, 2, true, &signing_keys[2]), // +1000
            GovernanceVote::new(&upgrade_proposal, 3, true, &signing_keys[3]), // +1000
            GovernanceVote::new(&upgrade_proposal, 4, true, &signing_keys[4]), // +1000
            GovernanceVote::new(&upgrade_proposal, 5, true, &signing_keys[5]), // +1000 => 8500
        ];
        let upgrade_ok = protocol
            .execute_governance_proposal(
                upgrade_proposal.proposal_id,
                &upgrade_ok_votes,
                &public_keys,
            )
            .unwrap();
        assert!(upgrade_ok);
        assert_eq!(protocol.governance_access_policy(), target_access);
    }
}
