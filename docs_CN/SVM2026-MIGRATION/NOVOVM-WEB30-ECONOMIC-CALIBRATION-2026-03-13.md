# NOVOVM WEB30/经济进度统一口径说明（2026-03-13）

## 1. 目的

解决现有文档中“WEB30 仍未全量迁移”与“经济开放业务面已收口”并存导致的理解冲突，统一对外表述与发布判断口径。

## 2. 统一口径（三层状态）

### L1：共识主链路（Core Release Path）

- 含义：`novovm-consensus + novovm-node + 基础 gate` 的可发布主链路。
- 状态：已形成可发布闭环（以 acceptance/release snapshot 证据为准）。

### L2：经济开放业务面（Economic Open Surface）

- 含义：经济能力从“受限主链路”转为“可运营业务面”，要求有服务面、运营控制面、资金安全与运行时安全门禁。
- 状态：已完成 Week3 收口（2026-03-13）。
- 判定证据：
  - `run_economic_infra_dedicated_gate.ps1`
  - `run_funds_path_safety_gate.ps1`
  - `run_runtime_security_baseline_gate.ps1`
  - `run_ops_control_surface_gate.ps1`
  - `run_economic_service_surface_gate.ps1`

### L3：WEB30 标准族全量迁移（F-10~F-13 Full Migration）

- 含义：`WEB30-PROTOCOL/SVM2026-REFERENCE/standards` 相关标准在 NOVOVM 主链路实现与门禁的全面对齐（不仅是经济治理最小闭环）。
- 状态：仍在迁移中（未声明全量完成）。

## 3. 关键澄清（防止误读）

1. “WEB30 标准族未全量迁移”不等于“经济开放面不可运营”。
2. 当前已完成的是 L2（经济开放业务面收口），不是 L3（F-10~F-13 全量实现完成）。
3. 发布判断以“代码主链路 + 最新 gate 证据”为准，参考文档快照不作为发布通过证据。

## 4. 对外统一表述（建议模板）

- 建议对外文本：
  - “NOVOVM 当前已完成共识主链路与经济开放业务面收口；WEB30 标准族（F-10~F-13）仍按计划持续迁移，尚未宣称全量完成。”

## 5. 关联文档（已对齐）

- `docs_CN/WEB30-PROTOCOL/README.md`
- `docs_CN/WEB30-PROTOCOL/WEB30-PROTOCOL-MIGRATION-INDEX-2026-03-05.md`
- `docs_CN/SVM2026-MIGRATION/NOVOVM-OPEN-BUSINESS-SURFACE-CLOSURE-CHECKLIST-2026-03-13.md`
