# AOEM FFI beta0.8 TPS 封盘（macOS Core+Persist+Wasm 十二线，2026-03-06）

## 本次封盘目标

- 基于当前 macOS 环境，按 Windows 封盘口径补齐 `core + persist + wasm` 十二线数据。
- 仅使用 FFI V2 typed 二进制路径（`aoem_execute_ops_v2`）。
- 固定参数、统一命名、统一输出格式。

## 固定口径（不可混写）

- 示例程序：`crates/aoem-bindings/examples/ffi_perf_worldline.rs`
- Core dylib：`SUPERVM/aoem/bin/libaoem_ffi.dylib`
- Persist dylib：`SUPERVM/aoem/variants/persist/bin/libaoem_ffi.dylib`
- Wasm dylib：`SUPERVM/aoem/variants/wasm/bin/libaoem_ffi.dylib`
- 固定参数：
  - `txs=1,000,000`
  - `key_space=128`
  - `rw=0.5`
  - `seed=123`
  - `warmup_calls=5`

## 测试环境

- OS：macOS 26.3（Build 25D125）
- Kernel：Darwin 25.3.0（arm64）
- CPU：Apple M4 Pro
- Rust：`rustc 1.93.0`，`cargo 1.93.0`
- 报告生成时间（UTC）：`2026-03-05T16:54:34Z`

## 四线命名

- `cpu_parity_single`：`preset=cpu_parity`, `submit_ops=1`, `threads=1`, `engine_workers=16`
- `cpu_parity_auto_parallel`：`preset=cpu_parity`, `submit_ops=1`, `threads=auto`, `engine_workers=auto`
- `cpu_batch_stress_single`：`preset=cpu_batch_stress`, `submit_ops=1024`, `threads=1`, `engine_workers=16`
- `cpu_batch_stress_auto_parallel`：`preset=cpu_batch_stress`, `submit_ops=1024`, `threads=auto`, `engine_workers=auto`

## 单位定义

- `ops/s`：事务吞吐（主 KPI）
- `plans/s`：提交吞吐（每次提交计 1 plan）
- `calls/s`：FFI 调用吞吐

## 原始数据

- `artifacts/migration/agent-baseline-2026-03-06-rerun-seal-single/performance-compare.json`
- `artifacts/migration/agent-baseline-2026-03-06-rerun-seal-auto/performance-compare.json`
- `artifacts/migration/agent-baseline-2026-03-06-rerun-persist-seal-single/performance-compare.json`
- `artifacts/migration/agent-baseline-2026-03-06-rerun-persist-seal-auto/performance-compare.json`
- `artifacts/migration/agent-baseline-2026-03-06-rerun-wasm-seal-single/performance-compare.json`
- `artifacts/migration/agent-baseline-2026-03-06-rerun-wasm-seal-auto/performance-compare.json`

## Core 实测矩阵（n=1）

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | ops/s | plans/s | calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 6,531,783.51 | 6,531,783.51 | 6,531,783.51 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 10,258,514.57 | 10,258,514.57 | 10,258,514.57 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 26,751,200.46 | 26,135.92 | 26,135.92 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 28,392,152.41 | 27,937.88 | 27,937.88 | 1,016.26 |

## Persist 实测矩阵（n=1）

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | ops/s | plans/s | calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 7,097,811.39 | 7,097,811.39 | 7,097,811.39 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 9,614,140.51 | 9,614,140.51 | 9,614,140.51 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 25,524,854.83 | 24,937.78 | 24,937.78 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 32,193,936.27 | 31,485.67 | 31,485.67 | 1,022.49 |

## Wasm 实测矩阵（n=1）

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | ops/s | plans/s | calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 6,397,038.61 | 6,397,038.61 | 6,397,038.61 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 9,932,052.35 | 9,932,052.35 | 9,932,052.35 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 31,506,234.30 | 30,781.59 | 30,781.59 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 39,114,575.86 | 38,332.28 | 38,332.28 | 1,020.41 |

说明：本轮是迁移验收脚本驱动的单次封盘（`n=1`），不提供 P50/P90/P99。

## 关键结论

