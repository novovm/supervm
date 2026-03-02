# SVM2026 -> SUPERVM 迁移计划 - 2026-03-03

## Phase 0（已完成）

- 清理本地历史数据与构建副产物，保留最小寄宿集合：
  - `aoem/bin/aoem_ffi.dll`
  - `aoem/include/aoem.h`
  - `aoem/manifest/aoem-manifest.json`
  - `aoem/INSTALL-INFO.txt`

## Phase 1（已完成）

- 建立门面 crate：`crates/novovm-exec`
- 输出最小 API：
  - `open`
  - `create_session`
  - `execute_ops_v2`

## Phase 2（进行中）

- 在 `SUPERVM` 中承接 `SVM2026` 已验证能力，并先迁移 `novovm-node` 主执行路径：
  - [x] 在 `novovm-exec` 增加主路径稳定入口 `submit_ops`（结果+指标）
  - [x] 提供可运行模板 `examples/main_path_template.rs`
  - [x] 先替换一条核心路径（`crates/novovm-node/src/main.rs`，`submit_ops` 主路径）
  - [ ] 对齐错误处理、指标、返回码

## Phase 3

- 将 `SVM2026` 的剩余已验证能力继续迁入 `SUPERVM`
- 删除 `SUPERVM` 中对 AOEM 源码 path 依赖
- 引入统一配置装载（core/persist/wasm 变体）

## Phase 4

- 验收与封盘：
  - 功能 smoke
  - TPS 口径矩阵
  - 稳定性与回归测试
