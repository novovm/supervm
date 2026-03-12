# NOVOVM MEV 对 EVM 上游能力需求清单（催办版）v1 - 2026-03-11

## 1. 目的

用于给 EVM 上游并行开发同事提供一个可执行、可验收、可催办的最小能力清单。  
本清单只包含 **MEV 必需且必须由 EVM 插件提供** 的能力，不包含 SUPERVM 已具备的重复能力。

适用范围：

1. 分支：`SUPERVM-MEV`
2. 路线：MEV 硬路线（Hard Route）
3. 阶段：M1~M5（M5 前 handover 为重点）

---

## 2. 排除项（SUPERVM 已有能力，不应重复建设）

以下能力不应由 EVM 插件重复开发：

1. AOEM 并行推演底座与执行框架
2. MEV 机会数学引擎（双边/三角核心算法）
3. MEV 风控策略与回归 smoke 框架
4. Shadow 管线、开发门禁脚本总框架
5. 主链共识与通用流水线壳

结论：EVM 插件只负责“以太坊语义与网络面能力”，不负责重写 MEV 内核能力。

---

## 3. 必需能力清单（EVM 插件）

状态口径（2026-03-11）来源：

- `artifacts/migration/mev/m5-handover-bundle-summary.json`
- `artifacts/migration/mev/m1a-shadow-smoke-summary.json`
- `artifacts/migration/mev/m1a-current-observability-snapshot.json`

| 优先级 | 能力ID | 能力项（EVM插件提供） | 最小能力面 | 在 MEV 中的作用 | 当前状态 | 阻断级别 |
|---|---|---|---|---|---|---|
| P0 | C-01 | 合约调用执行语义 | `eth_call` + 合约语义执行（含 Uniswap/Sushi 常用路径） | 机会识别与推演输入基础；没有它无法做真实套利决策 | 已具备（`contract_call_gate=true`） | 低 |
| P0 | C-04 | 公网广播提交路径 | `eth_sendRawTransaction` 可用并可验收 | 决策结果上链主路径；直接决定 M5 能否放行 | 未达标（`public_broadcast_gate=false`） | 高（主阻断） |
| P0 | C-02 | pending 事件流 | `eth_subscribe(newPendingTransactions)` | 实时机会发现主入口 | 未达 fullstack | 中高 |
| P0 | C-03 | txpool 快照 | `txpool_content`（最小 content） | 补足 pending 可见性，减少漏单与误判 | 未达 fullstack 双源 | 中高 |
| P0 | C-05 | 回执/错误码语义 | receipt 查询 + 错误分类稳定（nonce/intrinsic/replacement/unsupported） | 失败归因、重试策略、风控闭环 | 未达标（`HR-E05=false`） | 中 |
| P0 | C-05(扩) | logs/filter/subscribe 语义 | `eth_getLogs/newFilter/getFilterChanges/uninstallFilter/subscribe(logs)` | 事件驱动触发与池状态更新 | 未达标（`HR-E03=false`） | 中 |
| P1 | C-06 | TxType/Profile 策略位 | Type `0/1/2/3/4` 的支持/拒绝/降级显式策略 | 防止交易类型误判和策略穿透 | 部分具备（策略位框架已存在） | 中 |
| P1 | C-07 | UCA 对齐接口 | 签名域/nonce/权限口径一致 | 防 replay、权限越权、提交流程一致 | 并行对齐中 | 中 |

---

## 4. 当前阻断结论（可直接催办）

主阻断：

1. `public_broadcast_gate=false`（M5 生产放行硬阻断）

关键未达标（并行研发可继续，但不能替代 M5 放行）：

1. `m1_hr_e03_logs_filter_subscribe_gate_pass=false`
2. `m1_hr_e05_receipt_error_semantics_gate_pass=false`
3. `m1_current_profile=M1aCurrentFallback`（说明仍是降级观测态，不是 fullstack 达标）

---

## 5. 给上游同事的最小交付件（建议催办顺序）

1. **先交付公网广播可验证证据**  
   目标文件：`artifacts/migration/evm/eth-public-broadcast-upstream-summary.json`  
   作用：直接解锁 `public_broadcast` 相关检查链路。

2. **再交付双源观测能力**  
   至少满足：`newPendingTransactions + txpool_content` 可稳定提供。  
   作用：把当前 `M1aFallback` 推进到 fullstack 观测态。

3. **补齐语义面（HR-E03/HR-E05）**  
   logs/filter/subscribe + receipt/error 语义一致。  
   作用：降低误报、提升失败归因与自动重试正确性。

4. **最后收口 TxType/UCA**  
   明确 0/1/2/3/4 策略与 UCA 对齐口径。  
   作用：确保放量前不会在账户与交易类型边界出生产事故。

---

## 6. 验收命令（MEV 侧）

1. `pwsh -NoProfile -File scripts/migration/run_mev_m5_handover_bundle.ps1`
2. `pwsh -NoProfile -File scripts/migration/run_mev_m1a_evm_handover_precheck.ps1 -RefreshSmokeSample`
3. `pwsh -NoProfile -File scripts/migration/run_mev_parallel_rnd_regression_bundle.ps1`

关注字段：

1. `m5-handover-bundle-summary.json -> .status.public_broadcast_gate`
2. `m5-handover-bundle-summary.json -> .status.m5_pass`
3. `m5-handover-bundle-summary.json -> .status.m1_hr_e03_logs_filter_subscribe_gate_pass`
4. `m5-handover-bundle-summary.json -> .status.m1_hr_e05_receipt_error_semantics_gate_pass`
5. `m5-handover-bundle-summary.json -> .status.m1_current_profile`

---

## 7. 关联文档

1. `docs_CN/MEV/NOVOVM-MEV-EVM-PARALLEL-CONTRACT-v1-2026-03-09.md`
2. `docs_CN/MEV/NOVOVM-MEV-HARD-ROUTE-CAPABILITY-CONTRACT-v1-2026-03-10.md`
3. `docs_CN/MEV/NOVOVM-MEV-HARD-ROUTE-CAPABILITY-FIELD-QUICK-REFERENCE-v1-2026-03-10.md`
4. `docs_CN/MEV/NOVOVM-MEV-M5-UPSTREAM-HANDOVER-QUICKSTART-v1-2026-03-09.md`

