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
- `M2`：信用扩张层，包含外部锁仓、储备或信用生成的 `nAsset` / `n*` 系列资产，例如 `NETH`、`NUSDT`、`NUSD`。

### 5.2 明确禁止

- 禁止把 `pETH / pUSDT / pSOL` 这类镜像资产放在 M1。
- 禁止绕过 NOV 结算直接把外币当内部结算币。
- 禁止把 `NETH/NUSDT/nAsset` 写成 NOV 或 M1；它们是 M2 存款/信用/映射资产。
- 禁止把外部锁仓事件直接解释为 NOV 铸造事件；NOV 铸造必须经过 Treasury policy / emission policy。

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

## 7. 上层经济法条（2026-06-13 补充冻结）

本节是 NOVOVM 货币制度的上层边界，优先级高于产品叙事。后续实现、钱包、DAPP、网站、EVM adapter 均不得绕开。

### 7.1 货币层级法条

1. `NOV` 是 NOVOVM 唯一基础货币、最终结算货币、矿工/算力结算货币，归属 `M0/M1`。
2. `NETH`、`NUSDT`、`nAsset` 是外部锁仓、储备或信用生成的 M2 资产，不进入 `M0/M1`。
3. M2 资产可以支付、抵押、赎回或进入信用扩张，但其风险与负债归属在 M2，不回写为 NOV 基础货币。
4. `NETH` 是锁仓 ETH 的 1:1 储备/存款凭证，不是 NOV，也不是自动铸 NOV 的中间态。
5. NOV 新增发行或矿工结算额度必须受 `reserve bucket`、`fee bucket`、`risk buffer`、`emission policy` 约束。

### 7.2 多资产支付与 NOV 结算法条

1. 用户可用白名单 M2 资产支付 Execution Fee。
2. 系统按协议清算价把支付资产折算为 `NOV value`。
3. 支付资产进入 `Treasury Reserve Pool`，中文统一称为“国库储备池 / 外汇储备池”。
4. 矿工/算力提供者只以 NOV 结算，避免收入碎片化。
5. 费用扣收、矿工结算、国库分账不得绕开 Treasury settlement。
6. 当前制度写入不声明真实 ETH lock、NOV mint、M2 credit 全自动闭环已完成。

### 7.3 Treasury / AMM / Oracle 职责

1. `Treasury` 命名保留，中文统一写作“国库”。
2. Treasury 负责协议清算、储备、风险缓冲、分桶和 NOV 结算。
3. AMM 负责市场价格发现、用户交易、套利收敛，不直接决定 Execution Fee 清算价。
4. AMM spot price 禁止进入协议清算。
5. 外部 oracle 只能作为治理许可参考源，用于偏离检测、熔断和兜底。
6. 外部 oracle 不能开放给任意第三方喂价，不能单独决定协议清算价。

## 8. 当前代码差距（P0 可执行）

1. 原生 `tx_wire` 仍偏 transfer，需升级为原生执行/治理可表达结构。  
2. 原生 `nov_*` 入口虽已存在基础能力，但 NOV 原生执行与费用术语仍需进一步“主链优先化”。  
3. 多币支付路由、清算、国库 settlement 已有 native execution store v1 主线；但仍不声明真实外部桥、真实 Ethereum lock proof 自动验证或多进程高并发账本入口完成。
4. 协议清算价 v1 已落代码：`P_epoch/P_pay/P_redeem` 按 epoch 固定，输入为显式 AMM TWAP、Treasury NAV、许可 oracle reference 和上一 epoch 价格；AMM spot 不参与清算。
5. 当前仍缺治理层 oracle 白名单管理、真实外部 reserve proof、完整桥接铸造/赎回自动化和高并发事务后端。

## 9. 术语冻结

- 品牌：`NOVOVM`  
- 技术简称：`NVM`  
- 基础货币：`NOV`  
- 原生收费术语：`Execution Fee`  
- EVM 层 `gas` 为兼容字段，不代表 NOV 原生经济术语
- 国库：`Treasury`
- 国库储备池 / 外汇储备池：`Treasury Reserve Pool`
- 协议清算价：`Protocol Clearing Price`
- AMM 市场价：`AMM Market Price`
- 许可参考源：`Governance-permitted Oracle Reference`

## 10. 执行优先级（仅列下一刀）

1. 落 `NOV` 原生支付/清算路由草案（crate 级接口草图）。  
2. 升级原生 tx wire（表达 Execute/Governance，不再 transfer-only）。  
3. 补 M2 风险边界（抵押率、清算线、国库兜底与暂停开关）。  
4. 补协议清算价 epoch 固定与抗攻击规则的实现计划。

---

本文件用于“先对齐货币制度，再落代码”，后续代码变更需遵守本文件口径，除非有新版决议文档替代。

