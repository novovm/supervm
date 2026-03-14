# NOVOVM 统一账户 U2/U3/U4 冻结交付物（2026-03-13）

## 1. 范围与口径

- 本文用于冻结 U2/U3/U4 的最小交付，不新增工程化包装层。
- 内部主链路保持：`gateway -> opsw1 -> novovm-node -> AOEM`。
- 证据以 `2026-03-13` 同机复跑产物为准。

---

## 2. U2 路由决策表（冻结）

| 顺序 | 决策点 | 通过条件 | 拒绝语义（固定） | 输出 |
|---|---|---|---|---|
| 1 | 协议入口识别 | `eth_* / web30_*` 可识别 | `unsupported_protocol_rejected` | 协议类型 |
| 2 | Persona 识别 | `evm/web30/...` 可解析 | `persona_unresolved` | Persona 类型 |
| 3 | 签名域校验 | 域与入口一致 | `domain_mismatch_rejected` | 域校验结果 |
| 4 | UCA 定位 | 绑定索引可反查 owner | `binding_owner_not_found` | `uca_id` |
| 5 | 绑定约束校验 | 唯一性 + 冷却期通过 | `binding_conflict_rejected` / `binding_cooldown_rejected` | 绑定合法性 |
| 6 | 权限校验 | Owner/Delegate/SessionKey 权限匹配 | `permission_denied` | 授权结果 |
| 7 | nonce/replay 校验 | Persona 级 nonce 单调递增 | `nonce_replay_rejected` | nonce 结果 |
| 8 | Type4（7702）策略校验 | 按策略 `supported/rejected/degraded` | `type4_policy_rejected` | Type4 决策 |
| 9 | 路由落点决策 | `eth_*` 仅单链；跨链原子走 `web30_*` | `persona_boundary_rejected` | 执行落点 + 审计事件 |

冻结约束：

1. 决策顺序固定，不允许跳步短路。
2. 拒绝语义固定，禁止同类错误多口径返回。
3. 审计事件要求“成功/拒绝均落盘”。

---

## 3. U3 最小回归集（冻结）

| 用例 | 覆盖目标 | 失败级别 | 证据来源 |
|---|---|---|---|
| UA-G01/02/03 | 绑定成功、冲突拒绝、冷却期拒绝 | BlockMerge | `unified-account-gate-summary.json` |
| UA-G04/05 | 签名域隔离 | BlockMerge | `unified-account-gate-summary.json` |
| UA-G06/07 | nonce 重放/逆序拒绝 | BlockMerge | `unified-account-gate-summary.json` |
| UA-G08/09 | Delegate/SessionKey 越权拒绝 | BlockMerge | `unified-account-gate-summary.json` |
| UA-G10/11 | Persona 边界（`eth_*` 与 `web30_*`） | BlockRelease | `unified-account-gate-summary.json` |
| UA-G12/13/14 | Type4 支持/拒绝/混用限制 | BlockRelease | `unified-account-gate-summary.json` |
| UA-G15 | Persona 唯一性冲突阻断 | BlockMerge | `unified-account-gate-summary.json` |
| UA-G16 | 恢复/撤销审计事件 | Warn | `unified-account-gate-summary.json` |
| fuzz-min-tx-wire | 交易解码抗异常输入 | Warn | `fuzz-min-gate-summary.json` |
| fuzz-min-rpc-params | RPC 参数解析抗异常输入 | Warn | `fuzz-min-gate-summary.json` |

固定执行入口：

1. `scripts/migration/run_unified_account_gate.ps1`
2. `scripts/migration/run_fuzz_min_gate.ps1`

---

## 4. U4 生态兼容声明（冻结）

| 生态面 | 当前声明 | 说明 |
|---|---|---|
| `eth_*` 写入 | 支持单链语义 | 不承载跨链原子语义，跨链字段应拒绝或转 `web30_*` |
| `web30_*` 写入 | 支持协调语义 | 原子协调在 `web30_*` 完成，不污染 `eth_*` nonce 语义 |
| 查询兼容 | 保持 EVM 常用查询面 | 对外 JSON-RPC 兼容；对内仍走二进制主线 |
| Type4（7702） | 策略化支持 | 必须显式 `supported/rejected/degraded`；默认禁止与 delegate/session 混用 |
| 统一账户主控 | UCA 为唯一主身份 | Persona 地址仅视图与路由入口，不替代 UCA 主身份 |

依赖闭环声明（U4）：

1. WP-10（EVM Persona）已进入 `ReadyForMerge/生产主线收口`。
2. WP-11（WEB30 边界）已具备与 `eth_*` 语义隔离的生产约束。
3. WP-13（多链适配）按主能力台账为 `Done`，满足 U4 最小闭环前置。

---

## 5. UA-A03/A04/A09/A10 最小闭环定义（冻结）

### UA-A03（数字账户号段扩展）

- 冻结号段策略：保留系统保留段与业务可分配段，避免与历史 UCA 编号冲突。
- 当前要求：先完成策略冻结与输入校验口径，不把号段策略并入共识关键路径。

### UA-A04（KYC 扩展域策略）

- 冻结边界：KYC 为扩展域策略，不进入共识关键路径。
- 当前要求：仅允许作为账户策略扩展字段参与路由判定，不改变基础执行语义。

### UA-A09（Type4 账户约束）

- 冻结策略：`supported/rejected/degraded` 三态显式声明。
- 默认策略：`rejected` 或仅在受控条件下 `supported`，并保持固定拒绝语义。
- 混用约束：默认禁止 Type4 与 `Delegate/SessionKey` 混用。

### UA-A10（存储键空间规范）

- 冻结约束：统一账户键空间与链状态键空间必须隔离。
- 当前主约束：`ua_store:*` 专属前缀 + dedicated CF（`ua_store_state_v2`/`ua_store_audit_v2`）。
- 审计游标独立：`ua_store:audit:flushed_event_count:v1`，禁止复用链状态游标。

---

## 6. 证据索引

1. `artifacts/migration/week1-2026-03-13/unified-account-gate-baseline/unified-account-gate-summary.json`
2. `artifacts/migration/week1-2026-03-13/unified-account-gate-baseline/unified-account-gate-summary.md`
3. `artifacts/migration/week1-2026-03-13/fuzz-min-gate/fuzz-min-gate-summary.json`
4. `docs_CN/UNIFIEDACCOUNT/NOVOVM-UNIFIED-ACCOUNT-SPEC-v1-2026-03-06.md`
5. `docs_CN/Adapters/EVM/NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md`
