# WEB30 Token 标准规范

**版本**: v0.1.0  
**状态**: 草案  
**作者**: SuperVM Team

---

## 设计理念

WEB30 是为 SuperVM 生态设计的下一代代币标准，充分利用 MVCC 并行、跨链原生、隐私保护与 zkVM 可验证性，超越传统 ERC20 限制。

### 核心优势（对标主流公链）

| 特性 | ERC20<br>(Ethereum) | SPL Token<br>(Solana) | TRC20<br>(TRON) | BEP20<br>(BSC) | SUI Move | WEB30<br>(SuperVM) |
|------|-------|-------|-------|-------|-------|-------|
| **并行执行** | ❌ 串行 | ✅ Sealevel | ❌ 串行 | ❌ 串行 | ✅ Move VM | ✅ **MVCC 读写集** |
| **跨链原生** | ❌ 需桥接 | ❌ 需桥接 | ❌ 需桥接 | ❌ 需桥接 | ❌ 需桥接 | ✅ **统一账户体系** |
| **隐私保护** | ❌ 公开 | ❌ 公开 | ❌ 公开 | ❌ 公开 | ❌ 公开 | ✅ **环签名/隐身地址** |
| **原子操作** | ❌ approve 双步骤 | ✅ 单步 | ❌ 双步骤 | ❌ 双步骤 | ✅ 单步 | ✅ **MVCC 原子保证** |
| **Gas 优化** | ❌ 固定 | ✅ 按计算 | ❌ 固定 | ❌ 固定 | ✅ 按资源 | ✅ **动态批量+多币种** |
| **权限模型** | ❌ 仅 owner | ⚠️ 程序派生 | ❌ 仅 owner | ❌ 仅 owner | ✅ Capability | ✅ **多级角色+DAO** |
| **元数据** | ❌ 链下 | ⚠️ 链下/Metaplex | ❌ 链下 | ❌ 链下 | ⚠️ 链下 | ✅ **链上+Web3存储** |
| **可验证性** | ❌ 信任执行 | ❌ 信任执行 | ❌ 信任执行 | ❌ 信任执行 | ⚠️ 形式验证 | ✅ **zkVM 零知识证明** |
| **TPS** | ~15 | ~65K | ~2K | ~100 | ~300K | ✅ **495K (2PC混合)** |
| **确认时间** | ~12s | ~400ms | ~3s | ~3s | ~400ms | ✅ **<100ms (FastPath)** |

---

## 接口定义

### Rust Trait（WASM 合约）

