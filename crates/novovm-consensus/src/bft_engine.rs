// Phase 4.2: BFT Engine
//
// BFT 引擎：协调 Epoch 管理、HotStuff 协议、投票聚合

use crate::types::{BFTProposal, BFTResult, BFTError, ValidatorSet, NodeId};
use crate::epoch::{Epoch, EpochConfig, EpochManager};
use crate::quorum_cert::{QuorumCertificate, Vote};
use crate::protocol::{HotStuffProtocol, Phase};
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
        let epoch = manager.current_epoch_mut()
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
            eprintln!("[BFTEngine] commit_qc: begin height={} votes={}", qc.height, qc.votes.len());
        }
        // 验证 QC
        let protocol = self.protocol.lock().unwrap();
        let validator_set = protocol.validator_set();
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: verifying qc (quorum={})", validator_set.quorum_size());
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
        let epoch = manager.last_committed_epoch()
            .ok_or_else(|| BFTError::Internal("No committed epoch".to_string()))?
            .clone();
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: got committed epoch id={} height={}", epoch.id, epoch.height);
        }
        
        let committed = CommittedEpoch {
            epoch,
            qc,
            commit_time: current_timestamp_ms(),
        };
        
        self.committed_epochs.push(committed.clone());
        if cfg!(test) {
            eprintln!("[BFTEngine] commit_qc: done (total_committed_epochs={})", self.committed_epochs.len());
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
        self.committed_epochs
            .last()
            .map(|c| c.epoch.state_root)
    }
    
    /// 计算总 TPS
    pub fn compute_total_tps(&self) -> f64 {
        if self.committed_epochs.is_empty() {
            return 0.0;
        }
        
        let total_txs: u64 = self.committed_epochs.iter()
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
        let signing_keys: Vec<_> = (0..4)
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();
        
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
        ).unwrap();
        
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
                ).unwrap();
                
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

        let signing_keys: Vec<_> = (0..4)
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();

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
}

