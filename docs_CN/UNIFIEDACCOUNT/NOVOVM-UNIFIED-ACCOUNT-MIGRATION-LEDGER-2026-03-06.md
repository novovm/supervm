# NOVOVM 统一账户迁移进度台账（SUPERVM）- 2026-03-06

## 1. 台账说明

用途：跟踪 `SVM2026 -> SUPERVM` 统一账户迁移进度，并与主台账状态口径一致：
`NotStarted / InProgress / ReadyForMerge / Blocked / Done`。

生产优先口径（2026-03-09 修订）：

1. 完成度以生产主线接线为准（`gateway -> opsw1 -> novovm-node -> AOEM`）。
2. gate/snapshot/rc 产物仅用于回归与排障，不单独作为“功能完成”判据。
3. 若“门禁绿”与“主线未接线”冲突，以主线状态为准，视为未完成。

---

## 2. Domain Scan（UA 专项）

| Domain | Status | Owner | Blocking Dependency | Deliverable | Done Criteria | Current Evidence |
|---|---|---|---|---|---|---|
| U0 基线审计域 | InProgress | Architecture | 无 | 审计快照 | SUPERVM/SVM2026 差异可评审 | 审计快照已落盘 |
| U1 账户契约域 | InProgress | Protocol | U0 | Spec v1 | UCA/绑定/策略/唯一性规则冻结 | Spec v1 已创建 |
| U2 路由策略域 | NotStarted | Runtime | U1 | Router 决策表 | 决策顺序与拒绝语义冻结 | 待输出 |
| U3 回归验证域（辅助） | NotStarted | QA + Security | U1/U2 | 最小回归用例集 | 可快速复现关键失败，不阻断主线接线 | 待输出 |
| U4 生态对接域 | NotStarted | Adapter Team | U2/U3 | 兼容声明 | 与 WP-10/WP-11/WP-13 依赖闭环 | 待联动 |

---

## 3. 能力迁移矩阵（UA-A01 ~ UA-A13）

注：历史 `signal/gate` 字段保留用于追溯旧记录；当前统一以生产主线是否接线完成为判据。

| ID | Capability | Status | Owner | Blocking Dependency | Deliverable | Done Criteria | Next Production Step | Evidence | Updated |
|---|---|---|---|---|---|---|---|---|---|
| UA-A01 | 双标识账户模型（UCA+Persona） | InProgress | Protocol | U0 | Spec 身份章节 | 明确主身份与视图地址关系 | `ua_mapping_signal` | 审计快照 + Spec v1 | 2026-03-06 |
| UA-A02 | 多链地址绑定索引 | InProgress | Runtime | UA-A01 | 绑定索引设计 | 支持绑定/解绑/反查 | `ua_mapping_signal` | 审计快照 + Spec v1 | 2026-03-06 |
| UA-A03 | 数字账户号段扩展 | NotStarted | Protocol | UA-A01 | ID Policy 规范 | 号段策略不影响主路径 | `ua_id_policy_signal` | SVM2026 审计 | 2026-03-06 |
| UA-A04 | KYC 扩展域策略 | NotStarted | Governance | UA-A01 | KYC Policy 说明 | 默认不入共识关键路径 | `ua_kyc_policy_signal` | 审计快照 | 2026-03-06 |
| UA-A05 | 签名域隔离 | InProgress | Security | UA-A01 | Signature Domain 规范 | 跨域验签必失败 | `ua_signature_domain_signal` | Spec v1 | 2026-03-06 |
| UA-A06 | Nonce/Replay 策略 | InProgress | Runtime | UA-A01 | Nonce Policy 规范 | Persona 级 nonce + replay 拒绝 | `ua_nonce_replay_signal` | Spec v1 | 2026-03-06 |
| UA-A07 | 权限与授权模型 | InProgress | Security | UA-A01 | Permission 规范 | Owner/Delegate/SessionKey 权限边界冻结 | `ua_permission_signal` | Spec v1 | 2026-03-06 |
| UA-A08 | 原子协调边界 | ReadyForMerge | Protocol | UA-A01 | 边界策略说明 | `eth_*` 不承载跨链原子语义 | `ua_persona_boundary_signal` | `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-ga-next-rerun10/snapshot/acceptance-gate-full/unified-account-gate/unified-account-gate-summary.json`（UA-G10/UA-G11 通过） | 2026-03-09 |
| UA-A09 | Type 4（7702）账户约束 | NotStarted | Protocol + Security | UA-A05/UA-A07 | Type4 Policy 文档 | 明确输入受理、签名校验、拒绝错误码、降级策略、与代理/会话密钥混用限制 | `ua_type4_policy_signal` | EVM 文档联动 | 2026-03-06 |
| UA-A10 | 存储键空间规范 | NotStarted | Runtime | UA-A01/UA-A02 | Storage Key 规范 | 账户键空间与链状态键空间分离 | `ua_storage_key_signal` | SVM2026 审计 | 2026-03-06 |
| UA-A11 | 与 EVM WP-10 联动 | InProgress | Architecture | UA-A01~A09 | 依赖闭环记录 | WP 依赖状态一致 | `ua_dependency_signal` | EVM PLAN/LEDGER | 2026-03-06 |
| UA-A12 | RC 收口 | ReadyForMerge | Release | UA-A01~A11 | RC 候选包 | 门禁通过且兼容声明完成 | `ua_rc_candidate_gate` | `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/rc-candidate.json`（`overall_pass=true`） | 2026-03-09 |
| UA-A13 | 唯一性与冲突约束 | InProgress | Security + Runtime | UA-A01/UA-A02 | Unique/Conflict 规范 | `1 PersonaAddress -> 1 UCA` 强约束 + 冲突拒绝与事件闭环 | `ua_uniqueness_conflict_signal` | Spec v1 | 2026-03-06 |