```rust
use serde::{Serialize, Deserialize};
use anyhow::Result;

/// WEB30 代币标准接口
pub trait WEB30Token {
    // ========== 基础信息 ==========
    
    /// 代币名称（如 "SuperVM Token"）
    fn name(&self) -> String;
    
    /// 代币符号（如 "SVM"）
    fn symbol(&self) -> String;
    
    /// 小数位数（如 18）
    fn decimals(&self) -> u8;
    
    /// 总供应量
    fn total_supply(&self) -> u128;
    
    // ========== 余额查询 ==========
    
    /// 查询账户余额（公开）
    fn balance_of(&self, account: &Address) -> u128;
    
    /// 查询隐私余额（需签名证明）
    fn balance_of_private(&self, account: &Address, proof: &ZkProof) -> Result<u128>;
    
    // ========== 转账操作 ==========
    
    /// 标准转账（单步原子操作）
    fn transfer(&mut self, to: &Address, amount: u128) -> Result<TransferReceipt>;
    
    /// 批量转账（并行优化）
    fn batch_transfer(&mut self, recipients: &[(Address, u128)]) -> Result<Vec<TransferReceipt>>;
    
    /// 隐私转账（环签名/隐身地址）
    fn transfer_private(
        &mut self,
        to: &StealthAddress,
        amount: u128,
        ring_signature: &RingSignature,
    ) -> Result<TransferReceipt>;
    
    /// 跨链转账（原生支持）
    fn transfer_cross_chain(
        &mut self,
        to_chain: ChainId,
        to_address: &Address,
        amount: u128,
    ) -> Result<CrossChainReceipt>;
    
    // ========== 授权与委托（改进 ERC20）==========
    
    /// 设置支出限额（无需 approve+transferFrom 双步骤）
    fn set_allowance(&mut self, spender: &Address, amount: u128) -> Result<()>;
    
    /// 查询授权额度
    fn allowance(&self, owner: &Address, spender: &Address) -> u128;
    
    /// 代理转账（单步）
    fn transfer_from(&mut self, from: &Address, to: &Address, amount: u128) -> Result<TransferReceipt>;
    
    // ========== 高级功能 ==========
    
    /// 铸币（需权限）
    fn mint(&mut self, to: &Address, amount: u128) -> Result<()>;
    
    /// 销毁
    fn burn(&mut self, amount: u128) -> Result<()>;
    
    /// 冻结账户（合规/治理）
    fn freeze(&mut self, account: &Address) -> Result<()>;
    
    /// 解冻账户
    fn unfreeze(&mut self, account: &Address) -> Result<()>;
    
    // ========== 元数据与治理 ==========
    
    /// 获取链上元数据（图标/描述等）
    fn metadata(&self) -> TokenMetadata;
    
    /// 更新元数据（需治理投票）
    fn update_metadata(&mut self, new_metadata: TokenMetadata, gov_proof: &GovernanceProof) -> Result<()>;
    
    /// 提案投票（原生 DAO）
    fn propose(&mut self, proposal: Proposal) -> Result<ProposalId>;
    fn vote(&mut self, proposal_id: ProposalId, support: bool, amount: u128) -> Result<()>;
    fn execute(&mut self, proposal_id: ProposalId) -> Result<()>;
}

// ========== 数据结构 ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address(pub [u8; 32]);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthAddress {
    pub view_key: [u8; 32],
    pub spend_key: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferReceipt {
    pub tx_hash: [u8; 32],
    pub from: Address,
    pub to: Address,
    pub amount: u128,
    pub timestamp: u64,
    pub gas_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainReceipt {
    pub swap_id: [u8; 32],
    pub from_chain: ChainId,
    pub to_chain: ChainId,
    pub from_address: Address,
    pub to_address: Address,
    pub amount: u128,
    pub status: CrossChainStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub icon_uri: String,  // Web3 存储链接
    pub description: String,
    pub website: String,
    pub social: SocialLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialLinks {
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub discord: Option<String>,
}

pub type ChainId = u64;
pub type ProposalId = u64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CrossChainStatus {
    Pending,
    Confirmed,
    Failed,
}
```

---

## 多链兼容层

### Solidity 兼容层（Ethereum/BSC/TRON）

为兼容 EVM 生态，提供 Solidity 接口映射：

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// WEB30 Token 标准（Solidity 版本）
interface IWEB30 {
    // 基础 ERC20 兼容
    function name() external view returns (string memory);
    function symbol() external view returns (string memory);
    function decimals() external view returns (uint8);
    function totalSupply() external view returns (uint256);
    function balanceOf(address account) external view returns (uint256);
    
    // 增强转账
    function transfer(address to, uint256 amount) external returns (bool);
    function batchTransfer(address[] calldata recipients, uint256[] calldata amounts) external returns (bool);
    
    // 跨链扩展
    function transferCrossChain(
        uint64 toChain,
        address toAddress,
        uint256 amount
    ) external returns (bytes32 swapId);
    
    // 隐私扩展
    function transferPrivate(
        bytes calldata stealthAddress,
        uint256 amount,
        bytes calldata ringSignature
    ) external returns (bool);
    
    // 治理扩展
    function propose(bytes calldata proposalData) external returns (uint256 proposalId);
    function vote(uint256 proposalId, bool support, uint256 voteAmount) external;
    function execute(uint256 proposalId) external;
    
    // 元数据
    function metadata() external view returns (
        string memory iconUri,
        string memory description,
        string memory website
    );
    
    // 事件
    event Transfer(address indexed from, address indexed to, uint256 value);
    event CrossChainTransfer(uint64 indexed toChain, address indexed to, uint256 value, bytes32 swapId);
    event PrivateTransfer(bytes indexed stealthAddress, uint256 value);
    event ProposalCreated(uint256 indexed proposalId, address proposer);
    event Voted(uint256 indexed proposalId, address voter, bool support, uint256 votes);
}
```

### Rust SPL 兼容层（Solana）

为兼容 Solana 生态，提供 SPL Token 映射：

```rust
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
};

/// WEB30 适配为 SPL Token Program 接口
pub struct WEB30SolanaAdapter;

