# NOVOVM L1-L4 外部链确认手册（2026-03-23）

## 1. 目标

消费到账执行状态（executed），向外部链 RPC 查询回执并落库，形成“到账执行 -> 外部确认”闭环。

## 2. 一键命令

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-external-confirm.ps1
```

默认行为：

1. 读取 `artifacts/l1/l1l4-payout-executed.jsonl`
2. 按 cursor 增量处理
3. 为每个 voucher 生成 `.confirmed.json`
4. 追加确认索引 JSONL
5. 更新确认 cursor

兼容说明：也可直接读取真实广播产物 `artifacts/l1/l1l4-payout-submitted.jsonl`（通过 `-ExecutedIndexFile` 指定）。

## 3. 产物路径

1. 确认目录：`artifacts/l1/payout-confirmed`
2. 确认索引：`artifacts/l1/l1l4-payout-confirmed.jsonl`
3. 增量游标：`artifacts/l1/l1l4-payout-confirm.cursor`

## 4. 核心参数

```powershell
-RpcEndpoint http://127.0.0.1:9899 -RpcMethod eth_getTransactionReceipt -RpcTimeoutSec 15
```

说明：

1. `RpcMethod` 默认 `eth_getTransactionReceipt`。
2. 有回执则标记 `confirmed_v1`，无回执标记 `pending_external_confirm_v1`，请求异常标记 `confirm_error_v1`。

## 5. 常用操作

### 5.1 全量重放（不更新 cursor）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-external-confirm.ps1 -FullReplay -NoCursorUpdate
```

### 5.2 指向外部网关

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-external-confirm.ps1 -RpcEndpoint http://your-gateway:9899
```

### 5.3 从真实广播提交索引做确认

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-external-confirm.ps1 -ExecutedIndexFile artifacts/l1/l1l4-payout-submitted.jsonl -RpcEndpoint http://your-gateway:9899
```

## 6. 说明

当前版本完成了“外部链回执确认”的最小生产流程（回执落库 + 重放）。  
后续可继续接入真实交易签名与广播链路，替换当前状态层 `tx_hash_hex`。
