# AOEM FFI beta0.8 TPS 封盘（Linux Core+Persist+Wasm 十二线，2026-03-10）

## 本次封盘目标

- 基于当前 Linux 环境，补齐 `core + persist + wasm` 十二线 FFI TPS 文档。
- 仅使用 FFI V2 typed 二进制路径（`aoem_execute_ops_v2`）。
- `threads=auto` 保持联合自适应，口径为 `threads * engine_workers <= budget_threads`。

## 固定口径（不可混写）

- 示例程序：`crates/aoem-bindings/examples/ffi_perf_worldline.rs`
- Core so：`artifacts/aoem-platform-build/linux/core/bin/libaoem_ffi.so`
- Persist so：`artifacts/aoem-platform-build/linux/persist/bin/libaoem_ffi.so`
- Wasm so：`artifacts/aoem-platform-build/linux/wasm/bin/libaoem_ffi.so`
- 固定参数：
  - `txs=1,000,000`
  - `key_space=128`
  - `rw=0.5`
  - `seed=123`
  - `warmup_calls=5`

## 测试环境

- OS：Ubuntu 24.04.3 LTS（Noble Numbat）
- Kernel：Linux 6.17.0-14-generic（x86_64）
- CPU：AMD Ryzen AI MAX+ 395 w/ Radeon 8060S
- Rust：`rustc 1.94.0`
- Cargo：`cargo 1.94.0`
- PowerShell：`7.5.4`
- Python：`3.12.3`
- 报告生成时间（UTC）：`2026-03-10T16:48:25Z`

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

- `artifacts/migration/linux-ffi-tps-seal-2026-03-10/ffi-linux-tps-raw.csv`
- `artifacts/migration/linux-ffi-tps-seal-2026-03-10/logs/`

## Core 实测矩阵（n=1）

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | selected_threads | selected_engine_workers | total_workers | ops/s | plans/s | calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 1 | 16 | 16 | 4,092,559.90 | 4,092,559.90 | 4,092,559.90 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 15 | 2 | 30 | 19,294,546.24 | 19,294,546.24 | 19,294,546.24 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 1 | 16 | 16 | 12,044,286.65 | 11,767.27 | 11,767.27 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 22 | 1 | 22 | 43,918,617.57 | 43,479.43 | 43,479.43 | 1,010.10 |

## Persist 实测矩阵（n=1）

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | selected_threads | selected_engine_workers | total_workers | ops/s | plans/s | calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 1 | 16 | 16 | 3,555,640.42 | 3,555,640.42 | 3,555,640.42 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 15 | 2 | 30 | 11,927,137.07 | 11,927,137.07 | 11,927,137.07 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 1 | 16 | 16 | 10,146,458.75 | 9,913.09 | 9,913.09 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 22 | 1 | 22 | 20,750,468.93 | 20,542.96 | 20,542.96 | 1,010.10 |

## Wasm 实测矩阵（n=1）

| line_name | preset | submit_ops | threads_arg | engine_workers_arg | selected_threads | selected_engine_workers | total_workers | ops/s | plans/s | calls/s | avg_ops_per_plan |
|---|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|
| cpu_parity_single | cpu_parity | 1 | 1 | 16 | 1 | 16 | 16 | 4,248,221.55 | 4,248,221.55 | 4,248,221.55 | 1.00 |
| cpu_parity_auto_parallel | cpu_parity | 1 | auto | auto | 15 | 2 | 30 | 18,211,328.81 | 18,211,328.81 | 18,211,328.81 | 1.00 |
| cpu_batch_stress_single | cpu_batch_stress | 1024 | 1 | 16 | 1 | 16 | 16 | 12,313,455.91 | 12,030.25 | 12,030.25 | 1,023.54 |
| cpu_batch_stress_auto_parallel | cpu_batch_stress | 1024 | auto | auto | 22 | 1 | 22 | 43,901,611.75 | 43,462.60 | 43,462.60 | 1,010.10 |

说明：本轮是单次封盘（`n=1`），不提供 P50/P90/P99；自适应并行的 `selected_threads`、`selected_engine_workers`、`total_workers` 取自对应日志首行。

## 自适应并行证据

- 本机 `hw_threads=32`，预算 `budget_threads=30`。
- `cpu_parity_auto_parallel` 三个变体均选中 `selected_threads=15`、`selected_engine_workers=2`、`total_workers=30`。
- `cpu_batch_stress_auto_parallel` 三个变体均选中 `selected_threads=22`、`selected_engine_workers=1`、`total_workers=22`。

## 关键结论

1. Linux `core + persist + wasm` 十二线已全部补齐并写入正式文档。
2. 本机 32 线程环境下，`cpu_parity` auto 相比 single 的提升分别为：Core `371.45%`、Persist `235.44%`、Wasm `328.68%`。
3. `cpu_batch_stress` auto 相比 single 的提升分别为：Core `264.64%`、Persist `104.51%`、Wasm `256.53%`。
4. Linux 本轮最高吞吐出现在 `core cpu_batch_stress_auto_parallel`，达到 `43,918,617.57 ops/s`；Wasm 同档位达到 `43,901,611.75 ops/s`。
5. 本次封盘基于真实 `.so` 实测与日志留存，不混入旧 Windows 口径，也不混入 baseline compare 结论。

## 复现命令（Linux）

```bash
cd /home/aoem-a3/WorksArea/SUPERVM/crates/aoem-bindings
cargo build --release --example ffi_perf_worldline

cd /home/aoem-a3/WorksArea/SUPERVM

# core
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/core/bin/libaoem_ffi.so" --preset cpu_parity --submit-ops 1 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/core/bin/libaoem_ffi.so" --preset cpu_parity --submit-ops 1 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/core/bin/libaoem_ffi.so" --preset cpu_batch_stress --submit-ops 1024 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/core/bin/libaoem_ffi.so" --preset cpu_batch_stress --submit-ops 1024 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# persist
export AOEM_PERSISTENCE_PATH="$PWD/artifacts/migration/linux-ffi-persist-db/manual-run"
mkdir -p "$AOEM_PERSISTENCE_PATH"
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/persist/bin/libaoem_ffi.so" --preset cpu_parity --submit-ops 1 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/persist/bin/libaoem_ffi.so" --preset cpu_parity --submit-ops 1 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/persist/bin/libaoem_ffi.so" --preset cpu_batch_stress --submit-ops 1024 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/persist/bin/libaoem_ffi.so" --preset cpu_batch_stress --submit-ops 1024 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5

# wasm
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/wasm/bin/libaoem_ffi.so" --preset cpu_parity --submit-ops 1 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/wasm/bin/libaoem_ffi.so" --preset cpu_parity --submit-ops 1 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/wasm/bin/libaoem_ffi.so" --preset cpu_batch_stress --submit-ops 1024 --threads 1 --engine-workers 16 --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
./target/release/examples/ffi_perf_worldline --dll "$PWD/artifacts/aoem-platform-build/linux/wasm/bin/libaoem_ffi.so" --preset cpu_batch_stress --submit-ops 1024 --threads auto --engine-workers auto --txs 1000000 --key-space 128 --rw 0.5 --seed 123 --warmup-calls 5
```
