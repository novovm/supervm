# NOVOVM 第三方漏洞审计交付包（2026-03-13）

## 1. 目标与验收口径

- 目标：为 GA 前的外部安全审计提供统一入口、固定范围与可复现证据。
- 本轮验收口径：
  - `Critical=0`
  - `High=0`
  - `Medium/Low` 必须有明确处置计划（版本与 ETA）。
- 未满足上述口径时，GA 必须阻断。

## 2. 本轮审计范围（In Scope）

- 共识与状态一致性主链路：
  - `crates/novovm-consensus`
  - `crates/novovm-protocol`
  - `crates/novovm-coordinator`
- 运行时执行与网络入口：
  - `crates/novovm-exec`
  - `crates/novovm-network`
  - `crates/novovm-node`
- 经济开放业务面与运营控制面门禁对应实现：
  - `scripts/migration/run_economic_service_surface_gate.ps1`
  - `scripts/migration/run_ops_control_surface_gate.ps1`
  - `scripts/migration/run_funds_path_safety_gate.ps1`
  - `scripts/migration/run_runtime_security_baseline_gate.ps1`

## 3. 本轮排除范围（Out of Scope）

- 外部设备独立开发的 EVM 台账与节点实现（不在本仓验收计数）。
- 商业计划、市场策略与非代码运营文案。
- 与当前 GA 发布无关的历史归档文档。

## 4. 审计输入证据（固定入口）

- GA 快照（含经济/运营 gate）：
  - `artifacts/migration/week1-2026-03-13/release-snapshot-ga-v1-2026-03-13-1425/release-snapshot.json`
- RC 候选（含经济/运营 gate 字段）：
  - `artifacts/migration/week1-2026-03-13/release-candidate-novovm-rc-2026-03-13-ga-v1-econops-1334/rc-candidate.json`
- acceptance 汇总：
  - `artifacts/migration/week1-2026-03-13/release-candidate-novovm-rc-2026-03-13-ga-v1-econops-1334/snapshot/acceptance-gate-full/acceptance-gate-summary.json`
- 依赖与供应链扫描：
  - `artifacts/migration/week1-2026-03-13/security-scan/cargo-audit.json`
  - `artifacts/migration/week1-2026-03-13/security-scan/cargo-deny-advisories.json`
  - `artifacts/migration/week1-2026-03-13/security-scan/cargo-deny-policy.json`
- 稳定窗口进行中证据（持续补充）：
  - `artifacts/migration/week1-2026-03-13/stability-window-72h-r2/`
  - `artifacts/migration/week1-2026-03-13/stability-window-72h-r2.nohup.log`

## 5. 审计方最小复现实操

1. 复核 RC 事实入口：读取 `rc-candidate.json` 与 `release-snapshot.json`，确认 `snapshot_overall_pass=true`。
2. 复核 gate 事实：读取 `acceptance-gate-summary.json`，确认经济服务面与运营控制面 gate 为启用且通过。
3. 复核供应链基线：读取 `cargo-audit/cargo-deny` 报告并给出处置建议。
4. 对 in-scope 代码实施漏洞审计（静态 + 动态 + 关键路径人工复核）。

## 6. 交付要求（审计报告）

- 必须交付：
  - 漏洞清单（ID、级别、影响范围、利用前提、复现步骤）
  - 修复建议与优先级
  - 总结结论（是否达到 `Critical/High=0`）
- 建议交付：
  - 风险趋势与后续复审建议
  - 对资金路径/共识路径的专项评估附录

## 7. 整改与关单流程

1. 按严重级别创建修复任务并绑定代码提交。
2. 修复后复跑 `release-snapshot` + 相关专项 gate。
3. 将复跑证据路径写回收口清单。
4. 满足 `Critical/High=0` 后，才可推进 GA 放行。

## 8. 责任与接口

- 发布口径负责：`Release Coordinator`
- 安全响应负责：见 `NOVOVM-VULNERABILITY-RESPONSE-POLICY-2026-03-13.md`
- 审计输出归档：`docs_CN/SVM2026-MIGRATION/` + `artifacts/migration/**`