1. `core + persist + wasm` 十二线已全部跑通并写入报告。
2. Core：`cpu_parity` auto 相比 single 提升约 `57.06%`；`cpu_batch_stress` 提升约 `6.13%`。
3. Persist：`cpu_parity` auto 相比 single 提升约 `35.45%`；`cpu_batch_stress` 提升约 `26.13%`。
4. Wasm：`cpu_parity` auto 相比 single 提升约 `55.26%`；`cpu_batch_stress` 提升约 `24.15%`。
5. Core 基线对比保持通过（`compare_pass=true`）；persist/wasm 本轮用于实测封盘，未绑定 baseline compare。

## 复现命令（macOS）

```bash
cd /Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/SUPERVM

# core
CARGO_HOME="$PWD/.cargo-local" pwsh -NoProfile -ExecutionPolicy Bypass \
  -File scripts/migration/run_performance_compare.ps1 \
  -RepoRoot "$PWD" \
  -OutputDir "$PWD/artifacts/migration/agent-baseline-2026-03-06-rerun-seal-single" \
  -BaselineJson "$PWD/artifacts/migration/baseline/svm2026-baseline-core.json" \
  -Variants core \
  -LineProfile seal_single \
  -IncludeCapabilitySnapshot:$false

CARGO_HOME="$PWD/.cargo-local" pwsh -NoProfile -ExecutionPolicy Bypass \
  -File scripts/migration/run_performance_compare.ps1 \
  -RepoRoot "$PWD" \
  -OutputDir "$PWD/artifacts/migration/agent-baseline-2026-03-06-rerun-seal-auto" \
  -BaselineJson "$PWD/artifacts/migration/baseline/svm2026-baseline-core.json" \
  -Variants core \
  -LineProfile seal_auto \
  -IncludeCapabilitySnapshot:$false

# persist
CARGO_HOME="$PWD/.cargo-local" pwsh -NoProfile -ExecutionPolicy Bypass \
  -File scripts/migration/run_performance_compare.ps1 \
  -RepoRoot "$PWD" \
  -OutputDir "$PWD/artifacts/migration/agent-baseline-2026-03-06-rerun-persist-seal-single" \
  -Variants persist \
  -LineProfile seal_single \
  -IncludeCapabilitySnapshot:$false

CARGO_HOME="$PWD/.cargo-local" pwsh -NoProfile -ExecutionPolicy Bypass \
  -File scripts/migration/run_performance_compare.ps1 \
  -RepoRoot "$PWD" \
  -OutputDir "$PWD/artifacts/migration/agent-baseline-2026-03-06-rerun-persist-seal-auto" \
  -Variants persist \
  -LineProfile seal_auto \
  -IncludeCapabilitySnapshot:$false

# wasm
CARGO_HOME="$PWD/.cargo-local" pwsh -NoProfile -ExecutionPolicy Bypass \
  -File scripts/migration/run_performance_compare.ps1 \
  -RepoRoot "$PWD" \
  -OutputDir "$PWD/artifacts/migration/agent-baseline-2026-03-06-rerun-wasm-seal-single" \
  -Variants wasm \
  -LineProfile seal_single \
  -IncludeCapabilitySnapshot:$false

CARGO_HOME="$PWD/.cargo-local" pwsh -NoProfile -ExecutionPolicy Bypass \
  -File scripts/migration/run_performance_compare.ps1 \
  -RepoRoot "$PWD" \
  -OutputDir "$PWD/artifacts/migration/agent-baseline-2026-03-06-rerun-wasm-seal-auto" \
  -Variants wasm \
  -LineProfile seal_auto \
  -IncludeCapabilitySnapshot:$false
```

## AOEM GPU MSM 专线补测（macOS，2026-03-06）

### 补测口径

- AOEM 仓库：`/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM`
- 示例：`cargo run -p aoem-gpu-kernels --example afp_stage1_matrix --features spirv-vulkan --release`
- 固定参数：
  - `num_points=262144`
  - `window_bits=10`
  - `num_windows=26`
  - `num_passes=2`
  - `include_p=0.10`
  - `hot_k=8`
  - `warmup=2`
  - `iters=5`
- Vulkan 运行时：
  - `VK_ICD_FILENAMES=/opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json`
  - `DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib`
  - `AOEM_GLSLC_PATH=/opt/homebrew/bin/glslc`
  - `AOEM_ENABLE_ASH_SPIRV=1`

### 补测结果

