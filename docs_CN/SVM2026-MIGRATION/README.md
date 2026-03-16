# NOVOVM（SuperVM）迁移文档索引（2026-03-03）

本文档集用于替代 `SVM2026/ROADMAP.md` 中旧的混合叙事，面向 `NOVOVM` 生产版落地。

## 命名与边界铁律（防误导）

- `SuperVM 主网`：唯一主链与共识主体，长期口径不变。
- `EVM`：仅作为 `SuperVM` 的插件增强能力（兼容接口/钱包接入/多链桥接），不是独立主链。
- `novovm-node`：`SuperVM` 主网节点二进制，不定义为 “EVM 节点客户端”。
- `novovm-evm-gateway`：EVM 插件网关（兼容入口），不是独立 EVM 链执行客户端。
- 文档/代码中出现 `eth_*` 仅表示兼容命名空间，不代表系统架构切换为 “EVM 链模式”。

## 边界自检接口（运行态一眼确认）

- `novovm_getSurfaceMap`：返回 `novovm_mainnet` / `evm_plugin` 两个域的总览。
- `novovm_getMethodDomain`：输入任意方法名，返回该方法归属域（`novovm_mainnet` / `evm_plugin` / `unknown`）。

最小调用示例（JSON-RPC）：

```json
{"jsonrpc":"2.0","id":1,"method":"novovm_getSurfaceMap","params":{}}
```

```json
{"jsonrpc":"2.0","id":2,"method":"novovm_getMethodDomain","params":{"method":"eth_getBalance"}}
```

## 文档清单

1. `NOVOVM-SVM2026-AUDIT-2026-03-03.md`
   - 对 `SVM2026` 代码与 `ROADMAP.md` 做结构化审计，输出可迁移性结论。
2. `NOVOVM-PANORAMA-ARCH-2026-03-03.md`
   - 给出 NOVOVM 新全景图与分层边界（替代旧五层图）。
3. `NOVOVM-FUNCTION-CATALOG-2026-03-03.md`
   - 给出功能分类、模块归属、迁移方式（复用/重构/暂缓）。
4. `NOVOVM-MIGRATION-MASTER-PLAN-2026-03-03.md`
   - 给出阶段计划、时间窗、验收门槛与风险控制。
5. `SVM2026-LAYER-STATUS-VERIFIED-2026-03-03.md`
   - 对 SVM2026 共识层/网络层做构建与测试核验，形成迁移基线状态。
6. `NOVOVM-CAPABILITY-MIGRATION-LEDGER-TEMPLATE-2026-03-03.md`
   - 能力迁移执行台账模板（按能力项推进、验证、回退）。
7. `NOVOVM-CAPABILITY-MIGRATION-LEDGER-2026-03-03.md`
   - 能力迁移执行台账（实际推进状态）。
8. `NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md`
   - 自动回填台账快照（当前主口径，full-snapshot 产物回填）。
9. `NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-03.md`
   - 历史自动快照（保留归档，不作为当前进度主口径）。
9. `NOVOVM-RELEASE-RC-RUNBOOK-2026-03-05.md`
   - 发布候选（RC）流程手册，固定 `full_snapshot_v1` 复现口径，并新增治理 RPC 安全发布铁律与 `full_snapshot_v2`（含 RPC 暴露门禁）入口。

## 2026-03-13 收口新增入口

1. `NOVOVM-OPEN-BUSINESS-SURFACE-CLOSURE-CHECKLIST-2026-03-13.md`
   - 开放业务面收口总清单（按周 gate + 每日证据回填）。
2. `NOVOVM-ECONOMIC-OPS-RUNBOOK-2026-03-13.md`
   - 经济开放面值班/回滚/对账/巡检最小运维手册。
3. `NOVOVM-WEB30-ECONOMIC-CALIBRATION-2026-03-13.md`
   - WEB30 与经济开放面的统一口径说明。
4. `NOVOVM-VULNERABILITY-RESPONSE-POLICY-2026-03-13.md`
   - 漏洞披露窗口、SLA、回滚预案、联系人。
5. `NOVOVM-THIRD-PARTY-AUDIT-HANDOFF-PACK-2026-03-13.md`
   - 第三方审计交付包范围与验收口径。
6. `NOVOVM-THIRD-PARTY-AUDIT-INTAKE-REGISTER-2026-03-13.md`
   - 第三方审计受理登记、回包导入与关单入口。
7. `NOVOVM-GA-CLOSURE-REPORT-DRAFT-2026-03-13.md`
   - GA 收口正式版前的阻断态草案。

## Linux 封盘证据入口（2026-03-10）

1. `../AOEM-FFI/AOEM-FFI-BETA08-TPS-SEAL-Linux-2026-03-10.md`
   - AOEM FFI Linux 十二线封盘（core/persist/wasm）。
2. `../CONSENSUS/NOVOVM-CONSENSUS-NETWORK-E2E-TPS-SEAL-Linux-2026-03-10.md`
   - 共识网络 E2E TPS Linux 封盘（persist + ops_wire_v1 + inmemory）。

## 建议阅读顺序

1. 审计报告（先确认问题边界）
2. 新全景架构（确认目标形态）
3. 功能分类（确认迁移颗粒度）
4. 总计划（确认执行节奏）

## 本轮冻结结论

- `AOEM` 已是 NOVOVM 的底座，不再把执行内核散落在业务模块中。
- 旧 `SVM2026` 的“核心/共识/应用混合”结构不再沿用。
- “将 `SVM2026` 已验证能力逐项迁入 `SUPERVM` 对应模块”放在最后阶段执行。
