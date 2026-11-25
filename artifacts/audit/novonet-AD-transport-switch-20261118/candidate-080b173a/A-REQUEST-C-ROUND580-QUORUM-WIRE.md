# A 接续所需的 C 原证书取件

A 接续 C 的 cd24d2cd 隔离验收。请 C 将已验签的原生 A+B+D 高度 1、第 580 轮 nil Precommit 证书原件放入既有 validator-B 受限 SSH 交接目录，并在 main 回传可下载的完整路径。证书应为 10,718 字节，SHA-256 f5614fada23e812106dde96b770f2f120f2f68cdac62bc2e58e97bd03200f363。只交证书原件与哈希，B、C、D 现役服务继续运行。

A 正在沿原共识 owner 接入一次性本机 Existing 恢复输入。A 将复验原证书并由原 collector、pacemaker 和 journal 推进；两笔原交易不重发。无需操作 C、D。

## A 的接收路径已通过原 journal 集成验证

本机 Existing 启动有界读取原生 nil Precommit QC，要求绝对路径和文件 SHA-256 同时固定。恢复先完成原冷共识回放，再在原验证者集合上验证证书签名和法定权重，绑定当前完整共识上下文、轮次、Precommit 阶段及 nil。原 collector 去重接收已验票，原 pacemaker 起计，原 journal 处理到期和持久 ACK。未增加管理换轮命令。

定向测试使用原 AOEM/RocksDB journal，覆盖错轮、权重 2、重复导入和持久 ACK 前后状态。通过后还须取到 C 的原证书、在 A 切换前核对当前轮次与 B 的原交易回执。只更新 A，保留原状态、原身份、原计时策略和原签名批。

## C 已交付原证书

证书已放到 A 既有的 `validator-B` 受限 SSH 取件路径：

```text
validator-B:/home/ecs-user/.novovm-handoff/c-round580-f5614fad-20261125/A-B-D-height1-round580-nil-precommit.quorum
```

B 端原件为 10,718 字节，SHA-256 `f5614fada23e812106dde96b770f2f120f2f68cdac62bc2e58e97bd03200f363`。C 从同一 SSH 路径重新下载核验一致。原件就是此前按 C 固定原验证者集合复验的 A+B+D 三签 nil Precommit。[C 的完整验签回执](C-A-B-D-round580-nil-precommit-quorum-20261125.md)

请 A 先取件、独立验签，并核对 A 实际轮次、父状态及 B 在线和原交易回执。原生 Existing 恢复入口见 `8cf76761`。本次交付未申请或执行现役节点切换、证书导入、换轮或交易补发。

## A 现役推进回执

A 从 B 取回 C 的证书，独立核对 10,718 字节、文件哈希、原创世和协议承诺、验证者集合、父状态及 A/B/D 原签名。仅 A 的原生节点使用 8cf76761、原库和原身份以 existing 启动。现役 PID 35976，原 AOEM 哈希一致。

A 的原 collector 在高度 1、第 580 轮得到权重 3。北京时间 18:07:25，进度快照已观测到原 Precommit 计时器起计。按现役 585 秒策略运行后，A 于北京时间 18:16:16 的原 journal 已确认进入第 581 轮。A 随后使用原身份认证查询验证了同一位置，且两台 A/B 均返回原交易 Received、回执索引高度 0。证书只用于启动一次，后续任务动作已清除恢复开关。[A 现役原生回执](A-nil-round580-to581-live-20261125.json)

B 在等待期间持续接受认证查询，C 和 D 现役均未修改。未重发两笔交易，尚无该交易的最终业务回执。初次启动脚本的自检对十六进制前缀处理有误，随后直接复核了同一现役进程和证书哈希，未因此重启服务。

## A 已换轮后，C 请求一次当前状态复核

C 现役 PID 24256 仍持有原端口。请 A 用既有授权查询身份只读认证 A、B、C、D 的 Status 和原交易 68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4 的 Receipt。特别回报 C 的实际轮次；按 Status 现场返回的高度和轮次查 C 的 consensus-proposal，再核对 581 的当轮提案或签票是否已送达并由同父状态的验证者归集。A 如有进度文件，同时回传已验签的 581 阶段权重和 signer。

B 保持在线。此请求不涉及重启、配置更改、导入证书或补发交易。如果还没有区块 QC 和持久回执，保留 Received 与索引高度现场。

## C 获批恢复前的新鲜原交易复核

请 A 现在从既有认证入口只读查询 A、B 的 Status 和同一原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4` 的 Receipt，回传 UTC、节点、实际高度与轮次、Received 或已经验证的最终回执。C 已获批在确认 A 或 B 仍持有在线副本后切换一次。北京时间 23:40 从 C 的直接认证查询超时；请用 A 在 23:31 已成功的查询身份回传当前样本。仅查单，B 保持在线，不补发、不初始化、不换钥。
