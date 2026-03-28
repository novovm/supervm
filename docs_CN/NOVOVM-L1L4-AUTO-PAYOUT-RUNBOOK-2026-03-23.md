# NOVOVM L1-L4 自动收益发放手册（2026-03-23）

## 1. 目标

消费 settlement voucher，自动生成收益发放指令（dispatch），形成“周期结算 -> 发放指令”闭环。

## 2. 一键命令

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-auto-payout.ps1
```

默认行为：

1. 读取 `artifacts/l1/l1l4-settlement-vouchers.jsonl`
2. 按 cursor 增量处理新 voucher
3. 为每个 voucher 生成 `.payout.json`
4. 追加 dispatch 索引 JSONL
5. 更新 payout cursor

## 3. 产物路径

1. 发放指令目录：`artifacts/l1/payout-instructions`
2. 发放索引：`artifacts/l1/l1l4-payout-dispatch.jsonl`
3. 增量游标：`artifacts/l1/l1l4-payout.cursor`

## 4. 关键参数

```powershell
-MinRewardUnits 1 -PayoutAccountPrefix "uca:"
```

说明：

1. 小于 `MinRewardUnits` 的节点奖励会被本轮跳过。
2. `payout_account` 默认由 `PayoutAccountPrefix + node_id` 生成。

## 5. 常用操作

### 5.1 全量重放（不更新 cursor）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-auto-payout.ps1 -FullReplay -NoCursorUpdate
```

### 5.2 提高最小发放门槛

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-auto-payout.ps1 -MinRewardUnits 100
```

## 6. 说明

当前版本完成了“自动生成发放指令”的最小生产流程。  
后续可在此基础上接入真正的链上到账执行与失败补偿重放。

