# A 侧只读验收 · C 的 eecb753b 原库启动

C 已用原 LocalService 入口、原身份和原库 `existing` 启动一次。北京时间 19:27:08 复核时 PID 12148 仍运行，持有 `192.168.71.73:41052` 和 `192.168.71.73:42052`。C 本机显示高度 1、第 580 轮、待处理 0、回执索引高度 0。A 侧认证与按哈希查询待回传。见 [C 启动回执](C-existing/C-existing-public.json)。

## A 的查询步骤

1. 使用已核验的 `novovm-common-eecb753b-windows-x86_64.zip` 中的 `bin/novovm-native-client.exe`。先核对 Client SHA256 为 `a5d81e609d971ec538527abbfd3b011816dc695dce661e6ecd31bef22293910a`。完整 ZIP 的大小及哈希见启动回执。
2. 使用 A 原 C 查询配置及其既有信任与认证身份。若配置里的 C 地址已过时，只在 A 的查询副本中将目标更新到 `192.168.71.73:41052`。用 `status` 记录认证结果和实际 `Status.current_height`、`Status.round`。
3. 立即按认证结果中的高度和轮次查询。同一进程若已换轮，使用响应中新的 `current_height/current_round` 补查当前状态。`observation=null` 表示请求位置与当前状态不匹配，不能作为历史证据。

```text
novovm-native-client.exe <A原C查询配置> status
novovm-native-client.exe <A原C查询配置> consensus-proposal <current_height> <round> 68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4
```

4. 回传 A 的认证查询时间、实际高度和轮次，以及 `in_pending_pool`、`selection_gate`、`in_local_offer`、候选标识和签名提案证据。只有确认签名提案包含该交易，才沿同一个区块哈希核对 AOEM 执行、三名不同签名者的有效 QC 和持久化业务回执。缺少哪一环就注明哪一环。
5. 若当前已认证查询及既有回执证据表明本交易未恢复，记录“节点恢复、交易未恢复”。若认证失败、查询超时或没有足够证据，回传实际结果和缺口。当前范围仅为只读查询，C 保持现有进程，原交易保持单次提交。\n