# NOVOVM Full 模式最小启动模板（2026-04-18）

Status: OPERATIONAL TEMPLATE  
Scope: 生产 `novovm-node` full 主路径启动（单 ingress 源）+ P2-D 日报

## 目的

本模板用于给出 `novovm-node` 生产 full 模式的最小可复现启动路径。

它保持生产边界，不启用 P3（`Decision Only / Not Enabled`）。

## 运行硬约束（代码合同）

1. `novovm-node` 仅接受 `NOVOVM_NODE_MODE=full`。
2. `NOVOVM_EXEC_PATH` 必须是 `ffi_v2`。
3. 不允许 `NOVOVM_ENABLE_HOST_ADMISSION=1`。
4. ingress 源必须三选一且仅能选一个：
   - `NOVOVM_TX_WIRE_FILE`
   - `NOVOVM_OPS_WIRE_FILE`
   - `NOVOVM_OPS_WIRE_DIR`

参考合同：
- `crates/novovm-node/src/bin/novovm-node.rs`（full 模式与 ingress 约束）
- `crates/novovm-exec/src/lib.rs`（`AoemRuntimeConfig::from_env` 默认行为）

## Step 1：准备最小 ingress 输入

先生成一个小规模 `.txwire` 样本：

```powershell
cargo run -p novovm-bench --bin novovm-txgen -- `
  --out artifacts/mainline/full-mode/ingress.txwire.bin `
  --txs 128 `
  --accounts 16
```

## Step 2：设置最小环境变量（PowerShell）

```powershell
$env:NOVOVM_NODE_MODE = "full"
$env:NOVOVM_EXEC_PATH = "ffi_v2"
$env:NOVOVM_ENABLE_HOST_ADMISSION = "0"

$env:NOVOVM_TX_WIRE_FILE = (Resolve-Path "artifacts/mainline/full-mode/ingress.txwire.bin").Path

Remove-Item Env:NOVOVM_OPS_WIRE_FILE -ErrorAction SilentlyContinue
Remove-Item Env:NOVOVM_OPS_WIRE_DIR -ErrorAction SilentlyContinue
```

如果 AOEM 自动定位失败，可显式指定其中一个：

```powershell
$env:NOVOVM_AOEM_ROOT = "D:\WEB3_AI\SUPERVM\aoem"
# 或
$env:NOVOVM_AOEM_DLL = "D:\WEB3_AI\SUPERVM\aoem\core\bin\aoem_ffi.dll"
```

## Step 3：启动生产 full 主路径节点

```powershell
cargo run -p novovm-node --bin novovm-node
```

启动后建议确认日志中出现：
- `d1_ingress_contract: ...`
- `tx_ingress_source: ...`
- `mode=ffi_v2 ...`

## Step 4：执行 P2-D 日报

在另一个终端运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/novovm-p2d-daily-report.ps1 `
  -RpcUrl "http://127.0.0.1:8899"
```

说明：
- 脚本优先走 RPC。
- 若 RPC 不可达，会自动 fallback 到 `supervm-mainline-query`。
- 输出目录：
  - `artifacts/mainline/p2d-run-phase/<YYYY-MM-DD>/`

## ingress 变体与约束

1. `NOVOVM_TX_WIRE_FILE`
   - full 模式最简单入口。
2. `NOVOVM_OPS_WIRE_FILE`
   - 需要 `ops_wire_v1` 路径。
3. `NOVOVM_OPS_WIRE_DIR`
   - 目录内必须存在 `.opsw1` 文件。
   - 必须 `NOVOVM_TX_REPEAT_COUNT=1`。

不要同时设置多个 ingress 变量。

## 非本模板范围

- 启用 P3-A/P3-B/P3-C。
- 重新开放非 full node mode。
- 修改已封盘的 P2-C/P2-D 语义。

