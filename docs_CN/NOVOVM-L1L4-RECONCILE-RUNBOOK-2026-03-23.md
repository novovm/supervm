# NOVOVM L1-L4 强一致回补手册（2026-03-23）

## 1. 目标

把“提交广播 + 回执确认 + 失败重放”收敛到统一状态机，减少人工分步介入。

## 2. 一键命令

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-reconcile.ps1 -RpcEndpoint http://127.0.0.1:9899 -SenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```

## 3. 脚本行为

1. 读取 `dispatch` 和 `submitted` 索引。
2. 维护统一状态文件 `l1l4-payout-state.json`。
3. 对未确认交易查询 `eth_getTransactionReceipt`。
4. 对超时/失败项按策略自动重放提交。
5. 输出本轮回补快照与索引。

## 4. 产物路径

1. 状态文件：`artifacts/l1/l1l4-payout-state.json`
2. 回补快照目录：`artifacts/l1/payout-reconcile`
3. 回补索引：`artifacts/l1/l1l4-payout-reconcile.jsonl`
4. 增量游标：`artifacts/l1/l1l4-payout-reconcile.cursor`

## 5. 关键参数

```powershell
-ReplayMaxPerPayout 3 -ReplayCooldownSec 30 -WeiPerRewardUnit 1
```

## 6. 重放

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-reconcile.ps1 -FullReplay -NoCursorUpdate -RpcEndpoint http://127.0.0.1:9899 -SenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```

## 7. 说明

该脚本是“手工单次执行”入口；主线生产路径已切到 `novovm-evm-gateway` 内嵌 Rust 常驻回补循环。  
统一状态机 + 自动重放 + 回执确认仍保持同一逻辑口径。