---

## 4. 风险与阻塞

| ID | Type | Description | Impact | Mitigation | Status |
|---|---|---|---|---|---|
| UR-01 | 代码成熟度风险 | SVM2026 关联模块存在 TODO/占位实现 | 直接平移会引入生产不确定性 | 迁模型不迁实现 | Open |
| UR-02 | 协议污染风险 | 跨链原子语义若暴露到 `eth_*` | 破坏 EVM Persona 预期 | 原子协调固定在 `web30_*` | Open |
| UR-03 | 签名域风险 | 缺少域隔离会导致跨协议重放 | 安全边界失效 | 在主线路由中强制域隔离阻断 | Open |
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
| 2026-03-09 | UA-A08/UA-A12 状态收口 | 以 `ua_persona_boundary_signal` 与严格 RC 证据将两项升级到 `ReadyForMerge` | `unified-account-gate-summary.json` + `rc-ua-selfguard-rocksdb-20260308-000948/rc-candidate.json` | Accepted |
| 2026-03-09 | 外部入口边界回退与架构收敛 | 回退 `novovm-node` 入口耦合改动，统一为“外部 RPC 仅在边界层，对内统一二进制流水线接入 UA/Adapter/AOEM” | `docs_CN/Adapters/EVM/NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md` + `crates/novovm-node/src/bin/novovm-node.rs` | Accepted |
| 2026-03-09 | UA/EVM 二进制 ingress 主线接线补齐 | 新增 `NOVOVM_OPS_WIRE_DIR` 批量消费能力，允许边界层输出 `.opsw1` 直接进入 AOEM 主路径，继续保持内部无 RPC/HTTP 的高速流水线约束 | `crates/novovm-node/src/bin/novovm-node.rs` + `crates/novovm-edge-gateway/src/main.rs` | Accepted |
| 2026-03-09 | UA 外部入口到主链路一键运行路径落地 | 新增运行脚本统一打通 `gateway -> opsw1 -> novovm-node`，保持 UA 路由在边界层完成、内部保持二进制流水线 | `scripts/migration/run_gateway_node_pipeline.ps1` | Accepted |
| 2026-03-09 | UA 最小真实链路冒烟通过 | `ua_createUca` 与 `ua_bindPersona` 生效后，`eth_sendRawTransaction` 成功落地 `.opsw1` 并进入主线消费归档 | `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/ingress/done/batch-20260309063719351/ingress-1773009433843-0.opsw1` | Accepted |
| 2026-03-09 | UA 管理入口生产补齐（gateway） | 边界层补齐 `ua_rotatePrimaryKey / ua_revokePersona / ua_getBindingOwner / ua_setPolicy`，并新增 `eth_getTransactionCount` 直接读取 UA nonce，统一账户管理与 EVM 入口同层处理 | `crates/novovm-edge-gateway/src/main.rs` | Accepted |
| 2026-03-09 | UA+WEB30 生产 ingress 接线补齐（无 gate 包装） | 边界层新增 `web30_sendRawTransaction / web30_sendTransaction`，与 `eth_sendRawTransaction` 共用 `ops_wire_v1` 编码并统一进入 `novovm-node -> AOEM` 主路径；内部仍保持二进制流水线 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/ingress/done/batch-20260309074046040/ingress-1773013144163-0.opsw1` + `artifacts/ingress/done/batch-20260309074046040/ingress-1773013144214-1.opsw1` | Accepted |
| 2026-03-09 | `web30_sendTransaction`（非 raw）纳入 smoke+pipeline 证据链并固定回归样例 | smoke 默认加载固定样例 `gateway-web30-nonraw-regression-sample-v1.json`，完成 `eth raw + web30 raw + web30 nonraw` 三笔 `.opsw1` 生成，并在同次执行中自动触发 pipeline 消费归档 | `scripts/migration/baselines/gateway-web30-nonraw-regression-sample-v1.json` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309075057706/ingress-1773013857302-0.opsw1` + `artifacts/ingress/done/batch-20260309075057706/ingress-1773013857357-1.opsw1` + `artifacts/ingress/done/batch-20260309075057706/ingress-1773013857437-2.opsw1` | Accepted |
| 2026-03-09 | `eth_sendTransaction`（非 raw）生产接线补齐并入同一证据链 | 边界层新增 `eth_sendTransaction` 归一化接线（非 raw），与 `eth_sendRawTransaction / web30_sendRawTransaction / web30_sendTransaction` 同步写入 `.opsw1` 并在同次 smoke 中自动 pipeline 消费归档；固定样例 `gateway-eth-nonraw-regression-sample-v1.json` | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/baselines/gateway-eth-nonraw-regression-sample-v1.json` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309080726585/ingress-1773014846079-0.opsw1` + `artifacts/ingress/done/batch-20260309080726585/ingress-1773014846148-1.opsw1` + `artifacts/ingress/done/batch-20260309080726585/ingress-1773014846204-2.opsw1` + `artifacts/ingress/done/batch-20260309080726585/ingress-1773014846264-3.opsw1` | Accepted |
| 2026-03-09 | `eth_sendTransaction` `tx` 子对象形态兼容回归固定化 | smoke 增加 `eth_sendTransaction` 的 `tx` 子对象样例（`chainId/nonce/from/to` 置于 `params.tx`），验证边界层字段回退解析后仍写入 `.opsw1`，并与其它 4 笔请求在同次 pipeline 消费归档 | `scripts/migration/baselines/gateway-eth-nonraw-tx-object-regression-sample-v1.json` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403613-0.opsw1` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403685-1.opsw1` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403751-2.opsw1` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403824-3.opsw1` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403882-4.opsw1` | Accepted |
| 2026-03-09 | `eth_sendTransaction` 标准数组参数形态兼容落地（`params: [tx]`） | 边界层新增标准 JSON-RPC 数组参数形态兼容，`eth_sendTransaction` 的 `params:[{...}]` 与 `eth_sendRawTransaction/web30_*` 同批次写入 `.opsw1` 并进入统一 `gateway -> opsw1 -> novovm-node -> AOEM` 主链路，无内部 RPC/HTTP 回退 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/baselines/gateway-eth-nonraw-array-params-regression-sample-v1.json` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323027-0.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323096-1.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323157-2.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323227-3.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323287-4.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323352-5.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323410-6.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323482-7.opsw1` | Accepted |
| 2026-03-09 | EVM 查询入口补齐（`eth_getTransactionByHash` / `eth_getTransactionReceipt`） | 边界层补齐两条 EVM 查询方法，返回 gateway 已接收交易的 `pending` 视图，确保对外 JSON-RPC 完整性；内部仍保持 `gateway -> opsw1 -> novovm-node -> AOEM` 二进制主线不变。smoke 同次完成 13 请求（含 2 条查询）并通过 pipeline 归档 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017732912-0.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017732986-1.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733054-2.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733118-3.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733257-4.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733326-5.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733388-6.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733455-7.opsw1` | Accepted |
| 2026-03-09 | gateway ETH 查询索引可选 rocksdb 持久化落地 | 在不改变内部二进制主线的前提下，新增 gateway 查询索引后端开关（默认 `memory`，可切 `rocksdb`），并验证重启后 `eth_getTransactionByHash` 仍可命中已接收交易 | `crates/novovm-edge-gateway/src/main.rs` + `artifacts/migration/unifiedaccount/gateway-eth-tx-index-rocksdb-restart-smoke-summary.json` | Accepted |
| 2026-03-09 | gateway EVM 返回语义收敛（标准 JSON-RPC） | `eth_sendRawTransaction/eth_sendTransaction` 返回值收敛为交易哈希字符串，`eth_getTransactionCount` 收敛为 hex quantity，减少外部钱包/SDK 兼容摩擦；内部二进制主线不变 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` | Accepted |
| 2026-03-09 | `eth_chainId/net_version` 边界兼容补齐（无内部链路改造） | gateway 补齐 EVM 基础网络查询别名，仅在外部边界层处理并直接返回（不进入 `.opsw1`），保持内部 `gateway -> opsw1 -> novovm-node -> AOEM` 二进制主线不变 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` | Accepted |
| 2026-03-09 | `eth_gasPrice/eth_estimateGas` 边界兼容补齐（无内部链路改造） | gateway 补齐 EVM 常用费用查询接口，`eth_gasPrice` 与 `eth_estimateGas` 仅在边界层返回结果，不触发内部 `.opsw1` 写入，继续保持内部二进制流水线高性能路径 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` | Accepted |
| 2026-03-09 | `eth_getCode/eth_getStorageAt` 边界兼容补齐（无内部链路改造） | gateway 补齐 EVM 常用只读查询接口：`eth_getCode` 与 `eth_getStorageAt`，仅用于边界协议兼容，不进入 `.opsw1` 与内部 AOEM 主线，维持 UA/EVM 内部二进制流水线不变 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` | Accepted |

---

## 6. 2026-03-07 历史工程化记录（不作为完成判据）

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
2. 生产链路证据优先落盘：`artifacts/ingress/`（门禁产物仅作辅助，不单独驱动里程碑）。
3. 与 EVM WP-10 的依赖变更必须同日更新两侧台账。
