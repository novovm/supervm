// Phase 4.2: Epoch Management
//
// Epoch = N 个 Batch 的聚合单位
// BFT 只对 Epoch 进行共识，不对单笔交易

use crate::types::{Hash, Height, BFTResult, BFTError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Batch ID（来自 Phase 4.1）
pub type BatchId = u64;

/// Epoch 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochConfig {
    /// 每个 Epoch 包含的 Batch 数量
    pub batches_per_epoch: usize,
    
    /// Epoch 最大持续时间（毫秒）
    pub max_duration_ms: u64,
    
    /// 是否启用自适应调整
    pub adaptive: bool,
}

impl Default for EpochConfig {
    fn default() -> Self {
        Self {
            batches_per_epoch: 10,      // 10 个 Batch → 1 个 Epoch
            max_duration_ms: 2000,       // 最多 2 秒
            adaptive: true,              // 根据负载动态调整
        }
    }
}

/// Epoch 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Epoch {
    /// Epoch ID（单调递增）
    pub id: u64,
    
    /// 区块高度
    pub height: Height,
    
    /// 包含的 Batch IDs
    pub batches: Vec<BatchId>,
    
    /// Coordinator ID（生成该 Epoch 的节点）
    pub coordinator_id: u32,
    
    /// 状态增量 Merkle Root
    pub state_root: Hash,
    
    /// 开始时间戳
    pub start_time: u64,
    
    /// 结束时间戳（提交时填写）
    pub end_time: Option<u64>,
    
    /// 总交易数
    pub total_txs: u64,
}

impl Epoch {
    /// 创建新的 Epoch
    pub fn new(id: u64, height: Height, coordinator_id: u32) -> Self {
        Self {
            id,
            height,
            batches: Vec::new(),
            coordinator_id,
            state_root: [0u8; 32],
            start_time: current_timestamp_ms(),
            end_time: None,
            total_txs: 0,
        }
    }
    
    /// 添加 Batch
    pub fn add_batch(&mut self, batch_id: BatchId, tx_count: u64) {
        self.batches.push(batch_id);
        self.total_txs += tx_count;
    }
    
    /// 计算状态增量哈希（Merkle Root）
    pub fn compute_state_root(&mut self, batch_results: &HashMap<BatchId, Hash>) -> BFTResult<()> {
        use sha2::{Sha256, Digest};
        
        // 简化版 Merkle Root：将所有 Batch 结果哈希串联
        let mut hasher = Sha256::new();
        for batch_id in &self.batches {
            let batch_hash = batch_results
                .get(batch_id)
                .ok_or_else(|| BFTError::Internal(format!("Missing batch result: {}", batch_id)))?;
            hasher.update(batch_hash);
        }
        
        self.state_root = hasher.finalize().into();
        Ok(())
    }
    
    /// 标记为已提交
    pub fn commit(&mut self) {
        self.end_time = Some(current_timestamp_ms());
    }
    
    /// 检查是否已满（达到配置的 Batch 数量）
    pub fn is_full(&self, config: &EpochConfig) -> bool {
        self.batches.len() >= config.batches_per_epoch
    }
    
    /// 检查是否超时
    pub fn is_timeout(&self, config: &EpochConfig) -> bool {
        let elapsed = current_timestamp_ms() - self.start_time;
        elapsed >= config.max_duration_ms
    }
    
    /// 计算 TPS（仅在已提交时有效）
    pub fn compute_tps(&self) -> Option<f64> {
        self.end_time.map(|end| {
            let duration_sec = (end - self.start_time) as f64 / 1000.0;
            if duration_sec > 0.0 {
                self.total_txs as f64 / duration_sec
            } else {
                0.0
            }
        })
    }
}

/// 获取当前时间戳（毫秒）
fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Epoch 管理器（维护 Epoch 链）
#[derive(Debug)]
pub struct EpochManager {
    /// 配置
    config: EpochConfig,
    
    /// 当前 Epoch
    current_epoch: Option<Epoch>,
    
    /// 已提交的 Epoch（最近 N 个）
    committed_epochs: Vec<Epoch>,
    
    /// 下一个 Epoch ID
    next_epoch_id: u64,
    
    /// 下一个区块高度
    next_height: Height,
}

