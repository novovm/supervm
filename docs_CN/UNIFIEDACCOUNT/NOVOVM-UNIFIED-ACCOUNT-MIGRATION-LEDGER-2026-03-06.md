# NOVOVM 统一账户迁移进度台账（SUPERVM）- 2026-03-06

## 1. 台账说明

用途：跟踪 `SVM2026 -> SUPERVM` 统一账户迁移进度，并与主台账状态口径一致：
`NotStarted / InProgress / ReadyForMerge / Blocked / Done`。

---

## 2. Domain Scan（UA 专项）

| Domain | Status | Owner | Blocking Dependency | Deliverable | Done Criteria | Current Evidence |
|---|---|---|---|---|---|---|
| U0 基线审计域 | InProgress | Architecture | 无 | 审计快照 | SUPERVM/SVM2026 差异可评审 | 审计快照已落盘 |
| U1 账户契约域 | InProgress | Protocol | U0 | Spec v1 | UCA/绑定/策略/唯一性规则冻结 | Spec v1 已创建 |
| U2 路由策略域 | NotStarted | Runtime | U1 | Router 决策表 | 决策顺序与拒绝语义冻结 | 待输出 |
| U3 门禁证据域 | NotStarted | QA + Security | U1/U2 | Gate Matrix v1 | 输入/期望/失败级别/证据路径冻结 | 待输出 |
| U4 生态对接域 | NotStarted | Adapter Team | U2/U3 | 兼容声明 | 与 WP-10/WP-11/WP-13 依赖闭环 | 待联动 |

---

## 3. 能力迁移矩阵（UA-A01 ~ UA-A13）

| ID | Capability | Status | Owner | Blocking Dependency | Deliverable | Done Criteria | Next Gate | Evidence | Updated |
|---|---|---|---|---|---|---|---|---|---|
| UA-A01 | 双标识账户模型（UCA+Persona） | InProgress | Protocol | U0 | Spec 身份章节 | 明确主身份与视图地址关系 | `ua_mapping_signal` | 审计快照 + Spec v1 | 2026-03-06 |
| UA-A02 | 多链地址绑定索引 | InProgress | Runtime | UA-A01 | 绑定索引设计 | 支持绑定/解绑/反查 | `ua_mapping_signal` | 审计快照 + Spec v1 | 2026-03-06 |
| UA-A03 | 数字账户号段扩展 | NotStarted | Protocol | UA-A01 | ID Policy 规范 | 号段策略不影响主路径 | `ua_id_policy_signal` | SVM2026 审计 | 2026-03-06 |
| UA-A04 | KYC 扩展域策略 | NotStarted | Governance | UA-A01 | KYC Policy 说明 | 默认不入共识关键路径 | `ua_kyc_policy_signal` | 审计快照 | 2026-03-06 |
| UA-A05 | 签名域隔离 | InProgress | Security | UA-A01 | Signature Domain 规范 | 跨域验签必失败 | `ua_signature_domain_signal` | Spec v1 | 2026-03-06 |
| UA-A06 | Nonce/Replay 策略 | InProgress | Runtime | UA-A01 | Nonce Policy 规范 | Persona 级 nonce + replay 拒绝 | `ua_nonce_replay_signal` | Spec v1 | 2026-03-06 |
| UA-A07 | 权限与授权模型 | InProgress | Security | UA-A01 | Permission 规范 | Owner/Delegate/SessionKey 权限边界冻结 | `ua_permission_signal` | Spec v1 | 2026-03-06 |
| UA-A08 | 原子协调边界 | NotStarted | Protocol | UA-A01 | 边界策略说明 | `eth_*` 不承载跨链原子语义 | `ua_persona_boundary_signal` | 迁移方案 | 2026-03-06 |
| UA-A09 | Type 4（7702）账户约束 | NotStarted | Protocol + Security | UA-A05/UA-A07 | Type4 Policy 文档 | 明确输入受理、签名校验、拒绝错误码、降级策略、与代理/会话密钥混用限制 | `ua_type4_policy_signal` | EVM 文档联动 | 2026-03-06 |
| UA-A10 | 存储键空间规范 | NotStarted | Runtime | UA-A01/UA-A02 | Storage Key 规范 | 账户键空间与链状态键空间分离 | `ua_storage_key_signal` | SVM2026 审计 | 2026-03-06 |
| UA-A11 | 与 EVM WP-10 联动 | InProgress | Architecture | UA-A01~A09 | 依赖闭环记录 | WP 依赖状态一致 | `ua_dependency_signal` | EVM PLAN/LEDGER | 2026-03-06 |
| UA-A12 | RC 收口 | NotStarted | Release | UA-A01~A11 | RC 候选包 | 门禁通过且兼容声明完成 | `ua_rc_candidate_gate` | 待生成 | 2026-03-06 |
| UA-A13 | 唯一性与冲突约束 | InProgress | Security + Runtime | UA-A01/UA-A02 | Unique/Conflict 规范 | `1 PersonaAddress -> 1 UCA` 强约束 + 冲突拒绝与事件闭环 | `ua_uniqueness_conflict_signal` | Spec v1 | 2026-03-06 |

