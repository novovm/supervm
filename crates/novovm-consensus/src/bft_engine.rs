// Phase 4.2: BFT Engine
//
// BFT 引擎：协调 Epoch 管理、HotStuff 协议、投票聚合

use crate::epoch::{Epoch, EpochConfig, EpochManager};
use crate::protocol::{HotStuffProtocol, Phase};
use crate::quorum_cert::{QuorumCertificate, Vote};
use crate::types::{
    BFTError, BFTProposal, BFTResult, FeeRoutingOutcome, GovernanceAccessPolicy, GovernanceOp,
    GovernanceProposal, GovernanceVote, NetworkDosPolicy, NodeId, SlashEvidence, SlashExecution,
    SlashPolicy, TokenEconomicsPolicy, ValidatorSet,
};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default)]
pub struct CommitQcTimings {
    pub verify: Duration,
    pub commit: Duration,
}

/// BFT 引擎配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BFTConfig {
    /// Epoch 配置
    pub epoch_config: EpochConfig,

    /// 超时时间（毫秒）
    pub timeout_ms: u64,

    /// 是否启用快速路径（跳过 PreCommit）
    pub enable_fast_path: bool,
}

impl Default for BFTConfig {
    fn default() -> Self {
        Self {
            epoch_config: EpochConfig::default(),
            timeout_ms: 5000,
            enable_fast_path: false,
        }
    }
}

/// 已提交的 Epoch 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommittedEpoch {
    pub epoch: Epoch,
    pub qc: QuorumCertificate,
    pub commit_time: u64,
}

/// BFT 引擎
pub struct BFTEngine {
    /// 配置
    #[allow(dead_code)]
    config: BFTConfig,

    /// 本节点 ID
    self_id: NodeId,

    /// 签名密钥
    signing_key: SigningKey,

    /// 公钥映射（所有验证者）
    public_keys: HashMap<NodeId, VerifyingKey>,

    /// Epoch 管理器
    epoch_manager: Arc<Mutex<EpochManager>>,

    /// HotStuff 协议
    protocol: Arc<Mutex<HotStuffProtocol>>,

    /// 已提交的 Epoch
    committed_epochs: Vec<CommittedEpoch>,

    /// Last commit_qc() timings (best-effort; set on successful commit).
    last_commit_qc_timings: Option<CommitQcTimings>,
}

impl BFTEngine {
    /// 创建新的 BFT 引擎
    pub fn new(
        config: BFTConfig,
        self_id: NodeId,
        signing_key: SigningKey,
        validator_set: ValidatorSet,
        public_keys: HashMap<NodeId, VerifyingKey>,
    ) -> BFTResult<Self> {
        let epoch_manager = Arc::new(Mutex::new(EpochManager::new(config.epoch_config.clone())));
        let protocol = Arc::new(Mutex::new(HotStuffProtocol::new(validator_set, self_id)?));

        Ok(Self {
            config,
            self_id,
            signing_key,
            public_keys,
            epoch_manager,
            protocol,
            committed_epochs: Vec::new(),
            last_commit_qc_timings: None,
        })
    }

    pub fn last_commit_qc_timings(&self) -> Option<CommitQcTimings> {
        self.last_commit_qc_timings
    }

    /// 启动新的 Epoch
    pub fn start_epoch(&self) -> BFTResult<Epoch> {
        let mut manager = self.epoch_manager.lock().unwrap();
        let epoch = manager.start_epoch(self.self_id);
        Ok(epoch.clone())
    }

    /// 检查是否有活跃的 Epoch
    pub fn has_active_epoch(&self) -> bool {
        let mut manager = self.epoch_manager.lock().unwrap();
        manager.current_epoch_mut().is_some()
    }

    /// 添加 Batch 到当前 Epoch
    pub fn add_batch(&self, batch_id: u64, tx_count: u64) -> BFTResult<()> {
        let mut manager = self.epoch_manager.lock().unwrap();

        if let Some(epoch) = manager.current_epoch_mut() {
            epoch.add_batch(batch_id, tx_count);
            Ok(())
        } else {
            Err(BFTError::Internal("No active epoch".to_string()))
        }
    }