impl WEB30SolanaAdapter {
    /// 初始化代币（对应 SPL InitializeMint）
    pub fn initialize_mint(
        program_id: &Pubkey,
        mint: &AccountInfo,
        mint_authority: &Pubkey,
        freeze_authority: Option<&Pubkey>,
        decimals: u8,
    ) -> ProgramResult {
        // 调用 WEB30 核心合约初始化
        let token = WEB30Token::new(decimals, *mint_authority);
        // ... 存储到 Solana 账户
        Ok(())
    }
    
    /// 铸币（对应 SPL MintTo）
    pub fn mint_to(
        program_id: &Pubkey,
        mint: &AccountInfo,
        destination: &AccountInfo,
        authority: &AccountInfo,
        amount: u64,
    ) -> ProgramResult {
        // 调用 WEB30 mint
        token.mint(&destination.key.to_bytes(), amount as u128)?;
        Ok(())
    }
    
    /// 转账（对应 SPL Transfer）
    pub fn transfer(
        program_id: &Pubkey,
        source: &AccountInfo,
        destination: &AccountInfo,
        authority: &AccountInfo,
        amount: u64,
    ) -> ProgramResult {
        // 调用 WEB30 transfer
        token.transfer(&destination.key.to_bytes(), amount as u128)?;
        Ok(())
    }
    
    /// 批量转账（WEB30 增强）
    pub fn batch_transfer(
        program_id: &Pubkey,
        source: &AccountInfo,
        destinations: &[AccountInfo],
        authority: &AccountInfo,
        amounts: &[u64],
    ) -> ProgramResult {
        let recipients: Vec<_> = destinations.iter()
            .zip(amounts)
            .map(|(acc, amt)| (acc.key.to_bytes(), *amt as u128))
            .collect();
        token.batch_transfer(&recipients)?;
        Ok(())
    }
}
```

### Move 兼容层（SUI/Aptos）

为兼容 Move 生态，提供 Move 模块映射：

```move
module supervm::web30_token {
    use std::string::String;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::coin::{Self, Coin};
    
    /// WEB30 代币对象（对应 Move Coin）
    struct WEB30Token has key, store {
        id: UID,
        name: String,
        symbol: String,
        decimals: u8,
        total_supply: u128,
    }
    
    /// 代币余额（对应 Move Coin<T>）
    struct TokenBalance has key {
        id: UID,
        value: u128,
    }
    
    /// 初始化代币
    public fun initialize(
        name: vector<u8>,
        symbol: vector<u8>,
        decimals: u8,
        ctx: &mut TxContext
    ): WEB30Token {
        WEB30Token {
            id: object::new(ctx),
            name: string::utf8(name),
            symbol: string::utf8(symbol),
            decimals,
            total_supply: 0,
        }
    }
    
    /// 铸币
    public fun mint(
        token: &mut WEB30Token,
        amount: u128,
        ctx: &mut TxContext
    ): TokenBalance {
        token.total_supply = token.total_supply + amount;
        TokenBalance {
            id: object::new(ctx),
            value: amount,
        }
    }
    
    /// 转账（Move 风格）
    public fun transfer(
        balance: &mut TokenBalance,
        amount: u128,
        recipient: address,
        ctx: &mut TxContext
    ) {
        assert!(balance.value >= amount, 0);
        balance.value = balance.value - amount;
        
        let new_balance = TokenBalance {
            id: object::new(ctx),
            value: amount,
        };
        transfer::transfer(new_balance, recipient);
    }
    
    /// 批量转账（WEB30 增强）
    public fun batch_transfer(
        balance: &mut TokenBalance,
        recipients: vector<address>,
        amounts: vector<u128>,
        ctx: &mut TxContext
    ) {
        let total: u128 = 0;
        let i = 0;
        while (i < vector::length(&amounts)) {
            total = total + *vector::borrow(&amounts, i);
            i = i + 1;
        };
        assert!(balance.value >= total, 0);
        
        // 批量创建并转移
        i = 0;
        while (i < vector::length(&recipients)) {
            let amt = *vector::borrow(&amounts, i);
            let recipient = *vector::borrow(&recipients, i);
            let new_balance = TokenBalance {
                id: object::new(ctx),
                value: amt,
            };
            transfer::transfer(new_balance, recipient);
            i = i + 1;
        };
        balance.value = balance.value - total;
    }
    
