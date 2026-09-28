# 跨轮完整确认凭证追赶 V2

## 本刀交付

实验模式 `commit_v2_enabled=true` 的节点现在可以接收**与本地轮次不同的完整
V2 确认凭证**。即使本节点尚未 prepared，也能验证、持久保存、重传和重启恢复。
仍须已有同一候选的本地 AOEM-owned 执行事实；不会从远端凭证补造执行结果。

例：三个节点在 round 1 形成确认凭证，而延迟节点仍在 round 0。延迟节点可以
保存该凭证，不必先逐轮推进自己的投票状态，也不会为 round 1 新签名。

这只解决“完整凭证已经存在，其他轮次的节点如何追赶”。**若没有任何轮次形成
完整凭证，各轮只有零散确认票，本刀仍不支持混票、改签名目标或解锁重签。**
V2 确认签名与安全锁语义不变，不声称已完成一般跨轮活性或主网最终性。

## 验证和存储边界

1. 通过原有有界、认证的 kind 8 消息验证发送来源、固定 authority、链域、创世、
   协议配置、验证者集合、height、scheduled leader、完整 proposal/prepare QC、
   非零轮 new-view 证据及严格超过 2/3 的确认权重。单票/不足额凭证不能追赶。
2. 以本地固定候选重建完整 subject，核对根、父依赖和执行承诺，不仅比较块 hash。
3. ingress 仅暂存一个已验证的完整凭证；本地 poll 才将其存入独立的 observation
   记录，与内容哈希标记组成一个同步 RocksDB batch，并读回核对。
4. observation 不写入本地 active proposal/QC/new-view-admission，不修改 round
   tracker、投票锁或 outbox；它不是新的签名许可。之后只重传原始完整凭证。
5. 已存凭证不覆盖；相同目标的其他签名子集不替换旧证据。已有本地完整确认凭证
   或 observation 与新目标冲突时拒绝，不把同块不同轮的完整证书静默当成同一目标。
6. 重启重新验证 envelope、本地候选、authority 和内容哈希。记录与标记不成对、
   证据内容篡改或替换均报错；证据损坏时停止报告确认成功。没有承诺检测整个
   数据库回滚或全部恢复证据同时被删除；正常重启也不等于物理断电验证。

## 状态和观测

- `round`：本地投票 tracker 的轮次，追赶不会修改它。
- `commit_round`：已验证确认凭证的签名轮次。
- `commit_observed=true`：该凭证来自独立的跨轮观察归档，不是本地 active QC
  路径的证书；不能假定它存在于 `load_commit_certificate_by_height_v2` 的槽中。
- `commit_confirmed=true`：已持久验证完整确认凭证，可能是本地路径，也可能是
  observation。`prepared=false` 与之可以同时成立，不代表本节点参与过该轮。

`proof_sealed/chain_canonical/safe/finalized` 均保持原状（测试中为 false）。
没有增加最终祖先验证、fork choice、连续出块或 AOEM/ledger 跨库晋升。

## 验证范围

受控四独立签名者/数据库测试验证已 prepared 且已签过一票 commit 的 round 0
节点追赶 round 1、不足额和错误来源拒绝、
ingress 不写盘、原有数据库键值不变（只新增 observation 及哈希两个记录）、
原样重传、重启恢复和丢失标记后停机。另有本机真实 WSS 通道中尚未 prepared
节点的跨轮凭证追赶、
认证去重和重启测试。候选执行事实为合成 fixture，不是独立 AOEM 实机签收。
实体局域网、公网、断电和长期运行未在本刀执行；AOEM 内核/DLL/ABI、聊天服务
及现有操作员数据库未改。
