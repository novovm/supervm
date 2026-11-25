# C 第 580 轮进入 Prevote 的持久证据

使用 [22:49 独立文件副本](C-post-A581-durable-state-20261125.md) 已导出的原 C journal 和 outbox。按 `080b173a` codec 核查 786 条 outbox 的原格式尾校验及键内 revision。高度 1、第 580 轮仅有一条带消息的 outbox，revision `786`，其消息为已由原 `ValidatorJournal::open` 验签的 C nil `Prevote`，没有该轮的持久 Proposal。该 outbox 的 snapshot 摘要与当前快照匹配，票后唯一 replay evidence 标签是 `0`（`None`）。记录 SHA-256 为 `20bc736c5b32f12883add10e89facd2d8177dd6c599e1ceb213182b6696c94d5`；原票字节 SHA-256 为 `558fc9b7afabecfaf135213a5579dc769ec391dc455963ecd7a1bfaa1bdbc6ef`。

`080b173a` 只通过接受提案或本地 `Propose` 超时产生 `Prevote`。接受提案即使因锁规则投 nil，也会写入 `ReplayEvidence::Proposal`；`TimeoutStep::Propose` 路径写入 `ReplayEvidence::None`。故 C 的 580 nil `Prevote` 来自本地提案计时器超时。C 是本轮排程提案者，但到该次转换没有形成持久签名提案。

[23:31 认证回执](A-post581-four-host-recheck-20261125.json)说明当时本地候选已经存在。现役服务未保留候选首次就绪与计时器实际触发的时刻，无法把候选首次就绪排到该次超时之前或之后，也无法确认逐票接收入账。原服务及数据库未改动。C PID 24256 运行，B PID 571511 在线，交易未补发。
