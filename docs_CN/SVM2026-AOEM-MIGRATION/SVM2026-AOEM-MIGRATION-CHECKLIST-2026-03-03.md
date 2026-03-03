# SVM2026 -> SUPERVM 功能迁移清单 - 2026-03-03

## A. 工程纯净性

- [x] `aoem/data` 已清理
- [x] `aoem/config` 与变体 `config` 已清理
- [x] `.pdb/.lib/.exp/.d` 副产物已清理
- [x] 保留最小寄宿集合（dll/h/manifest/install-info）

## B. 门面层

- [x] 新建 `crates/novovm-exec`
- [x] 具备 `open/create_session/execute_ops_v2`
- [x] 增加主路径提交入口 `submit_ops`（统一返回结果+指标）
- [x] 增加统一错误码映射（`submit_ops_report` 输出 `rc/code_name/error`）

## C. 迁移实施

- [x] 提供 `novovm-node` 主路径替换模板（文档+示例）
- [x] `novovm-node` 首条主路径改为调用门面（`crates/novovm-node/src/main.rs`）
- [x] `novovm-node` 其余路径逐步替换（`legacy` 入口兼容转发到 `ffi_v2`）
- [ ] 将 `SVM2026` 已验证能力逐项迁入 `SUPERVM` 对应模块（最后做）
- [x] 去除 AOEM 源码 path 依赖（运行时代码不再依赖 `../aoem/crates`）
- [x] 统一 core/persist/wasm 配置入口（`AoemRuntimeConfig` + `aoem/config/aoem-runtime-profile.json`）

## D. 验收

- [ ] 功能一致性（前后状态根一致；已落地代理脚本 `scripts/migration/run_functional_consistency.ps1`，待接入 state_root 字段）
- [x] 性能对照（迁移前后同口径；已冻结唯一门禁 `scripts/migration/run_performance_gate_seal_single.ps1`，固定 `release + seal_single + AOEM 封盘基线`，按 3-run P50 判定）
- [ ] 崩溃恢复与持久化一致性
- [ ] 文档封盘（迁移版本、回退步骤）