| lane | 关键开关 | p50_wall_ms | p50_stage_sum_ms | gpu_dispatch_hit_rate | 结果判定 |
|---|---|---:|---:|---:|---|
| dedicated_onesweep | `AOEM_AFP_FORCE_ONESWEEP=1` + onesweep staged 开关 | 197 | 25 | 0.0% | 失败（onesweep 管线创建失败） |
| dedicated_classic | `AOEM_AFP_FORCE_ONESWEEP=0` | 275 | 18 | 100.0% | 通过（真实 GPU dispatch） |
| primitive_default | `AOEM_MSM_PRIMITIVE_ROUTE=1`（默认 profile） | 141 | 124 | 0.0% | 失败（回退路径） |
| primitive_fused_v2 | `AOEM_MSM_PRIMITIVE_ROUTE=1` + `AOEM_PRIMITIVE_SORT_PROFILE=graph_sort_reduce_fused_v2` | 271 | 15 | 100.0% | 通过（真实 GPU dispatch） |

### 关键证据

- onesweep 失败日志包含：
  - `onesweep_error=Command execution failed: create compute pipeline failed: ERROR_INITIALIZATION_FAILED`
- primitive 默认 profile 失败日志包含：
  - `fallback_reason=dispatch_fail`
  - `vk_result=ERROR_INITIALIZATION_FAILED`
- dedicated_classic 与 primitive_fused_v2 的汇总均为：
  - `gpu_dispatch_hit_rate=100.0`
  - `consistency_hit_rate=100.0`
  - `readback_hash_match_rate=100.0`

### 产物路径

- dedicated_onesweep：
  - `/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM/tmp/msm-dedicated-onesweep-bench2-20260306-012848.log`
  - `/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM/tmp/msm-dedicated-onesweep-bench2-20260306-012848.csv`
- dedicated_classic：
  - `/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM/tmp/msm-dedicated-noonesweep-bench2-20260306-012830.log`
  - `/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM/tmp/msm-dedicated-noonesweep-bench2-20260306-012830.csv`
- primitive_default：
  - `/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM/tmp/msm-primitive-default-bench-20260306-012754.log`
  - `/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM/tmp/msm-primitive-default-bench-20260306-012754.csv`
- primitive_fused_v2：
  - `/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM/tmp/msm-primitive-fusedv2-bench2-20260306-012830.log`
  - `/Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM/tmp/msm-primitive-fusedv2-bench2-20260306-012830.csv`

### AOEM 复现命令（GPU MSM 专线）

```bash
cd /Users/aoem-a2/Downloads/dev_project/vsCode/WorksArea/AOEM
export CARGO_HOME="$PWD/.cargo-local"
export VK_ICD_FILENAMES=/opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json
export DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib
export AOEM_GLSLC_PATH=/opt/homebrew/bin/glslc
export AOEM_ENABLE_ASH_SPIRV=1
export AOEM_GPU_DIAG=1
export AOEM_AFP_NUM_POINTS=262144
export AOEM_AFP_WINDOW_BITS=10
export AOEM_AFP_NUM_WINDOWS=26
export AOEM_AFP_NUM_PASSES=2
export AOEM_AFP_WARMUP=2
export AOEM_AFP_ITERS=5
export AOEM_AFP_INCLUDE_PS=0.10
export AOEM_AFP_HOT_KS=8
export AOEM_AFP_SEED=7

# dedicated_classic（推荐：macOS 当前可稳定达成 100% GPU dispatch）
export AOEM_AFP_FORCE_ONESWEEP=0
unset AOEM_MSM_PRIMITIVE_ROUTE AOEM_MSM_PRIMITIVE_MIN_LEN AOEM_PRIMITIVE_SORT_PROFILE
cargo run -p aoem-gpu-kernels --example afp_stage1_matrix --features spirv-vulkan --release

# primitive_fused_v2（可达成 100% GPU dispatch）
export AOEM_AFP_FORCE_ONESWEEP=0
export AOEM_MSM_PRIMITIVE_ROUTE=1
export AOEM_MSM_PRIMITIVE_MIN_LEN=1
export AOEM_PRIMITIVE_SORT_PROFILE=graph_sort_reduce_fused_v2
cargo run -p aoem-gpu-kernels --example afp_stage1_matrix --features spirv-vulkan --release
```
