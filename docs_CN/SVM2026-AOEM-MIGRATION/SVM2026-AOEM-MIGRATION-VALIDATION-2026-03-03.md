# SVM2026 -> SUPERVM 验收执行记录 - 2026-03-03

## 本次新增

- 功能一致性脚本：`scripts/migration/run_functional_consistency.ps1`
- 性能对照脚本：`scripts/migration/run_performance_compare.ps1`
- 性能门禁脚本（封盘口径唯一入口）：`scripts/migration/run_performance_gate_seal_single.ps1`
- 一键迁移验收门禁脚本：`scripts/migration/run_migration_acceptance_gate.ps1`
- 一致性摘要示例：`crates/aoem-bindings/examples/ffi_consistency_digest.rs`

## 运行命令

```powershell
powershell -ExecutionPolicy Bypass -File D:\WorksArea\SUPERVM\scripts\migration\run_migration_acceptance_gate.ps1
```

## 结果产物

- `artifacts/migration/functional/functional-consistency.json`
- `artifacts/migration/functional/functional-consistency.md`
- `artifacts/migration/performance-gate/seal-single/performance-compare.json`
- `artifacts/migration/performance-gate/seal-single/performance-compare.md`
- `artifacts/migration/performance-gate/seal-single/performance-gate-summary.json`
- `artifacts/migration/performance-gate/seal-single/performance-gate-summary.md`
- `artifacts/migration/acceptance-gate/acceptance-gate-summary.json`
- `artifacts/migration/acceptance-gate/acceptance-gate-summary.md`

## 功能一致性结果（2026-03-03）

- `node_mode_consistency.pass = true`（`ffi_v2` 与 `legacy_compat` 同口径输出一致）
- `variant_digest_consistency.pass = true`（`core/persist/wasm` 摘要一致）
- digest：`23fa19a67e4f34ef2cc98a448d1326b7a9b5496e2dd73424b81a5a403cc23b80`

说明：当前 AOEM FFI V2 骨架未暴露 `state_root` 字段，功能一致性先用可重复执行摘要作为代理校验。

## 性能当前值（封盘口径，3-run P50，2026-03-03）

| variant | preset | tps(ops/s) |
|---|---|---:|
| core | cpu_parity | 4755810.53 |
| core | cpu_batch_stress | 22945726.47 |

说明：已固定 `release + seal_single + AOEM 封盘基线(2026-03-02)`，并执行回归阈值判定（`AllowedRegressionPct=-5`）。

## 本轮顺带修复

- 修复 `ffi_perf_worldline` debug 构建下 seed 混合溢出 panic（改为 `wrapping_mul`）。
- 修复 `novovm-node` 进程退出时 AOEM DLL 卸载竞态导致的偶发 `STATUS_ACCESS_VIOLATION`（保持 DLL 常驻至进程退出）。

## 下一步闭环

1. 保持性能门禁唯一入口：`run_performance_gate_seal_single.ps1`（禁止用非封盘口径覆盖门禁结论）。
2. 在 AOEM FFI V2 返回结构中补齐 `state_root`，将代理摘要校验升级为“状态根一致”硬校验。
3. 基于上述结果持续更新 Checklist D 四项最终状态。
