# NOVOVM 第三方漏洞审计委托草稿（2026-03-13 13:52 HDT）

## 邮件标题

`[Security Audit Request] NOVOVM GA Pre-Launch Audit (Critical/High=0 Required)`

## 邮件正文（可直接发送）

您好，

我们正在进行 NOVOVM GA 前的安全收口，现委托贵方执行第三方漏洞审计。

审计目标（Gate）：
1. 识别并验证可利用安全漏洞。
2. 出具漏洞分级与修复建议。
3. 交付结论需满足：`Critical=0`、`High=0`（否则 GA 阻断）。

审计范围（In Scope）：
1. `crates/novovm-consensus`
2. `crates/novovm-protocol`
3. `crates/novovm-coordinator`
4. `crates/novovm-exec`
5. `crates/novovm-network`
6. `crates/novovm-node`
7. `scripts/migration/run_economic_service_surface_gate.ps1`
8. `scripts/migration/run_ops_control_surface_gate.ps1`
9. `scripts/migration/run_funds_path_safety_gate.ps1`
10. `scripts/migration/run_runtime_security_baseline_gate.ps1`

排除范围（Out of Scope）：
1. 外部设备独立开发的 EVM 台账与节点实现。
2. 非代码类商业文档与运营文案。

交付包路径：
- `artifacts/migration/week1-2026-03-13/third-party-audit-handoff-pack-2026-03-13-1342.tar.gz`
- SHA256: `634d01b3678387b96f8cb22dec75fe1ef4fba6db855962fc848a2f2cc949bdc2`

期望回传材料：
1. 正式审计报告（PDF/Markdown）。
2. 漏洞明细（级别/影响/复现/修复建议）。
3. 最终结论页（是否满足 `Critical=0`、`High=0`）。

请回传以下受理信息：
1. 受理单号。
2. 预计回传时间（含时区）。
3. 主要对接人。

谢谢。
