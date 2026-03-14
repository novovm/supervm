# NOVOVM 第三方漏洞审计委托模板（2026-03-13）

## 1. 邮件标题模板

`[Security Audit Request] NOVOVM GA Pre-Launch Audit (Critical/High=0 Required)`

## 2. 邮件正文模板

```text
您好，

我们正在进行 NOVOVM GA 前的安全收口，现委托贵方执行第三方漏洞审计。

审计目标（Gate）：
1) 识别并验证可利用安全漏洞；
2) 出具分级结论；
3) 交付报告时需满足：Critical=0、High=0（否则 GA 阻断）。

审计范围（In Scope）：
- crates/novovm-consensus
- crates/novovm-protocol
- crates/novovm-coordinator
- crates/novovm-exec
- crates/novovm-network
- crates/novovm-node
- scripts/migration/run_economic_service_surface_gate.ps1
- scripts/migration/run_ops_control_surface_gate.ps1
- scripts/migration/run_funds_path_safety_gate.ps1
- scripts/migration/run_runtime_security_baseline_gate.ps1

排除范围（Out of Scope）：
- 外部设备独立开发的 EVM 台账与节点实现
- 非代码类商业文档/运营文案

交付包路径：
- artifacts/migration/week1-2026-03-13/third-party-audit-handoff-pack-2026-03-13-1342.tar.gz
- SHA256: 634d01b3678387b96f8cb22dec75fe1ef4fba6db855962fc848a2f2cc949bdc2

期望交付：
1) 正式审计报告（PDF/Markdown）
2) 漏洞明细（级别/影响/复现/修复建议）
3) 总结页（是否满足 Critical=0、High=0）

请回传：
- 受理单号
- 预计回传时间（含时区）
- 主要对接人

谢谢。
```

## 3. IM/工单简版模板

```text
请求第三方漏洞审计（NOVOVM GA 前）：交付包已就绪（sha256 已附），验收门槛为 Critical=0、High=0。请回传受理单号和预计回传时间。
```

## 4. 附件与链接清单

- 审计交付包说明：
  - `docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-HANDOFF-PACK-2026-03-13.md`
- 审计受理登记单：
  - `docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-INTAKE-REGISTER-2026-03-13.md`
- 漏洞响应机制：
  - `docs_CN/SVM2026-MIGRATION/NOVOVM-VULNERABILITY-RESPONSE-POLICY-2026-03-13.md`
