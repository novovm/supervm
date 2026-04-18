# NOVOVM 货币架构决议（M0/M1/M2 与多币支付）  
_2026-04-17_

## 1. 目的与范围

本文件用于冻结以下口径，避免后续实现再次分叉：

1. `NOV` 与 `EVM/ETH` 的协议定位差异  
2. `Gas` 与 NOVOVM 费用模型的术语与实现边界  
3. `M0/M1/M2` 的货币分层与“外币支付、NOV 结算”的主线模型

本文件是实现约束，不是概念草稿。

## 2. 输入依据（已比对）

- 宏观稿件（外部）：  
  - `C:\Users\leadb\Desktop\苹果电脑的代币经济学和商业叙述\代币宏观经济学\Final\现代代币宏观经济学(2025-12第一版出版稿).pdf`
- 仓库内抽取文本：  
  - [macro-econ-fulltext-2026-04-17.txt](/d:/WEB3_AI/SUPERVM/artifacts/audit/macro-econ-fulltext-2026-04-17.txt)  
  - [macro-econ-key-extract-2026-04-17.txt](/d:/WEB3_AI/SUPERVM/artifacts/audit/macro-econ-key-extract-2026-04-17.txt)
- 当前实现位点：  
  - [token_runtime.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-consensus/src/token_runtime.rs:65)（`NOV` 主币符号）  
  - [protocol.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-consensus/src/protocol.rs:111)（HotStuff/BFT 主线）  
  - [tx_wire.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-protocol/src/tx_wire.rs:9)（当前原生 tx wire 仍是 transfer 核心字段）  
  - [tx_ingress.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-node/src/tx_ingress.rs:69)（当前 ingress 映射仍以 transfer 语义为主）

## 3. 问题 1：NOV 协议对比 EVM/ETH 的先进性

### 3.1 冻结结论

- NOVOVM 的先进性不在“TPS 口号”，而在“执行优先 + 可验证结算 + 宿主化多链能力”。
- 对外可表述为：`proof-driven execution network`（以可验证执行为中心的网络）。
- **但当前实现仍有 canonical chain / HotStuff / block lifecycle。**  
  结论：现在是“执行证明导向的链上系统”，不是“彻底无块结构”。

### 3.2 对外口径（固定）

- 可说：`我们不是以区块打包为中心，而是以执行与可验证结果为中心。`
- 不可说：`我们已经完全不是区块链模式。`

## 4. 问题 2：GAS 与费用模型

### 4.1 冻结结论

- 对开发者兼容层可继续保留 `gas_*` 字段（兼容 EVM 工具链）。
- 对 NOV 原生口径统一使用：`Execution Fee`（执行费），不再把主叙事写成 Gas。

### 4.2 费用模型约束

- 原生费用应按资源分项计量：`compute + storage + bandwidth + proof + routing`。
- 结算货币唯一：`NOV`（内部结算不分叉）。

## 5. 问题 3：M0/M1/M2 与多币支付

### 5.1 分层冻结（本次关键）

- `M0`：基础货币层，只是 `NOV`。  
- `M1`：流通货币层，只统计 NOV 体系内可流通货币，不放镜像外币条目。  
- `M2`：信用扩张层，允许发行新信用货币（`n*` 系列）。

### 5.2 明确禁止

- 禁止把 `pETH / pUSDT / pSOL` 这类镜像资产放在 M1。
- 禁止绕过 NOV 结算直接把外币当内部结算币。

### 5.3 M2 生成主线（冻结）

1. 外链资产（ETH/USDT/DAI 等）先在 EVM 插件侧锁仓。  
2. 锁仓结果进入 NOVOVM 国库储备记账。  
3. 通过清算/兑换规则转换为 NOV 抵押基础。  
4. 仅在满足抵押与风险参数时，铸造 `M2` 信用货币（`n*`）。  
5. `M2` 资产可流通（含 RWA 类资产），但其风险归属在 M2，不回写为 M1。

## 6. 多币支付模型（实现口径）

### 6.1 核心规则

- 用户可用外币支付（ETH/USDT/DAI...）。
- 系统自动兑换/清算后，内部仍以 NOV 结算。

### 6.2 标准流程

`外币支付 -> 清算池/AMM -> NOV 结算 -> 执行记账 -> 国库储备/分账`

### 6.3 边界约束

- 必须有报价有效期、滑点保护、流动性不足回退。
- 费用扣收不可直接绕开国库结算链路。

## 7. 当前代码差距（P0 可执行）

1. 原生 `tx_wire` 仍偏 transfer，需升级为原生执行/治理可表达结构。  
2. 原生 `nov_*` 入口虽已存在基础能力，但 NOV 原生执行与费用术语仍需进一步“主链优先化”。  
3. 多币支付路由、自动兑换、国库结算目前还不是统一主线模块（需新增 payment router + quote + clearing + treasury settlement 组合）。

## 8. 术语冻结

- 品牌：`NOVOVM`  
- 技术简称：`NVM`  
- 基础货币：`NOV`  
- 原生收费术语：`Execution Fee`  
- EVM 层 `gas` 为兼容字段，不代表 NOV 原生经济术语

## 9. 执行优先级（仅列下一刀）

1. 落 `NOV` 原生支付/清算路由草案（crate 级接口草图）。  
2. 升级原生 tx wire（表达 Execute/Governance，不再 transfer-only）。  
3. 补 M2 风险边界（抵押率、清算线、国库兜底与暂停开关）。  

---

本文件用于“先对齐货币制度，再落代码”，后续代码变更需遵守本文件口径，除非有新版决议文档替代。

