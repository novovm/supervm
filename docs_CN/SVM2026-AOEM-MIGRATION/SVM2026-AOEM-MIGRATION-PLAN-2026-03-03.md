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

## Phase 2（已完成）

- 在 `SUPERVM` 中承接 `SVM2026` 已验证能力，并先迁移 `novovm-node` 主执行路径：
  - [x] 在 `novovm-exec` 增加主路径稳定入口 `submit_ops`（结果+指标）
  - [x] 提供可运行模板 `examples/main_path_template.rs`
  - [x] 先替换一条核心路径（`crates/novovm-node/src/bin/novovm-node.rs`，`submit_ops` 主路径）
  - [x] 对齐错误处理、指标、返回码（`submit_ops_report` 统一输出）

## Phase 3

- [ ] 将 `SVM2026` 的剩余已验证能力继续迁入 `SUPERVM`（最后做）
- [x] 删除 `SUPERVM` 中对 AOEM 源码 path 依赖
- [x] 引入统一配置装载（core/persist/wasm 变体）

## Phase 4

- 验收与封盘：
  - [x] 功能一致性代理脚本（`scripts/migration/run_functional_consistency.ps1`）
  - [x] TPS 口径采集脚本（`scripts/migration/run_performance_compare.ps1`）
  - [x] 导入 `AOEM seal baseline` 并完成性能回归判定（`2026-03-13 11:55 HDT`，Linux seal baseline，`pass=true`）
  - [x] 接入 `state_root` 并完成硬一致性校验（`2026-03-13 11:59 HDT`，`epoch_commit/proposal_emit` 双阶段校验 + 负向单测）
  - [ ] 稳定性与回归测试封盘（`2026-03-13 12:15 HDT` 已新增 `run_stability_window_gate.ps1`、完成 smoke 并启动 `72h` 实窗，待窗口完成后封盘）

### Phase 4 证据路径（2026-03-13）

- `artifacts/migration/week1-2026-03-13/perf-gate-seal-single/performance-gate-summary.json`
- `cargo test -p novovm-consensus state_root_match_helper`
- `cargo test -p novovm-consensus test_propose_epoch_with_state_root_override`
- `scripts/migration/run_stability_window_gate.ps1`
- `artifacts/migration/week1-2026-03-13/stability-window-smoke/stability-window-summary.json`

