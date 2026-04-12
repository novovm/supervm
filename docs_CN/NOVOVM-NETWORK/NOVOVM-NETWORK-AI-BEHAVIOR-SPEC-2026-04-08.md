# NOVOVM-NETWORK-AI-BEHAVIOR-SPEC-2026-04-08

## 1. 文档定位

本文定义 NOVOVM Network 主线开发中 AI/Codex 的执行行为硬规则。  
本规范用于约束“实现方式”和“验证方式”，避免阶段收口验证与生产发布口径分叉。

## 2. 生产验证语言规则（硬约束）

生产验证场景必须使用 Rust，禁止使用 `ps1` 脚本替代主线验证。

适用范围：

1. 阶段收口 smoke（Phase closure smoke）
2. 发布前主线行为验证（release-gate behavior validation）
3. CI 中的主路径行为用例
4. 作为“主线能力成立证据”的验证流程

强制规则：

1. 必须使用 Rust 测试或 Rust 可执行程序：
   - `crates/*/tests/*.rs`
   - `crates/*/src/bin/*.rs`
2. 禁止将 PowerShell 脚本作为生产级行为验证入口：
   - 禁止以 `*.ps1` 作为主验证实现
   - 禁止以 `*.ps1` 产物作为主线通过证据

## 3. PowerShell 的允许边界

`ps1` 仅可用于运维辅助，不可替代 Rust 主线验证。

允许：

1. 本地开发环境准备（目录清理、日志汇总、临时编排）
2. 非阻断型辅助脚本

禁止：

1. 生产行为验证主流程
2. 阶段收口唯一证据
3. 代替 Rust 集成测试的验证路径

## 4. Phase B 当前绑定规则

`QueueOnly 入队 -> 重启后 replay` 的节点级集成 smoke，必须采用 Rust 集成测试落点：

1. `crates/novovm-node/tests/queue_replay_smoke.rs`

不得采用：

1. `scripts/*.ps1` 作为该主线能力的最终验证实现

## 5. 变更控制

若需调整本规范，必须：

1. 先更新 RFC 或阶段实施文档
2. 明确给出偏离原因与回滚路径
3. 通过文档评审后再变更执行

## 6. 一句话结论

> NOVOVM Network 的生产级行为验证必须走 Rust 主线测试体系；PowerShell 不能作为主线能力成立的证明路径。

