# NOVOVM Account Protocol v1（2026-04-20）

Status: AUTHORITATIVE PROTOCOL (`v1-min`)  
Scope: 统一账户主体协议在真实 `novovm-node` 主线产物上的当前有效协议面

## 目的

本文件用于冻结 NOVOVM 当前统一账户的最小主体协议面，只描述已经进入真实主线、可验证、可观测的当前结果。

本文件不描述迁移过程，不把 legacy 入口写成对外主叙述，也不把尚未落地的完整账户宇宙提前冻结成公开协议事实。

## 设计定位

NOVOVM 当前把统一账户定义为：

`系统主体协议`

而不是：

`多链钱包集合`

当前阶段成立的是：

- 一个统一主体方向：`account_id`
- 一套统一账户入口：真实 `novovm-node -> mainline_query -> unified_account_surface`
- 一组当前可用的身份、绑定、策略、nonce、审计与路由能力
- 一组当前可用的统一资产视图读面：`account_balance / account_assets`

当前阶段不成立的是：

- proof-root 账户状态树
- 新的统一账户资产账本
- 隐私子账户主实现
- 完整映射资产结算协议

## 最终结论

统一账户已经进入真实 `novovm-node` 主线入口，并可作为当前系统主体协议的最小公开口径发布。

当前可成立的项目状态：

- `统一账户真实主线入口`：已接入
- `主体语义方向`：`account_id` 为规范主体，`uca_id` 为兼容别名
- `统一账户读写与路由能力`：已接入真实主线
- `统一账户门禁`：已通过真实入口级 gate
- `协议冻结级别`：`v1-min`

## 稳定基线结论

当前统一账户主线已经完成以下最小生产闭环：

- `主体层`：`account_id` 作为唯一主体
- `资产视图层`：`account_balance / account_assets`
- `密钥能力层`：`Cut A / KeyAlgo`
- `执行策略层`：`Cut C / ExecutionPolicy`

因此，当前权威工程结论固定为：

`统一账户主线已完成主体层、资产视图层、密钥能力层与执行策略层的最小生产闭环；AccountMode / Cut B 保持为非核心可选标签层，默认 No-Go；Phase 4 继续保持触发式治理，当前 No-Go。`

这意味着当前基线已经满足长期运行条件：

- 当前不再需要重新打开统一账户架构重设计议题
- 当前不再默认推进新的结构层、统一资产账本层或 root 层
- 后续工程动作只分为两类：业务接入并使用当前能力；或等待真实触发信号后，再决定是否开启 `Cut B` 或 `Phase 4` 的最小实现切片

如果只用一句工程口径收尾，则当前结论是：

`这条线现在已经能长期运行，不需要再做结构性推进。`

## 当前真实主线路径

当前统一账户真实产物路径为：

`novovm-node (bin) -> mainline_query -> unified_account_surface -> UnifiedAccountRouter`

这意味着当前统一账户不再只停留在 legacy `main.rs` 或旧公共面上，而是已经进入当前真实主线产物。

关键接线点：

- `crates/novovm-node/src/bin/novovm-node.rs`
- `crates/novovm-node/src/bin/supervm-mainline-query.rs`
- `crates/novovm-node/src/mainline_query.rs`
- `crates/novovm-node/src/unified_account_surface.rs`
- `crates/novovm-adapter-api/src/unified_account.rs`

## v1-min 冻结范围

本次 `v1-min` 只冻结以下协议面：

1. `account_id` 作为统一主体方向
2. 主身份绑定与主密钥轮换
3. 派生地址 / persona 绑定与撤销
4. 策略约束、权限边界与路由决策
5. nonce 归属与重放保护语义
6. 审计事件与可追溯查询面
7. 真实主线入口的统一账户方法合同

本次 `v1-min` 不冻结以下事项：

- `identity_root / key_root / asset_root / permission_root`
- 新的统一账户资产账本
- 隐私子账户完整语义
- 完整映射资产 mint/burn 结算协议
- 完整 `recover_account` 生命周期状态机

## 规范主体规则

当前规范主体规则如下：

- `account_id`：当前统一主体的规范口径
- `uca_id`：当前迁移期兼容别名
- 地址 / persona：派生表示，不是公开协议中的最终主体定义

这意味着当前迁移方向是：

`address-driven + account-attached`

逐步迁到：

`account-driven + address-derived`

## 当前已封盘的方法合同

当前进入真实主线入口的统一账户方法如下：

| 方法 | 当前语义 | 状态 |
| --- | --- | --- |
| `ua_createUca` | 创建统一账户主体 | 已封盘 |
| `ua_rotatePrimaryKey` | 轮换主密钥 | 已封盘 |
| `ua_setPolicy` | 更新账户策略 | 已封盘 |
| `ua_bindPersona` | 绑定 persona / 派生地址 | 已封盘 |
| `ua_revokePersona` | 撤销 persona / 派生地址 | 已封盘 |
| `ua_getBindingOwner` | 查询 persona 所属主体 | 已封盘 |
| `ua_getAuditEvents` | 查询统一账户审计事件 | 已封盘 |
| `ua_getAccount` | 查询主体账户信息 | 已封盘 |
| `ua_getPolicy` | 查询主体策略 | 已封盘 |
| `ua_listBindings` | 查询主体绑定列表 | 已封盘 |
| `ua_getNextNonce` | 查询下一可用 nonce | 已封盘 |
| `ua_route` | 在当前策略边界下做主体路由决策 | 已封盘 |
| `account_balance` | 查询当前 `account_id` 的统一资产视图与结构化资产暴露组件 | 已封盘（Phase 3 / Cut 1，Cut 2 扩展） |
| `account_assets` | 查询当前 `account_id` 的统一资产清单 / 仓位 / treasury exposure 视图 | 已封盘（Phase 3 / Cut 1，Cut 2 扩展） |

