# NOVOVM Account Protocol Phase 4 最小验证切片模板（2026-04-21）

Status: AUTHORITATIVE EXECUTION TEMPLATE（Trigger-Activated）  
Scope: 仅在 `Phase 4` 触发评审为 `Go` 后可使用的内部最小验证切片模板

## 目的

本模板用于规范 `Phase 4` 在触发通过后的最小实现切片做法。

它不是功能路线图，也不是对外上线计划。

默认结论不变：

`Phase 4 在触发通过前始终 No-Go。`

## 启用前提

仅当以下条件全部成立时，才允许使用本模板：

1. `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md` 已评审为 `Go`
2. 结构性改动 PR 已满足：
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`
3. 实施范围仅限内部验证（不对外发布）

任一项不满足时，结论为：

`不得启动 Phase 4 实现切片。`

## 强约束边界（必须持续成立）

- 禁止新增 mapped-asset 独立账本
- 禁止引入 `asset_root` 或新全局 root 来源
- 禁止引入通用多链框架
- 禁止引入隐私资产账本空间
- 禁止把 `account_balance / account_assets` 改成写路径
- 规范主体必须保持为 `account_id`

## MVP 切片声明块（编码前必填）

将以下内容原样放入 PR 描述：

```text
Trigger checklist:
Trigger item IDs:
Decision request: Go

Slice scope:
- source chain:
- source asset:
- target asset representation:
- ownership subject: account_id

Storage statement:
- no new ledger:
- no new root:
- no new global asset truth source:

Flow:
- lock -> register -> visible-in-view -> burn -> release

Rollback boundary:
- code boundary:
- state boundary:
- abort condition:
```

## 必须证明的不变量

实现必须同时证明以下事项：

1. 切片内供给守恒成立
2. 每个映射单元可追溯到 `lock_id -> source_tx_hash`
3. 未引入新全局资产真相源
4. register/burn/release 的审计链连续
5. release 必须依赖 burn 前置状态

## 最小测试集

1. 有效 lock proof -> register 成功 -> `account_assets` 可见
2. 重复 lock proof 拒绝
3. 无效 proof 拒绝
4. burn 触发 `active -> burn_pending`
5. 未 burn 直接 release 拒绝
6. burn + release 后状态为 `released`，且不再作为 active 展示
7. 审计追踪可还原 `account_id -> mapping_id -> lock_id -> source_tx_hash`

## 完成标准

仅当以下全部满足时，切片才算完成：

- 不变量全部通过
- 最小测试集全部通过
- `cargo clippy` 与现有 unified-account gate 仍为绿色
- 未触碰任何强约束边界

## 决策输出

- `Continue in internal validation`：切片通过且边界未破坏
- `No-Go rollback`：任一不变量或边界失败
- `Candidate for controlled expansion`：仅在新一轮触发评审后可讨论

## 关联文档

- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`
