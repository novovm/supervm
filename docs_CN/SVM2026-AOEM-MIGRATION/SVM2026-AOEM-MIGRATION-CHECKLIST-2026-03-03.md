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
- [ ] 增加统一错误码映射（下一步）

## C. 迁移实施

- [x] 提供 `novovm-node` 主路径替换模板（文档+示例）
- [x] `novovm-node` 首条主路径改为调用门面（`crates/novovm-node/src/main.rs`）
- [ ] `novovm-node` 其余路径逐步替换
- [ ] 将 `SVM2026` 已验证能力逐项迁入 `SUPERVM` 对应模块
- [ ] 去除 AOEM 源码 path 依赖
- [ ] 统一 core/persist/wasm 配置入口

## D. 验收

- [ ] 功能一致性（前后状态根一致）
- [ ] 性能对照（迁移前后同口径）
- [ ] 崩溃恢复与持久化一致性
- [ ] 文档封盘（迁移版本、回退步骤）