## 当前已成立的协议事实

### 1）主体、绑定与唯一性

当前统一账户已经成立：

- 主体创建
- persona 唯一性约束
- 绑定冲突拒绝
- 撤销与冷却期重绑边界

### 2）签名域与 nonce 规则

当前统一账户已经成立：

- 签名域隔离
- chain 相关域约束
- nonce 重放拒绝
- nonce 逆序拒绝

### 3）权限与策略边界

当前统一账户已经成立：

- delegate / session key 权限边界
- 过期 session key 拒绝
- policy 驱动的路由判断
- Type4 模式边界

### 4）审计与持久化

当前统一账户已经成立：

- 统一账户 snapshot 持久化
- audit sink 持久化
- 真实入口级审计查询

这意味着当前统一账户不只是“能调”，而是：

`可落盘 + 可审计 + 可回放当前主体事实`

### 5）统一资产视图（只读）

当前统一账户已经成立：

- `account_balance`
- `account_assets`
- `ownership_subject = account_id`
- 基于现有资产源聚合的主线级资产可见性
- `components`
- `pledges`
- `treasury_exposures`
- 显式 `source`

当前统一资产视图成立的是：

`读取与聚合合同`

不是：

`新的统一资产账本`

当前仍未成立的是：

- `staking`
- unified asset ledger
- `asset_root`
- mapped asset settlement protocol
- privacy subaccount asset space

### 6）密钥能力与执行强约束

当前统一账户已经成立：

- `Cut A / KeyAlgo`
  - `key_algo` 元数据
  - 持有证明校验
  - 查询与审计可见性
- `Cut C / ExecutionPolicy`
  - 最小枚举：
    - `Standard`
    - `PqRequired`
    - `PrivacyRequired`
  - 主线唯一 resolve + enforcement
  - `PqRequired / PrivacyRequired` 的明确拒绝路径
  - `receipt / trace / audit` 中的 policy 可见性

当前成立的是：

`KeyAlgo + ExecutionPolicy 会真实决定执行是否允许`

当前仍未成立的是：

- `AccountMode`
- `Confidential` 完整路径
- `ExecutionPolicy` 反向触发任何资产账本或新状态源

## 当前未宣称完成的范围

本文件不宣称以下事项已经完成：

- 不宣称 proof-root 化账户状态树已经进入主线
- 不宣称统一账户资产账本已经成立
- 不宣称隐私子账户已经进入真实主线
- 不宣称完整恢复状态机已经封盘
- 不宣称当前统一资产视图已经等同于统一资产系统完成

换言之，当前 `v1-min` 成立的是：

`统一主体协议`

而不是：

`完整账户宇宙`

## 验收基线（2026-04-20 本地实际执行）

本次统一账户主线收口基于以下真实执行结果：

- `cargo fmt --all`
- `cargo check -p novovm-node`
- `cargo test -p novovm-node unified_account_gate_ua_g -- --nocapture`
  - 结果：`16 passed; 0 failed`
- `scripts/migration/run_unified_account_gate.ps1`
  - 结果：`pass: True`
  - 结果：`passed_cases: 16/16`

本次 gate 已运行在真实主线入口测试上，不再依赖 legacy `main.rs` 下的旧测试面。

## 当前读取顺序

对外读取当前统一账户能力时，建议按以下顺序理解：

1. 系统总览：`docs_CN/NOVOVM-NETWORK/NOVOVM-CURRENT-SYSTEM-ARCHITECTURE-2026-04-19.md`
2. 当前统一账户主体协议：本文
3. 原生执行、经济与治理入口封盘：
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-ECONOMIC-USER-SURFACE-SEAL-2026-04-18.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-USER-SURFACE-SEAL-2026-04-18.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-MLDSA87-EXTERNAL-VOTE-SEAL-2026-04-18.md`
4. 后续演进顺序：
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-v1-MIN-TO-v1-EVOLUTION-ROADMAP-2026-04-20.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTA-KEYALGO-SEAL-2026-04-20.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTA-KEYALGO-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-SEAL-2026-04-20.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
5. 统一资产视图当前封盘与后续门禁：
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT1-SEAL-2026-04-20.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT2-SEAL-2026-04-20.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md`

这意味着当前对外口径直接读取“当前主体协议、当前入口、当前边界和当前验证结果”，不需要先阅读开发过程文档。

## 建议对外口径

`NOVOVM Account Protocol v1-min 已把统一账户推进为真实主线中的主体协议：当前统一账户入口、主体绑定、策略、nonce、审计、统一资产视图读面，以及 KeyAlgo / ExecutionPolicy 的最小执行闭环都已进入真实 novovm-node 主线；当前仍不宣称统一资产账本、asset_root、隐私子账户与 proof-root 账户树已经完成。`
