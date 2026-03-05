// Phase 4.2: Quorum Certificate (QC)
//
// QC = 法定人数的投票聚合
// 使用 Ed25519 签名（Phase 4.1 已完成 batch verify PoC）

use crate::types::{Hash, Height, NodeId, BFTResult, BFTError, ValidatorSet};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 投票（由验证者签名）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// 投票者 ID
    pub voter_id: NodeId,
    
    /// 投票的提案哈希
    pub proposal_hash: Hash,
    
    /// 区块高度
    pub height: Height,
    
    /// Ed25519 签名
    pub signature: Vec<u8>,
}

impl Vote {
    /// 创建新投票
    pub fn new(
        voter_id: NodeId,
        proposal_hash: Hash,
        height: Height,
        signing_key: &SigningKey,
    ) -> Self {
        let message = Self::construct_message(&proposal_hash, height);
        let signature = signing_key.sign(&message).to_bytes().to_vec();
        
        Self {
            voter_id,
            proposal_hash,
            height,
            signature,
        }
    }
    
    /// 验证投票签名
    pub fn verify(&self, verifying_key: &VerifyingKey) -> BFTResult<()> {
        let message = Self::construct_message(&self.proposal_hash, self.height);
        let signature = Signature::from_slice(&self.signature)
            .map_err(|e| BFTError::InvalidSignature(format!("Invalid signature format: {}", e)))?;
        
        verifying_key
            .verify(&message, &signature)
            .map_err(|e| BFTError::InvalidSignature(format!("Verification failed: {}", e)))?;
        
        Ok(())
    }
    
    /// 构造签名消息
    fn construct_message(proposal_hash: &Hash, height: Height) -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(b"VOTE:");
        message.extend_from_slice(proposal_hash);
        message.extend_from_slice(&height.to_le_bytes());
        message
    }
}

/// 法定人数证书（Quorum Certificate）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumCertificate {
    /// 提案哈希
    pub proposal_hash: Hash,
    
    /// 区块高度
    pub height: Height,
    
    /// 投票列表
    pub votes: Vec<Vote>,
    
    /// 投票权重总和
    pub total_weight: u64,
}

impl QuorumCertificate {
    /// 创建新的 QC（从投票聚合）
    pub fn new(proposal_hash: Hash, height: Height) -> Self {
        Self {
            proposal_hash,
            height,
            votes: Vec::new(),
            total_weight: 0,
        }
    }
    
    /// 添加投票
    pub fn add_vote(&mut self, vote: Vote, weight: u64) {
        self.votes.push(vote);
        self.total_weight += weight;
    }
    
    /// 验证 QC（批量验证所有签名）
    pub fn verify(&self, validator_set: &ValidatorSet, public_keys: &HashMap<NodeId, VerifyingKey>) -> BFTResult<()> {
        if cfg!(test) {
            eprintln!("[QC] verify: begin height={} votes={} total_weight={}", self.height, self.votes.len(), self.total_weight);
        }
        // 验证投票集合并重新计算权重（不信任声明的 total_weight）。
        let observed_weight = self.validate_votes_and_weight(validator_set)?;

        // 检查是否达到法定人数
        let quorum_size = validator_set.quorum_size();
        if observed_weight < quorum_size {
            return Err(BFTError::InsufficientVotes {
                required: quorum_size as usize,
                received: observed_weight as usize,
            });
        }
        if cfg!(test) {
            eprintln!("[QC] verify: quorum ok (required={})", quorum_size);
        }

        // 批量验证签名（使用 Ed25519 batch verify）
        self.batch_verify_signatures(validator_set, public_keys)?;
        if cfg!(test) {
            eprintln!("[QC] verify: signatures ok");
        }
        
        Ok(())
    }

    fn validate_votes_and_weight(&self, validator_set: &ValidatorSet) -> BFTResult<u64> {
        let mut voter_ids = std::collections::HashSet::new();
        let mut observed_weight = 0u64;

        for vote in &self.votes {
            if !validator_set.is_validator(vote.voter_id) {
                return Err(BFTError::NotValidator(vote.voter_id));
            }
            if !voter_ids.insert(vote.voter_id) {
                return Err(BFTError::DuplicateVote(vote.voter_id));
            }
            let weight = validator_set
                .weight_of(vote.voter_id)
                .ok_or(BFTError::NotValidator(vote.voter_id))?;
            observed_weight = observed_weight
                .checked_add(weight)
                .ok_or_else(|| BFTError::Internal("qc observed weight overflow".to_string()))?;
        }

        if observed_weight != self.total_weight {
            return Err(BFTError::InvalidProposal(format!(
                "QC total_weight mismatch: declared={} observed={}",
                self.total_weight, observed_weight
            )));
        }

        Ok(observed_weight)
    }
    