    /// 检查是否应该关闭 Epoch
    pub fn should_close_epoch(&self) -> bool {
        let manager = self.epoch_manager.lock().unwrap();
        manager.should_close_epoch()
    }

    /// 关闭当前 Epoch 并提出提案（Leader 执行）
    pub fn propose_epoch(&self, batch_results: &HashMap<u64, [u8; 32]>) -> BFTResult<BFTProposal> {
        let mut manager = self.epoch_manager.lock().unwrap();

        // 获取当前 Epoch
        let epoch = manager
            .current_epoch_mut()
            .ok_or_else(|| BFTError::Internal("No active epoch".to_string()))?;

        // 计算状态根
        epoch.compute_state_root(batch_results)?;

        // 提交 Epoch
        let committed_epoch = manager.commit_current_epoch()?;
        drop(manager);

        // 通过协议提出提案
        let mut protocol = self.protocol.lock().unwrap();
        let proposal = protocol.propose(&committed_epoch)?;

        Ok(proposal)
    }

    /// 关闭当前 Epoch 并提出提案（Leader 执行），但使用执行层提供的状态承诺根覆盖 epoch.state_root。
    ///
    /// 设计目的：以“最小切口”把执行层（例如 AOEM 的 `__state_root__`）接入共识提案，
    /// 且不改变现有 batch_results 的收集/校验流程。
    pub fn propose_epoch_with_state_root(
        &self,
        batch_results: &HashMap<u64, [u8; 32]>,
        state_root: [u8; 32],
    ) -> BFTResult<BFTProposal> {
        let mut manager = self.epoch_manager.lock().unwrap();

        // 获取当前 Epoch
        let epoch = manager
            .current_epoch_mut()
            .ok_or_else(|| BFTError::Internal("No active epoch".to_string()))?;

        // 保留原流程：确保 batch_results 完整可用（避免隐藏上层 bug）。
        epoch.compute_state_root(batch_results)?;

        // 执行层覆盖：共识提案里承载“真实状态承诺根”。
        epoch.state_root = state_root;

        // 提交 Epoch
        let committed_epoch = manager.commit_current_epoch()?;
        drop(manager);

        // 通过协议提出提案
        let mut protocol = self.protocol.lock().unwrap();
        let proposal = protocol.propose(&committed_epoch)?;
        Ok(proposal)
    }

    /// 对提案投票（Validator 执行）
    pub fn vote_for_proposal(&self, proposal: &BFTProposal) -> BFTResult<Vote> {
        let mut protocol = self.protocol.lock().unwrap();
        let vote = protocol.vote(proposal, &self.signing_key)?;
        Ok(vote)
    }