---

## 4. 风险与阻塞

| ID | Type | Description | Impact | Mitigation | Status |
|---|---|---|---|---|---|
| UR-01 | 代码成熟度风险 | SVM2026 关联模块存在 TODO/占位实现 | 直接平移会引入生产不确定性 | 迁模型不迁实现 | Open |
| UR-02 | 协议污染风险 | 跨链原子语义若暴露到 `eth_*` | 破坏 EVM Persona 预期 | 原子协调固定在 `web30_*` | Open |
| UR-03 | 签名域风险 | 缺少域隔离会导致跨协议重放 | 安全边界失效 | 域隔离 gate 强制阻断 | Open |
| UR-04 | nonce 风险 | Persona/全局 nonce 口径不清 | 冲突与双花风险 | 固化 Persona 级 nonce | Open |
| UR-05 | 规范漂移风险 | 文档与实现路径不一致 | 迁移误导与返工 | 以 SUPERVM 口径冻结规范 | Open |
| UR-06 | 恢复风险 | 主密钥轮换/设备丢失恢复流程未定义 | 账户不可恢复或误恢复 | 冻结恢复流程 + 事件审计 | Open |
| UR-07 | 撤销风险 | 地址绑定撤销/重绑/冷却期策略不完整 | 账户劫持或脏映射残留 | 冻结撤销策略 + 冷却机制 | Open |

---

## 5. 里程碑记录

| Date | Milestone | Decision | Evidence | Result |
|---|---|---|---|---|
| 2026-03-06 | UNIFIEDACCOUNT 迁移启动 | 统一账户优先于 EVM WP-10 实施 | 目录创建与台账初始化 | Accepted |
| 2026-03-06 | 审计快照完成 | 明确 SUPERVM 未落地、SVM2026 为设计资产来源 | 审计快照文档 | Accepted |
| 2026-03-06 | 方案文档冻结（v1） | 采用“迁模型，不迁实验实现” | 迁移方案文档 | Accepted |
| 2026-03-06 | 规范文档创建（Spec v1） | 冻结统一账户核心约束草案 | Spec v1 文档 | Accepted |

---

## 6. 2026-03-07 落地进展快照

| Item | Status | Evidence | Notes |
|---|---|---|---|
| UA-G01~UA-G16 自动化门禁 | Done | `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-ga-next-rerun10/snapshot/acceptance-gate-full/unified-account-gate/unified-account-gate-summary.json` | `passed_cases=16/16` |
| full_snapshot_ga_v1 严格口径 RC | ReadyForMerge | `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-ga-next-rerun10/rc-candidate.json` | `overall_pass=true` |
| full_snapshot_ga_v1 本机稳定口径 RC（-7%） | Done | `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-ga-next-local-perf7/rc-candidate.json` | 用于本地持续开发联调 |
| foreign/nav 外部源门禁 | Done | `foreign-rate-source-gate-summary.json` / `nav-valuation-source-gate-summary.json`（同上 acceptance 目录） | fallback 竞态与聚合脚本问题已修复 |
| adapter stability | Done | `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-ga-next-rerun10/snapshot/acceptance-gate-full/adapter-stability-gate/adapter-stability-summary.json` | 已加入已知抖动重试 |
| D2/D3 持久化路径稳定性 | Done | `scripts/migration/run_functional_consistency.ps1` | 短路径 + per-run session，避免 Windows 路径过长与状态串扰 |
| full_snapshot_v2 严格口径 RC（UA plugin self-guard + rocksdb 场景） | ReadyForMerge | `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/rc-candidate.json` | `overall_pass=true`，并覆盖四链 compare + UA gate + strict performance |
| plugin-side standalone self-guard rocksdb 冒烟 | Done | `artifacts/migration/unifiedaccount/plugin-selfguard-standalone-smoke-20260308-001323/plugin-selfguard-standalone-smoke-summary.json` | `tests::plugin_apply_v2_self_guard_rejects_replay_nonce` 通过，store/audit rocksdb 均落盘 |

## 7. 更新规则

1. 每次完成 UA-Axx，必须同步更新 `Status/Owner/Dependency/Deliverable/Evidence`。
2. 门禁证据统一落盘：`artifacts/migration/unifiedaccount/`。
3. 与 EVM WP-10 的依赖变更必须同日更新两侧台账。
