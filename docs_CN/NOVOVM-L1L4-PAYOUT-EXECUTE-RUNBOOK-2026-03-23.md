# NOVOVM L1-L4 到账执行手册（2026-03-23）

## 1. 目标

消费自动发放指令（dispatch），生成到账执行状态（executed），完成“发放指令 -> 到账状态”闭环。

## 2. 一键命令

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-payout-execute.ps1
```

默认行为：

1. 读取 `artifacts/l1/l1l4-payout-dispatch.jsonl`
2. 按 cursor 增量处理新 dispatch
3. 为每个 voucher 生成 `.executed.json`
4. 追加到账执行索引 JSONL
5. 更新 execute cursor

## 3. 产物路径

1. 到账状态目录：`artifacts/l1/payout-executed`
2. 到账索引：`artifacts/l1/l1l4-payout-executed.jsonl`
3. 增量游标：`artifacts/l1/l1l4-payout-execute.cursor`

## 4. 核心参数

```powershell
-ChainId 1 -ExecutionMode ledger_status_only_v1
```

说明：

1. 当前最小版本默认以 `ledger_status_only_v1` 记录到账状态。
2. `tx_hash_hex` 为可追溯执行标识（伪交易哈希），用于状态审计与后续对接真实链上广播。

## 5. 常用操作

### 5.1 全量重放（不更新 cursor）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-payout-execute.ps1 -FullReplay -NoCursorUpdate
```

### 5.2 指定链 ID

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-payout-execute.ps1 -ChainId 9000
```

## 6. 说明

当前版本完成了“到账执行状态”最小闭环。  
后续可在此基础上接入真实外部链广播、确认回执、失败补偿重放。

