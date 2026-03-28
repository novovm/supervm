# NOVOVM L1-L4 收益结算周期手册（2026-03-23）

## 1. 目标

基于 `l1l4-anchor.jsonl` 周期化生成结算凭据（voucher），用于收益结算对账。

## 2. 一键命令

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-settlement-cycle.ps1
```

默认行为：

1. 读取 `artifacts/l1/l1l4-anchor.jsonl`
2. 按 cursor 增量处理新锚点
3. 输出结算凭据 JSON
4. 追加结算索引 JSONL
5. 更新 cursor

## 3. 产物路径

1. 结算凭据目录：`artifacts/l1/settlement-cycles`
2. 结算索引：`artifacts/l1/l1l4-settlement-vouchers.jsonl`
3. 增量游标：`artifacts/l1/l1l4-settlement.cursor`

## 4. 核心参数

```powershell
-PenaltyFailedFile 1 -RewardPerScoreUnit 1
```

评分公式：

```text
score = l4_ingress_ops + l3_routed_batches + l2_exec_ok_ops - PenaltyFailedFile * l2_exec_failed_files
reward_units = score * RewardPerScoreUnit
```

## 5. 常用操作

### 5.1 全量重算（忽略 cursor）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-settlement-cycle.ps1 -FullReplay -NoCursorUpdate
```

### 5.2 调整失败惩罚和奖励系数

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-settlement-cycle.ps1 -PenaltyFailedFile 2 -RewardPerScoreUnit 10
```

## 6. 说明

当前版本完成了“按锚点周期汇总并出结算凭据”的最小生产闭环。  
后续可在此基础上接入自动收益发放流程。