    /// 收集投票（Leader 执行）
    pub fn collect_vote(&self, vote: Vote) -> BFTResult<Option<QuorumCertificate>> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.collect_vote(vote)
    }

    /// 获取当前的 QC（如果已达法定人数）
    pub fn get_current_qc(&self) -> Option<QuorumCertificate> {
        let protocol = self.protocol.lock().unwrap();
        protocol.get_quorum_certificate()
    }

    /// 提交 QC（所有节点执行）
    pub fn commit_qc(&mut self, qc: QuorumCertificate) -> BFTResult<CommittedEpoch> {
        if cfg!(test) {
            eprintln!(
                "[BFTEngine] commit_qc: begin height={} votes={}",
                qc.height,
                qc.votes.len()
            );
        }
        // 验证 QC
        let protocol = self.protocol.lock().unwrap();
        let validator_set = protocol.validator_set();
        if cfg!(test) {
            eprintln!(
                "[BFTEngine] commit_qc: verifying qc (quorum={})",
                validator_set.quorum_size()
            );
        }

        let verify_start = Instant::now();
        qc.verify(validator_set, &self.public_keys)?;
        let verify_dur = verify_start.elapsed();
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: qc verified");
        }
        drop(protocol);

        // 提交
        let commit_start = Instant::now();
        let mut protocol = self.protocol.lock().unwrap();
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: pre_commit");
        }
        protocol.pre_commit(&qc)?;
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: commit");
        }
        protocol.commit()?;
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: protocol committed");
        }

        // 获取最后提交的 Epoch
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: locking epoch_manager");
        }
        let manager = self.epoch_manager.lock().unwrap();
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: epoch_manager locked, reading last_committed_epoch");
        }
        let epoch = manager
            .last_committed_epoch()
            .ok_or_else(|| BFTError::Internal("No committed epoch".to_string()))?
            .clone();
        if cfg!(test) {
            eprintln!(
                "[BFTEngine] commit_qc: got committed epoch id={} height={}",
                epoch.id, epoch.height
            );
        }

        let committed = CommittedEpoch {
            epoch,
            qc,
            commit_time: current_timestamp_ms(),
        };

        self.committed_epochs.push(committed.clone());
        if cfg!(test) {
            eprintln!(
                "[BFTEngine] commit_qc: done (total_committed_epochs={})",
                self.committed_epochs.len()
            );
        }

        let commit_dur = commit_start.elapsed();
        self.last_commit_qc_timings = Some(CommitQcTimings {
            verify: verify_dur,
            commit: commit_dur,
        });

        Ok(committed)
    }

    /// 获取当前高度
    pub fn current_height(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.current_height()
    }

    /// 获取当前 view（同高度换主计数）
    pub fn current_view(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.current_view()
    }

    /// 获取法定人数大小（用于协调器早期判断是否可达成法定票数）
    pub fn quorum_size(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.validator_set().quorum_size()
    }

    /// 检查是否是 Leader
    pub fn is_leader(&self) -> bool {
        let protocol = self.protocol.lock().unwrap();
        protocol.is_leader()
    }

    /// 触发超时换主（view-change）
    pub fn trigger_view_change(&self) -> BFTResult<NodeId> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.trigger_view_change()
    }

    /// Fork-choice：在候选 QC 中过滤无效项后，选择最佳 QC。
    pub fn select_best_qc(&self, candidates: &[QuorumCertificate]) -> BFTResult<QuorumCertificate> {
        if candidates.is_empty() {
            return Err(BFTError::Internal(
                "fork choice candidates cannot be empty".to_string(),
            ));
        }

        let protocol = self.protocol.lock().unwrap();
        let validator_set = protocol.validator_set().clone();
        let mut valid_candidates = Vec::new();
        for qc in candidates {
            if qc.verify(&validator_set, &self.public_keys).is_ok() {
                valid_candidates.push(qc.clone());
            }
        }
        if valid_candidates.is_empty() {
            return Err(BFTError::Internal(
                "fork choice has no valid qc candidate".to_string(),
            ));
        }
        let selected = protocol.select_fork_choice(&valid_candidates)?;
        drop(protocol);
        Ok(selected)
    }

    /// 获取协议已记录的 slash 证据。
    pub fn slash_evidences(&self) -> Vec<SlashEvidence> {
        let protocol = self.protocol.lock().unwrap();
        protocol.slash_evidences().to_vec()
    }

    /// 获取已执行的 slash 记录（evidence -> jailed/weight 生效）。
    pub fn slash_executions(&self) -> Vec<SlashExecution> {
        let protocol = self.protocol.lock().unwrap();
        protocol.slash_executions().to_vec()
    }

    /// 查询验证者是否已被 jailed。
    pub fn is_validator_jailed(&self, node_id: NodeId) -> bool {
        let protocol = self.protocol.lock().unwrap();
        protocol.is_validator_jailed(node_id)
    }

    /// 查询验证者当前 jail 自动解禁高度（仅当仍在 jailed 状态时返回）。
    pub fn validator_jailed_until_epoch(&self, node_id: NodeId) -> Option<u64> {
        let protocol = self.protocol.lock().unwrap();
        protocol.validator_jailed_until_epoch(node_id)
    }

    /// 当前活跃验证者法定权重（动态扣除已 jailed 验证者）。
    pub fn active_quorum_size(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.active_quorum_size()
    }

    /// 更新罚没治理策略（参数化 slash execution）。
    pub fn set_slash_policy(&self, policy: SlashPolicy) -> BFTResult<()> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.set_slash_policy(policy)
    }

    /// 获取当前罚没治理策略。
    pub fn slash_policy(&self) -> SlashPolicy {
        let protocol = self.protocol.lock().unwrap();
        protocol.slash_policy()
    }

    /// 治理操作挂点（当前 staged-only，不启用链上执行）。
    pub fn stage_governance_op(&self, op: GovernanceOp) -> BFTResult<()> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.stage_governance_op(op)
    }

    /// 查询 staged 治理操作列表。
    pub fn staged_governance_ops(&self) -> Vec<GovernanceOp> {
        let protocol = self.protocol.lock().unwrap();
        protocol.staged_governance_ops().to_vec()
    }

    /// 开关治理执行（默认关闭）。
    pub fn set_governance_execution_enabled(&self, enabled: bool) {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.set_governance_execution_enabled(enabled);
    }

    /// 查询治理执行是否已开启。
    pub fn governance_execution_enabled(&self) -> bool {
        let protocol = self.protocol.lock().unwrap();
        protocol.governance_execution_enabled()
    }

    /// 提交治理提案（最小闭环）。
    pub fn submit_governance_proposal(
        &self,
        proposer: NodeId,
        op: GovernanceOp,
    ) -> BFTResult<GovernanceProposal> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.submit_governance_proposal(proposer, op)
    }

    /// 提交治理提案（委员会阈值模型）。
    pub fn submit_governance_proposal_with_approvals(
        &self,
        proposer: NodeId,
        proposer_approvals: &[NodeId],
        op: GovernanceOp,
    ) -> BFTResult<GovernanceProposal> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.submit_governance_proposal_with_approvals(proposer, proposer_approvals, op)
    }

    /// 执行治理提案（签名投票 + weighted quorum）。
    pub fn execute_governance_proposal(
        &self,
        proposal_id: u64,
        votes: &[GovernanceVote],
    ) -> BFTResult<bool> {
        self.execute_governance_proposal_with_executor_approvals(
            proposal_id,
            votes,
            &[self.self_id],
        )
    }

    /// 执行治理提案（委员会阈值 + 时间锁 + 签名投票）。
    pub fn execute_governance_proposal_with_executor_approvals(
        &self,
        proposal_id: u64,
        votes: &[GovernanceVote],
        executor_approvals: &[NodeId],
    ) -> BFTResult<bool> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.execute_governance_proposal_with_executor_approvals(
            proposal_id,
            votes,
            &self.public_keys,
            executor_approvals,
        )
    }

    /// 读取治理访问策略。
    pub fn governance_access_policy(&self) -> GovernanceAccessPolicy {
        let protocol = self.protocol.lock().unwrap();
        protocol.governance_access_policy()
    }

    /// 更新治理访问策略。
    pub fn set_governance_access_policy(&self, policy: GovernanceAccessPolicy) -> BFTResult<()> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.set_governance_access_policy(policy)
    }

    /// 读取治理参数：mempool fee floor。
    pub fn governance_mempool_fee_floor(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.governance_mempool_fee_floor()
    }

    /// 读取治理参数：network dos policy。
    pub fn governance_network_dos_policy(&self) -> NetworkDosPolicy {
        let protocol = self.protocol.lock().unwrap();
        protocol.governance_network_dos_policy()
    }

    /// 读取治理参数：token economics policy。
    pub fn governance_token_economics_policy(&self) -> TokenEconomicsPolicy {
        let protocol = self.protocol.lock().unwrap();
        protocol.governance_token_economics_policy()
    }

    /// 更新 token economics policy（运行期可由治理层下发）。
    pub fn set_token_economics_policy(&self, policy: TokenEconomicsPolicy) -> BFTResult<()> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.set_token_economics_policy(policy)
    }

    /// Token mint（I-TOKEN 最小主链路）。
    pub fn mint_tokens(&self, account: NodeId, amount: u64) -> BFTResult<()> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.mint_tokens(account, amount)
    }

    /// Token burn（I-TOKEN 最小主链路）。
    pub fn burn_tokens(&self, account: NodeId, amount: u64) -> BFTResult<()> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.burn_tokens(account, amount)
    }

    /// gas fee 路由（provider/treasury/burn）。
    pub fn charge_gas_fee(&self, payer: NodeId, amount: u64) -> BFTResult<FeeRoutingOutcome> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.charge_gas_fee(payer, amount)
    }

    /// service fee 路由（provider/treasury/burn）。
    pub fn charge_service_fee(&self, payer: NodeId, amount: u64) -> BFTResult<FeeRoutingOutcome> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.charge_service_fee(payer, amount)
    }

    /// treasury spend（治理执行面）：从 treasury 主账户转账给目标账户。
    pub fn spend_treasury_tokens(&self, to: NodeId, amount: u64, reason: &str) -> BFTResult<()> {
        let mut protocol = self.protocol.lock().unwrap();
        protocol.spend_treasury_tokens(to, amount, reason)
    }

    pub fn token_balance(&self, account: NodeId) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.token_balance(account)
    }

    pub fn token_total_supply(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.token_total_supply()
    }

    pub fn token_locked_minted(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.token_locked_minted()
    }

    pub fn token_treasury_balance(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.token_treasury_balance()
    }

    pub fn token_burned_total(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.token_burned_total()
    }

    pub fn token_treasury_spent_total(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.token_treasury_spent_total()
    }

    pub fn token_gas_provider_fee_pool(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.token_gas_provider_fee_pool()
    }

    pub fn token_service_provider_fee_pool(&self) -> u64 {
        let protocol = self.protocol.lock().unwrap();
        protocol.token_service_provider_fee_pool()
    }

    /// 查询单个待执行治理提案。
    pub fn governance_pending_proposal(&self, proposal_id: u64) -> Option<GovernanceProposal> {
        let protocol = self.protocol.lock().unwrap();
        protocol.governance_pending_proposal(proposal_id)
    }

    /// 查询全部待执行治理提案。
    pub fn governance_pending_proposals(&self) -> Vec<GovernanceProposal> {
        let protocol = self.protocol.lock().unwrap();
        protocol.governance_pending_proposals()
    }

    /// 获取当前阶段
    pub fn current_phase(&self) -> Phase {
        let protocol = self.protocol.lock().unwrap();
        protocol.current_phase()
    }

    /// 获取已提交 Epoch 总数
    pub fn total_committed_epochs(&self) -> usize {
        self.committed_epochs.len()
    }

    /// Returns the state_root of the last committed epoch (if any).
    ///
    /// This is a read-only helper for adapters/tests to verify that an execution-layer
    /// state commitment (e.g. AOEM `__state_root__`) is actually carried through consensus.
    pub fn last_committed_state_root(&self) -> Option<[u8; 32]> {
        self.committed_epochs.last().map(|c| c.epoch.state_root)
    }

    /// 计算总 TPS
    pub fn compute_total_tps(&self) -> f64 {
        if self.committed_epochs.is_empty() {
            return 0.0;
        }

        let total_txs: u64 = self
            .committed_epochs
            .iter()
            .map(|ce| ce.epoch.total_txs)
            .sum();

        let first_start = self.committed_epochs[0].epoch.start_time;
        let last_commit = self.committed_epochs.last().unwrap().commit_time;

        let duration_sec = (last_commit - first_start) as f64 / 1000.0;
        if duration_sec > 0.0 {
            total_txs as f64 / duration_sec
        } else {
            0.0
        }
    }
}

fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_bft_engine_full_workflow() {
        // 设置 4 个验证者
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);

        // 生成密钥
        let signing_keys: Vec<_> = (0..4).map(|_| SigningKey::generate(&mut OsRng)).collect();

        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();

        // 创建 Leader 引擎（节点 0）
        let config = BFTConfig::default();
        let mut leader_engine = BFTEngine::new(
            config.clone(),
            0,
            signing_keys[0].clone(),
            validator_set.clone(),
            public_keys.clone(),
        )
        .unwrap();

        assert!(leader_engine.is_leader());

        // 启动 Epoch
        leader_engine.start_epoch().unwrap();

        // 添加 Batch
        leader_engine.add_batch(1, 100).unwrap();
        leader_engine.add_batch(2, 150).unwrap();

        // 提出提案
        let mut batch_results = HashMap::new();
        batch_results.insert(1, [1u8; 32]);
        batch_results.insert(2, [2u8; 32]);

        let proposal = leader_engine.propose_epoch(&batch_results).unwrap();

        // 其他验证者投票（需要同步协议状态）
        let mut votes = Vec::new();
        {
            let leader_protocol = leader_engine.protocol.lock().unwrap();
            let leader_state = leader_protocol.get_state();

            for i in 0..4 {
                let voter_engine = BFTEngine::new(
                    config.clone(),
                    i,
                    signing_keys[i as usize].clone(),
                    validator_set.clone(),
                    public_keys.clone(),
                )
                .unwrap();

                // 同步协议状态到 Vote 阶段
                {
                    let mut voter_protocol = voter_engine.protocol.lock().unwrap();
                    voter_protocol.sync_state(leader_state.clone());
                }

                let vote = voter_engine.vote_for_proposal(&proposal).unwrap();
                votes.push(vote);
            }
        }

        // Leader 收集投票
        let mut qc_opt = None;
        for vote in votes {
            if let Some(qc) = leader_engine.collect_vote(vote).unwrap() {
                qc_opt = Some(qc);
                break;
            }
        }

        assert!(qc_opt.is_some());

        // 提交 QC
        let qc = qc_opt.unwrap();
        let committed = leader_engine.commit_qc(qc).unwrap();

        assert_eq!(committed.epoch.total_txs, 250);
        assert_eq!(leader_engine.total_committed_epochs(), 1);
    }

    #[test]
    fn test_propose_epoch_with_state_root_override() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);

        let signing_keys: Vec<_> = (0..4).map(|_| SigningKey::generate(&mut OsRng)).collect();

        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();

        let config = BFTConfig::default();
        let leader_engine = BFTEngine::new(
            config,
            0,
            signing_keys[0].clone(),
            validator_set,
            public_keys,
        )
        .unwrap();

        leader_engine.start_epoch().unwrap();
        leader_engine.add_batch(1, 100).unwrap();
        leader_engine.add_batch(2, 150).unwrap();

        let mut batch_results = HashMap::new();
        batch_results.insert(1, [1u8; 32]);
        batch_results.insert(2, [2u8; 32]);

        let override_root = [9u8; 32];
        let proposal = leader_engine
            .propose_epoch_with_state_root(&batch_results, override_root)
            .unwrap();

        assert_eq!(proposal.state_delta_hash, override_root);
    }

    #[test]
    fn test_view_change_moves_leader_by_view() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let signing_keys: Vec<_> = (0..4).map(|_| SigningKey::generate(&mut OsRng)).collect();
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();

        let engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set,
            public_keys,
        )
        .unwrap();
        assert!(engine.is_leader());
        assert_eq!(engine.current_view(), 0);

        let next = engine.trigger_view_change().unwrap();
        assert_eq!(next, 1);
        assert!(!engine.is_leader());
        assert_eq!(engine.current_view(), 1);
    }

    #[test]
    fn test_select_best_qc_prefers_highest_valid_height() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let signing_keys: Vec<_> = (0..4).map(|_| SigningKey::generate(&mut OsRng)).collect();
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();

        let engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set.clone(),
            public_keys.clone(),
        )
        .unwrap();

        let mut qc_low = QuorumCertificate::new([3u8; 32], 10);
        for i in 0..3usize {
            let vote = Vote::new(i as NodeId, [3u8; 32], 10, &signing_keys[i]);
            qc_low.add_vote(vote, 1);
        }

        let mut qc_high = QuorumCertificate::new([7u8; 32], 11);
        for i in 0..3usize {
            let vote = Vote::new(i as NodeId, [7u8; 32], 11, &signing_keys[i]);
            qc_high.add_vote(vote, 1);
        }

        let mut qc_invalid = qc_high.clone();
        qc_invalid.total_weight = 99; // declared weight tamper -> invalid

        // protocol fork-choice会先选更高高度项；engine 会对选中项再次校验签名/权重。
        let best = engine
            .select_best_qc(&[qc_low.clone(), qc_high.clone()])
            .unwrap();
        assert_eq!(best.height, 11);
        assert_eq!(best.total_weight, 3);

        let fallback_best = engine
            .select_best_qc(&[qc_low.clone(), qc_invalid])
            .unwrap();
        assert_eq!(fallback_best.height, 10);
        assert_eq!(fallback_best.total_weight, 3);

        // 手动调用协议层选择后，组合一个仅含有效候选的路径仍然可用。
        assert!(qc_low.verify(&validator_set, &public_keys).is_ok());
    }

    #[test]
    fn test_stage_governance_op_is_staged_only() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1]);
        let signing_keys: Vec<_> = (0..2).map(|_| SigningKey::generate(&mut OsRng)).collect();
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();

        let engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set,
            public_keys,
        )
        .unwrap();
        let before = engine.slash_policy();
        let op = GovernanceOp::UpdateSlashPolicy {
            policy: SlashPolicy {
                mode: crate::types::SlashMode::ObserveOnly,
                equivocation_threshold: 3,
                min_active_validators: 1,
                cooldown_epochs: 8,
            },
        };
        let staged = engine.stage_governance_op(op.clone());
        assert!(matches!(staged, Err(BFTError::GovernanceNotEnabled(_))));
        assert_eq!(engine.staged_governance_ops(), vec![op]);
        assert_eq!(engine.slash_policy(), before);
    }

    #[test]
    fn test_execute_governance_update_slash_policy() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();
        let engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set,
            public_keys,
        )
        .unwrap();

        engine.set_governance_execution_enabled(true);
        assert!(engine.governance_execution_enabled());
        let target_policy = SlashPolicy {
            mode: crate::types::SlashMode::ObserveOnly,
            equivocation_threshold: 3,
            min_active_validators: 2,
            cooldown_epochs: 4,
        };
        let proposal = engine
            .submit_governance_proposal(
                0,
                GovernanceOp::UpdateSlashPolicy {
                    policy: target_policy.clone(),
                },
            )
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = engine
            .execute_governance_proposal(proposal.proposal_id, &votes)
            .unwrap();
        assert!(executed);
        assert_eq!(engine.slash_policy(), target_policy);
    }

    #[test]
    fn test_execute_governance_update_mempool_fee_floor() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();
        let engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set,
            public_keys,
        )
        .unwrap();
        engine.set_governance_execution_enabled(true);
        let proposal = engine
            .submit_governance_proposal(0, GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 11 })
            .unwrap();
        let votes = vec![
            GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
            GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
        ];
        let executed = engine
            .execute_governance_proposal(proposal.proposal_id, &votes)
            .unwrap();
        assert!(executed);
        assert_eq!(engine.governance_mempool_fee_floor(), 11);
    }

    #[test]
    fn test_execute_governance_update_network_dos_policy() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
        let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();
        let engine = BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set,
            public_keys,
        )
        .unwrap();
        engine.set_governance_execution_enabled(true);
        let target = NetworkDosPolicy {
            rpc_rate_limit_per_ip: 80,
            peer_ban_threshold: -7,
        };
        let proposal = engine
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
        let executed = engine
            .execute_governance_proposal(proposal.proposal_id, &votes)
            .unwrap();
        assert!(executed);
        assert_eq!(engine.governance_network_dos_policy(), target);
    }
}