    /// 批量验证签名（Phase 4.1 PoC：3.48x-3.90x 加速）
    fn batch_verify_signatures(
        &self,
        validator_set: &ValidatorSet,
        public_keys: &HashMap<NodeId, VerifyingKey>,
    ) -> BFTResult<()> {
        // TODO: 使用 ed25519_dalek::verify_batch() 进行批量验证
        // 当前实现：逐个验证（Phase 4.2 Week 3 优化）
        
        for vote in &self.votes {
            // 检查是否是验证者
            if !validator_set.is_validator(vote.voter_id) {
                return Err(BFTError::NotValidator(vote.voter_id));
            }
            
            // 检查高度匹配
            if vote.height != self.height {
                return Err(BFTError::HeightMismatch {
                    expected: self.height,
                    got: vote.height,
                });
            }
            
            // 检查提案哈希匹配
            if vote.proposal_hash != self.proposal_hash {
                return Err(BFTError::InvalidProposal(
                    "Proposal hash mismatch in vote".to_string()
                ));
            }
            
            // 验证签名
            let public_key = public_keys
                .get(&vote.voter_id)
                .ok_or(BFTError::NotValidator(vote.voter_id))?;
            
            vote.verify(public_key)?;
        }
        
        Ok(())
    }
    
    /// 计算 QC 的哈希
    pub fn hash(&self) -> Hash {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(self.proposal_hash);
        hasher.update(self.height.to_le_bytes());
        hasher.update(self.total_weight.to_le_bytes());
        hasher.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn test_vote_creation_and_verification() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        
        let proposal_hash = [42u8; 32];
        let height = 100;
        
        // 创建投票
        let vote = Vote::new(0, proposal_hash, height, &signing_key);
        
        // 验证投票
        assert!(vote.verify(&verifying_key).is_ok());
    }

    #[test]
    fn test_vote_verification_fails_with_wrong_key() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let wrong_key = SigningKey::generate(&mut OsRng);
        
        let proposal_hash = [42u8; 32];
        let height = 100;
        
        let vote = Vote::new(0, proposal_hash, height, &signing_key);
        
        // 使用错误的公钥验证
        assert!(vote.verify(&wrong_key.verifying_key()).is_err());
    }

    #[test]
    fn test_qc_quorum_check() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let _quorum_size = validator_set.quorum_size(); // 3
        
        // 生成密钥
        let signing_keys: Vec<_> = (0..4)
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();
        
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();
        
        let proposal_hash = [1u8; 32];
        let height = 10;
        
        // 创建 QC
        let mut qc = QuorumCertificate::new(proposal_hash, height);
        
        // 添加 2 个投票（不足法定人数）
        for (i, signing_key) in signing_keys.iter().enumerate().take(2) {
            let vote = Vote::new(i as NodeId, proposal_hash, height, signing_key);
            qc.add_vote(vote, 1);
        }
        
        // 验证失败（2/3 不足）
        assert!(qc.verify(&validator_set, &public_keys).is_err());
        
        // 添加第 3 个投票（达到法定人数）
        let vote = Vote::new(2, proposal_hash, height, &signing_keys[2]);
        qc.add_vote(vote, 1);
        
        // 验证成功（3/3）
        assert!(qc.verify(&validator_set, &public_keys).is_ok());
    }

    #[test]
    fn test_qc_duplicate_vote_detection() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        
        let signing_keys: Vec<_> = (0..4)
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();
        
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();
        
        let proposal_hash = [1u8; 32];
        let height = 10;
        
        let mut qc = QuorumCertificate::new(proposal_hash, height);
        
        // 添加投票
        for (i, signing_key) in signing_keys.iter().enumerate().take(3) {
            let vote = Vote::new(i as NodeId, proposal_hash, height, signing_key);
            qc.add_vote(vote, 1);
        }
        
        // 添加重复投票（节点 0 重复投票）
        let duplicate_vote = Vote::new(0, proposal_hash, height, &signing_keys[0]);
        qc.add_vote(duplicate_vote, 1);
        
        // 验证失败（检测到重复投票）
        let result = qc.verify(&validator_set, &public_keys);
        assert!(result.is_err());
        if let Err(BFTError::DuplicateVote(id)) = result {
            assert_eq!(id, 0);
        } else {
            panic!("Expected DuplicateVote error");
        }
    }

    #[test]
    fn test_qc_rejects_total_weight_tamper() {
        let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2, 3]);
        let signing_keys: Vec<_> = (0..4)
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();

        let mut qc = QuorumCertificate::new([7u8; 32], 22);
        for (i, signing_key) in signing_keys.iter().enumerate().take(3) {
            let vote = Vote::new(i as NodeId, [7u8; 32], 22, signing_key);
            qc.add_vote(vote, 1);
        }
        qc.total_weight = 99; // tamper declared weight
        let result = qc.verify(&validator_set, &public_keys);
        assert!(matches!(result, Err(BFTError::InvalidProposal(_))));
    }

    #[test]
    fn test_qc_weighted_quorum_passes() {
        let validator_set = ValidatorSet::new_weighted(vec![(0, 5), (1, 3), (2, 2)]).unwrap();
        let signing_keys: Vec<_> = (0..3)
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();
        let public_keys: HashMap<NodeId, VerifyingKey> = signing_keys
            .iter()
            .enumerate()
            .map(|(i, sk)| (i as NodeId, sk.verifying_key()))
            .collect();

        let mut qc = QuorumCertificate::new([8u8; 32], 33);
        qc.add_vote(Vote::new(0, [8u8; 32], 33, &signing_keys[0]), 5);
        qc.add_vote(Vote::new(2, [8u8; 32], 33, &signing_keys[2]), 2);
        assert_eq!(qc.total_weight, 7);
        assert!(qc.verify(&validator_set, &public_keys).is_ok()); // quorum=7
    }
}

