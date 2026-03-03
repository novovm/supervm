// Phase 4.2: HotStuff-2 Protocol Implementation
//
// 简化版 HotStuff-2 协议（3-chain rule）
// Propose → Vote → PreCommit → Commit

use crate::types::{BFTProposal, BFTResult, BFTError, ValidatorSet, NodeId, Height};
use crate::quorum_cert::{QuorumCertificate, Vote};
use crate::epoch::Epoch;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

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
            leader_id: initial_leader,
            active_proposal: None,
            votes: Vec::new(),
            last_qc: None,
        }
    }
    
    /// 进入下一个高度
    pub fn advance_height(&mut self, new_leader: NodeId) {
        self.height += 1;
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
}

impl HotStuffProtocol {
    /// 创建新的协议实例
    pub fn new(
        validator_set: ValidatorSet,
        self_id: NodeId,
    ) -> BFTResult<Self> {
        if !validator_set.is_validator(self_id) {
            return Err(BFTError::NotValidator(self_id));
        }
        
        let initial_leader = validator_set.validators[0];
        
        Ok(Self {
            validator_set,
            self_id,
            state: ProtocolState::new(initial_leader),
        })
    }
    
    /// 检查是否是当前 Leader
    pub fn is_leader(&self) -> bool {
        self.state.leader_id == self.self_id
    }
    
    /// 获取当前高度
    pub fn current_height(&self) -> Height {
        self.state.height
    }
    
    /// 获取当前阶段
    pub fn current_phase(&self) -> Phase {
        self.state.phase
    }
    
    /// Propose 阶段：Leader 提出新提案
    pub fn propose(&mut self, epoch: &Epoch) -> BFTResult<BFTProposal> {
        if !self.is_leader() {
            return Err(BFTError::Internal(
                "Only leader can propose".to_string()
            ));
        }
        
        if self.state.phase != Phase::Propose {
            return Err(BFTError::Internal(format!(
                "Cannot propose in phase {:?}",
                self.state.phase
            )));
        }
        
        // 获取前一个 QC 的哈希
        let prev_qc_hash = self.state.last_qc
            .as_ref()
            .map(|qc| qc.hash())
            .unwrap_or([0u8; 32]);
        
        // 创建提案
        let proposal = BFTProposal {
            epoch_id: epoch.id,
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
    pub fn vote(
        &mut self,
        proposal: &BFTProposal,
        signing_key: &SigningKey,
    ) -> BFTResult<Vote> {
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
        // 检查投票是否针对当前提案
        let proposal = self.state.active_proposal
            .as_ref()
            .ok_or_else(|| BFTError::Internal("No active proposal".to_string()))?;
        
        let proposal_hash = proposal.hash();
        if vote.proposal_hash != proposal_hash {
            return Err(BFTError::InvalidProposal(
                "Vote for wrong proposal".to_string()
            ));
        }
        
        // 检查是否已投票
        if self.state.votes.iter().any(|v| v.voter_id == vote.voter_id) {
            return Err(BFTError::DuplicateVote(vote.voter_id));
        }
        
        self.state.votes.push(vote);
        
        // 检查是否达到法定人数
        let quorum_size = self.validator_set.quorum_size();
        if self.state.votes.len() >= quorum_size as usize {
            // 形成 QC
            let mut qc = QuorumCertificate::new(proposal_hash, proposal.height);
            for vote in &self.state.votes {
                qc.add_vote(vote.clone(), 1); // 等权重
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
        
        // 选择下一个 Leader（Round-robin）
        let next_leader_idx = ((self.state.height + 1) % self.validator_set.validators.len() as u64) as usize;
        let next_leader = self.validator_set.validators[next_leader_idx];
        
        self.state.advance_height(next_leader);
        
        Ok(())
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
    }
    
    /// 获取协议状态（用于测试/模拟）
    pub fn get_state(&self) -> ProtocolState {
        self.state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epoch::Epoch;
    use rand::rngs::OsRng;
    use std::collections::HashMap;

    #[test]
    fn test_protocol_full_round() {
        // 4 个验证者
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        
        // 生成密钥
        let signing_keys: Vec<_> = (0..4)
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();
        
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
            
            let vote = voter_protocol.vote(&proposal, &signing_keys[i as usize]).unwrap();
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
}

