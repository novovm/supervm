# NOVOVM EVM Adapter Strict-v2 合并清单（2026-03-07）

## 1. 目标

- 以严格口径（`AllowedRegressionPct=-5`）确认 EVM 适配迁移链路可合并。
- 确认四链 compare（`evm/polygon/bnb/avalanche`）已进入 RC 证据闭环。

## 2. 代码接线清单

- [ ] EVM plugin chain code 映射已包含 `polygon=5`、`avalanche=7`  
  - `crates/novovm-adapter-evm-plugin/src/lib.rs`
- [ ] Node host chain code 映射已包含 `polygon=5`、`avalanche=7`  
  - `crates/novovm-node/src/bin/novovm-node.rs`
- [ ] Registry / matrix 已包含四链 allowlist  
  - `config/novovm-adapter-plugin-registry.json`
  - `config/novovm-adapter-compatibility-matrix.json`
- [ ] compare 脚本支持四链且复跑会重建状态目录  
  - `scripts/migration/run_evm_backend_compare_signal.ps1`

## 3. 发布汇总清单

- [ ] acceptance 汇总包含四链 compare 字段（enabled/include/pass/report）  
  - `scripts/migration/run_migration_acceptance_gate.ps1`
- [ ] snapshot 汇总包含 `evm_backend_compare_{evm,polygon,bnb,avalanche}_pass`  
  - `scripts/migration/run_release_snapshot.ps1`
- [ ] rc 汇总包含四链 compare pass/report 字段  
  - `scripts/migration/run_release_candidate.ps1`

## 4. 性能口径清单（严格）

- [ ] `seal_single` 口径固定：`threads=1`, `engine-workers=4`  
  - `scripts/migration/run_performance_compare.ps1`
- [ ] preset 冷却默认 `2s`  
  - `scripts/migration/run_performance_gate_seal_single.ps1`
- [ ] strict gate（`-5`, `Runs=3`）通过  
  - `artifacts/migration/perf-gate-strict-after-ew4-cooldown-next-step/performance-gate-summary.json`

## 5. 证据清单（必须存在）

- [ ] acceptance：  
  - `artifacts/migration/release-candidate-next-step-4chain-strict-v2/snapshot/acceptance-gate-full/acceptance-gate-summary.json`
- [ ] snapshot：  
  - `artifacts/migration/release-candidate-next-step-4chain-strict-v2/snapshot/release-snapshot.json`
- [ ] rc：  
  - `artifacts/migration/release-candidate-next-step-4chain-strict-v2/rc-candidate.json`
- [ ] 四链 compare：  
  - `.../evm-backend-compare-gate/evm/backend_compare_signal.json`
  - `.../evm-backend-compare-gate/polygon/backend_compare_signal.json`
  - `.../evm-backend-compare-gate/bnb/backend_compare_signal.json`
  - `.../evm-backend-compare-gate/avalanche/backend_compare_signal.json`

## 6. 验收命令（快速复核）

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_performance_gate_seal_single.ps1 `
  -RepoRoot . `
  -OutputDir artifacts/migration/perf-gate-strict-verify `
  -AllowedRegressionPct -5 `
  -Runs 3

powershell -ExecutionPolicy Bypass -File scripts/migration/run_release_candidate.ps1 `
  -RepoRoot . `
  -RcRef rc-evm-next-step-4chain-strict-v2-verify `
  -OutputDir artifacts/migration/release-candidate-next-step-4chain-strict-v2-verify `
  -FullSnapshotProfileV2
```

## 7. 当前判定（本轮）

- `status`: `ReadyForMerge/SnapshotGreen`
- `snapshot_profile`: `full_snapshot_v2`
- `evm_backend_compare_pass`: `true`
- `evm_backend_compare_polygon_pass`: `true`
- `evm_backend_compare_bnb_pass`: `true`
- `evm_backend_compare_avalanche_pass`: `true`
- 严格性能门禁：`pass=true`