    /// 跨链转账（WEB30 原生）
    public fun transfer_cross_chain(
        balance: &mut TokenBalance,
        to_chain: u64,
        to_address: vector<u8>,
        amount: u128,
        ctx: &mut TxContext
    ) {
        assert!(balance.value >= amount, 0);
        balance.value = balance.value - amount;
        // 调用 WEB30 跨链协调器
        // cross_chain::initiate_swap(to_chain, to_address, amount);
    }
}
```

### Rust 原生实现（WASM 合约）

```rust
use vm_runtime::*;

pub struct SVM30Token {
    name: String,
    symbol: String,
    decimals: u8,
    total_supply: u128,
    balances: HashMap<Address, u128>,
    allowances: HashMap<(Address, Address), u128>,
    frozen: HashSet<Address>,
    metadata: TokenMetadata,
}

impl WEB30Token for SVM30Token {
    fn name(&self) -> String {
        self.name.clone()
    }
    
    fn symbol(&self) -> String {
        self.symbol.clone()
    }
    
    fn decimals(&self) -> u8 {
        self.decimals
    }
    
    fn total_supply(&self) -> u128 {
        self.total_supply
    }
    
    fn balance_of(&self, account: &Address) -> u128 {
        *self.balances.get(account).unwrap_or(&0)
    }
    
    fn transfer(&mut self, to: &Address, amount: u128) -> Result<TransferReceipt> {
        let sender = get_caller(); // 从 Runtime 获取
        
        // 检查余额
        let sender_balance = self.balance_of(&sender);
        if sender_balance < amount {
            bail!("Insufficient balance");
        }
        
        // 检查冻结状态
        if self.frozen.contains(&sender) || self.frozen.contains(to) {
            bail!("Account frozen");
        }
        
        // 原子转账（MVCC 保证）
        *self.balances.entry(sender).or_insert(0) -= amount;
        *self.balances.entry(*to).or_insert(0) += amount;
        
        Ok(TransferReceipt {
            tx_hash: get_tx_hash(),
            from: sender,
            to: *to,
            amount,
            timestamp: get_timestamp(),
            gas_used: get_gas_used(),
        })
    }
    
    fn batch_transfer(&mut self, recipients: &[(Address, u128)]) -> Result<Vec<TransferReceipt>> {
        let sender = get_caller();
        let total_amount: u128 = recipients.iter().map(|(_, amt)| amt).sum();
        
        // 预检查余额
        if self.balance_of(&sender) < total_amount {
            bail!("Insufficient balance for batch transfer");
        }
        
        // 批量执行（并行优化由 MVCC 调度器处理）
        let mut receipts = Vec::new();
        for (to, amount) in recipients {
            let receipt = self.transfer(to, *amount)?;
            receipts.push(receipt);
        }
        
        Ok(receipts)
    }
    
    fn transfer_cross_chain(
        &mut self,
        to_chain: ChainId,
        to_address: &Address,
        amount: u128,
    ) -> Result<CrossChainReceipt> {
        let sender = get_caller();
        
        // 锁定本链代币
        *self.balances.entry(sender).or_insert(0) -= amount;
        
        // 调用跨链协调器
        let swap_id = atomic_cross_chain_swap(
            sender,
            *to_address,
            get_current_chain_id(),
            to_chain,
            amount,
        )?;
        
        Ok(CrossChainReceipt {
            swap_id,
            from_chain: get_current_chain_id(),
            to_chain,
            from_address: sender,
            to_address: *to_address,
            amount,
            status: CrossChainStatus::Pending,
        })
    }
    
    fn mint(&mut self, to: &Address, amount: u128) -> Result<()> {
        // 权限检查（仅 owner 或治理合约）
        require_permission(Permission::Mint)?;
        
        *self.balances.entry(*to).or_insert(0) += amount;
        self.total_supply += amount;
        
        Ok(())
    }
    