impl EpochManager {
    /// 创建新的 Epoch 管理器
    pub fn new(config: EpochConfig) -> Self {
        Self {
            config,
            current_epoch: None,
            committed_epochs: Vec::new(),
            next_epoch_id: 0,
            next_height: 0,
        }
    }
    
    /// 启动新的 Epoch
    pub fn start_epoch(&mut self, coordinator_id: u32) -> &Epoch {
        let epoch = Epoch::new(self.next_epoch_id, self.next_height, coordinator_id);
        self.next_epoch_id += 1;
        self.next_height += 1;
        self.current_epoch = Some(epoch);
        self.current_epoch.as_ref().unwrap()
    }
    
    /// 获取当前 Epoch（可变引用）
    pub fn current_epoch_mut(&mut self) -> Option<&mut Epoch> {
        self.current_epoch.as_mut()
    }
    
    /// 检查是否应该关闭当前 Epoch
    pub fn should_close_epoch(&self) -> bool {
        if let Some(epoch) = &self.current_epoch {
            epoch.is_full(&self.config) || epoch.is_timeout(&self.config)
        } else {
            false
        }
    }
    
    /// 提交当前 Epoch
    pub fn commit_current_epoch(&mut self) -> BFTResult<Epoch> {
        let mut epoch = self.current_epoch
            .take()
            .ok_or_else(|| BFTError::Internal("No current epoch to commit".to_string()))?;
        
        epoch.commit();
        
        // 保留最近 100 个 Epoch
        if self.committed_epochs.len() >= 100 {
            self.committed_epochs.remove(0);
        }
        self.committed_epochs.push(epoch.clone());
        
        Ok(epoch)
    }
    
    /// 获取最后提交的 Epoch
    pub fn last_committed_epoch(&self) -> Option<&Epoch> {
        self.committed_epochs.last()
    }
    
    /// 获取已提交 Epoch 总数
    pub fn total_committed(&self) -> usize {
        self.committed_epochs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_lifecycle() {
        let mut epoch = Epoch::new(0, 0, 0);
        assert_eq!(epoch.batches.len(), 0);
        assert_eq!(epoch.total_txs, 0);
        
        // 添加 Batch
        epoch.add_batch(1, 100);
        epoch.add_batch(2, 150);
        assert_eq!(epoch.batches.len(), 2);
        assert_eq!(epoch.total_txs, 250);
        
        // 提交
        epoch.commit();
        assert!(epoch.end_time.is_some());
        
        // 计算 TPS
        let tps = epoch.compute_tps();
        assert!(tps.is_some());
    }

    #[test]
    fn test_epoch_is_full() {
        let config = EpochConfig {
            batches_per_epoch: 3,
            max_duration_ms: 5000,
            adaptive: false,
        };
        
        let mut epoch = Epoch::new(0, 0, 0);
        assert!(!epoch.is_full(&config));
        
        epoch.add_batch(1, 100);
        epoch.add_batch(2, 100);
        assert!(!epoch.is_full(&config));
        
        epoch.add_batch(3, 100);
        assert!(epoch.is_full(&config)); // 达到 3 个 Batch
    }

    #[test]
    fn test_epoch_manager() {
        let config = EpochConfig::default();
        let mut manager = EpochManager::new(config);
        
        // 启动第一个 Epoch
        let epoch1 = manager.start_epoch(0);
        assert_eq!(epoch1.id, 0);
        assert_eq!(epoch1.height, 0);
        
        // 添加 Batch
        manager.current_epoch_mut().unwrap().add_batch(1, 100);
        
        // 提交
        let committed = manager.commit_current_epoch().unwrap();
        assert_eq!(committed.id, 0);
        assert_eq!(committed.total_txs, 100);
        assert_eq!(manager.total_committed(), 1);
        
        // 启动第二个 Epoch
        let epoch2 = manager.start_epoch(1);
        assert_eq!(epoch2.id, 1);
        assert_eq!(epoch2.height, 1);
    }

    #[test]
    fn test_state_root_computation() {
        let mut epoch = Epoch::new(0, 0, 0);
        epoch.add_batch(1, 100);
        epoch.add_batch(2, 150);
        
        let mut batch_results = HashMap::new();
        batch_results.insert(1, [1u8; 32]);
        batch_results.insert(2, [2u8; 32]);
        
        assert!(epoch.compute_state_root(&batch_results).is_ok());
        assert_ne!(epoch.state_root, [0u8; 32]); // 非空哈希
    }
}

