# NOVOVM Account Protocol 结构变更 PR 守门规则（2026-04-21）

Status: AUTHORITATIVE GOVERNANCE RULE  
Scope: 真实 `novovm-node` 主线路径上的统一账户结构性改动

## 核心规则

任何会改变统一账户结构的 PR，必须显式映射到至少一条已批准的 Trigger Checklist 条目；否则默认结论为 `Reject (No-Go)`。

固定治理方向：

`推进需要理由；不推进不需要解释`

## 什么算结构性改动

当 PR 改动以下任一项时，视为结构性改动：

- 规范主体语义（`account_id`、兼容别名规则）
- 协议方法合同或路由方法面
- 账户资产事实来源（资产视图来源、新账本来源、root 来源）
- `KeyAlgo`、`ExecutionPolicy` 或未来 `AccountMode` 的执行约束语义
- `Cut B` 或 `Phase 4` 的阶段边界

默认不算结构性改动的情况：

- 不改变协议语义的缺陷修复
- 仅测试改动
- 仅文档口径改动
- 不改变行为的重构

## 结构性改动 PR 的必填项

结构性 PR 必须同时给出以下四项：

1. Trigger Checklist 引用：
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
   - 或 `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
2. 触发条目 ID 与证据链接（测试、指标、事故、业务需求）
3. “最小不可回滚实现切片”定义与回滚边界
4. 明确决策请求（`Go` 或 `No-Go`）
5. 仅当涉及 `Phase 4` 结构改动时：必须补全
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`

任一项缺失，评审默认结论即为 `Reject (No-Go)`。

## 元治理锁（防治理漂移）

凡是修改以下治理控制文档的 PR，必须附带更高层级治理证据链：

- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`

证据链最少包含：

1. Governance proposal 引用
2. Governance vote 证据
3. Governance execute/activation 证据

任一项缺失，评审默认结论即为 `Reject (No-Go)`。

## 评审决策策略

- trigger 不满足：`Reject (No-Go)`
- trigger 满足但切片边界不清晰：`Reject (No-Go)`
- trigger 满足 + 切片收敛 + 证据完整：进入 `Go` 评审资格（不等于自动批准）

## 非结构性 PR 声明

若 PR 不涉及统一账户结构改动，作者应显式声明：

`No unified-account structural change in this PR. Trigger checklist not required.`

## 权威引用

- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-v1-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
- `docs_CN/CURRENT-AUTHORITATIVE-ENTRYPOINT-2026-04-17.md`