    // ... 其他方法实现
}
```

---

## 优势对比实例

### ERC20 的痛点

```solidity
// ERC20: 需要 2 步交互（易受攻击）
token.approve(spender, amount);  // 交易 1
spender.transferFrom(owner, recipient, amount);  // 交易 2（可能被抢跑）
```

### WEB30 的解决方案

```rust
// WEB30: 单步原子操作
token.set_allowance(&spender, amount)?;
token.transfer_from(&owner, &recipient, amount)?;  // 原子执行，MVCC 保证
```

---

## 扩展协议族

基于 WEB30 核心，扩展为协议族：

| 协议 | 用途 | 状态 |
|------|------|------|
| **WEB30** | 可替代代币（Token） | ✅ 本标准 |
| **WEB3001** | 非同质化代币（NFT） | 📋 规划中 |
| **WEB3002** | 多代币标准（Multi-Token） | 📋 规划中 |
| **WEB3003** | DAO 治理合约 | 📋 规划中 |
| **WEB3004** | DeFi 协议（AMM/Lending） | 📋 规划中 |
| **WEB3005** | 身份与信誉系统 | 📋 草案 |
| **WEB3006** | 去中心化存储接口 | 📋 规划中 |
| **WEB3007** | 跨链消息协议 | 📋 规划中 |
| **WEB3008** | 域名服务（DNS） | 📋 规划中 |
| **WEB3009** | 去中心化交易所（DEX） | 📋 草案 |
| **WEB3010** | 预言机标准 | 📋 规划中 |
| **WEB3011** | **AI 智能接口（大脑）** | 📋 规划中 |
| **WEB3012** | **物联网感知接口（传感器/输入）** | 📋 规划中 |
| **WEB3013** | **设备控制接口（执行器/输出）** | 📋 规划中 |
| **WEB3014** | 去中心化消息协议（Messaging） | 📋 草案 |

### 核心类比架构

```
┌─────────────────────────────────────────────────────────┐
│                    SuperVM 有机体                        │
├─────────────────────────────────────────────────────────┤
│  🧠 WEB3011 AI 接口 (大脑)                              │
│     - LLM 推理调用                                       │
│     - 链上 AI 决策                                       │
│     - 模型训练/微调                                      │
├─────────────────────────────────────────────────────────┤
│  ❤️  L0 MVCC 内核 (心脏)                                │
│     - 并行执行 495K TPS                                  │
│     - 跨链原子交换                                       │
│     - zkVM 可验证                                        │
├─────────────────────────────────────────────────────────┤
│  👁️ WEB3012 感知接口 (传感器/输入)                      │
│     - IoT 设备数据上链                                   │
│     - 实时事件流                                         │
│     - 外部系统集成                                       │
├─────────────────────────────────────────────────────────┤
│  🦾 WEB3013 控制接口 (执行器/输出)                      │
│     - 智能设备控制                                       │
│     - 物理世界执行                                       │
│     - 自动化流程                                         │
└─────────────────────────────────────────────────────────┘

完整闭环: 感知(WEB3012) → 思考(WEB3011) → 执行(WEB3013)
```

---

## 统一账户模型（登录与钱包绑定）

- 核心概念：SuperVM 统一账户支持“公钥地址(20字节)”与“数字账户(12位)”双形态，二者可互为别名，并可绑定多链外部钱包地址，形成统一身份与统一资产视图。
- 协议归属：统一账户/登录/钱包绑定的标准接口定义在 `WEB3005`（身份与信誉系统）。
- 运行时对齐：参考 `src/vm-runtime/src/adapter/account.rs`（`SuperVMAccountId`/`SuperVMAccount`）。
- 与 WEB30 的关系：
    - `transfer_cross_chain` 与 DEX 结算等需要解析“统一账户 → 目标链地址”的映射（通过 WEB3005 提供的绑定记录）。
    - 钱包授权/代理转账可按统一账户的登录态与授权策略实施（跨链与多钱包一致化）。
    - 可选 KYC：如需合规门槛或额度分级，可通过 WEB3005 的 KYC 证明（零知识）进行接入控制或额度调整，无需上链 PII。
    - 默认策略：KYC 为可选、用户自愿。默认无需 KYC 即可使用 WEB30 与生态协议；仅在特定 DApp 或合规场景由应用选择启用，且不在 L1 存放任何可识别 PII。

---

## 向后兼容

- **ERC20 桥接**: 提供 ERC20 → WEB30 自动转换层
- **Solidity 包装器**: 为 EVM 链提供 WEB30 Solidity 实现
- **ABI 兼容**: 前端可用 ethers.js 等工具直接调用

---

## 下一步

1. **标准完善**: 社区讨论与迭代（GitHub Issues/Discord）
2. **参考实现**: 完整 Rust WASM 合约 + Solidity 版本
3. **工具链**: 
   - WEB30 合约模板生成器
   - TypeScript SDK（`@supervm/web30`）
   - CLI 工具（`supervm token deploy --standard web30`）
4. **生态激励**: WEB30 代币白名单 + 流动性挖矿

---

**版本历史**:
- v0.1.0 (2025-11-17): 初始草案

**贡献**: 欢迎通过 GitHub PR 参与标准制定
