# NOVOVM L1-L4 真实签名与广播手册（2026-03-23）

## 1. 目标

消费 dispatch 发放指令，调用真实 RPC 广播交易，产出提交状态（submitted）。

## 2. 前置条件

1. 外部节点支持 `eth_sendTransaction` 或 `eth_sendRawTransaction`。  
2. 如果用 `eth_sendTransaction`，发送方账户必须已在外部节点可签名（例如解锁账户或外接签名器）。  
3. 准备地址映射文件（若 `payout_account` 不是直接 `0x` 地址）：`artifacts/l1/payout-address-map.json`。

示例映射：

```json
{
  "novovm-l1-01": "0x1111111111111111111111111111111111111111",
  "novovm-l2-01": "0x2222222222222222222222222222222222222222",
  "uca:novovm-l3-01": "0x3333333333333333333333333333333333333333"
}
```

## 3. 一键命令（eth_sendTransaction）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-real-broadcast.ps1 -RpcEndpoint http://127.0.0.1:9899 -SenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -WeiPerRewardUnit 1
```

## 4. 原始交易广播（eth_sendRawTransaction）

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-real-broadcast.ps1 -RpcMethod eth_sendRawTransaction -RpcEndpoint http://127.0.0.1:9899
```

说明：该模式要求 dispatch 指令里已携带 `signed_raw_tx_hex`。

## 5. 产物路径

1. 提交目录：`artifacts/l1/payout-submitted`
2. 提交索引：`artifacts/l1/l1l4-payout-submitted.jsonl`
3. 增量游标：`artifacts/l1/l1l4-payout-submit.cursor`

## 6. 常用参数

```powershell
-WeiPerRewardUnit 1 -GasLimit 21000 -MaxFeePerGasWei 0 -MaxPriorityFeePerGasWei 0
```

## 7. 重放

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-l1l4-real-broadcast.ps1 -FullReplay -NoCursorUpdate -RpcEndpoint http://127.0.0.1:9899 -SenderAddress 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```

