# A→C同一对象与会话的双端接管匹配

2026-11-08，C在原库existing更新至ddd89e8e后，以PID23024、LocalService运行。
A基于C就绪交接由原运行owner发布一条固定非业务对象，公开发送证据提交cc6d5a08。
C的有界只读采集提前运行，收到后及时保存原progress及底层观察，公开接收证据提交f7d5ec6d。
本轮独立读取双方原件核对，结果为同一对象、同一认证会话的完整有界接管匹配通过。

| 关联字段 | 实际值 |
| --- | --- |
| Object ID | 8ed4aa5273e61b563f402fe9490aaa0690c020bfd840bb888744e80392740343 |
| Session ID | 1d1181eae8f4d2117ff43dd624f4fafe |
| transfer tag | fc4142b2cde83e9e6238e8304d02d35981ad7100f2c73c5d2d92f270572591c9 |
| 实际路径 | authenticated_bound_carrier |
| A/C进程 | 24444 / 23024 |

## 原始时间线与逐项核对

| 所有者及事件 | 原始时刻/值 |
| --- | --- |
| A本地ctl命令 | 原377字节nvprobe文件及command.json，ok=true，发布时sent=false |
| A原owner接纳 | probe_owned，wall_ms=1794105660318 |
| A原lane准入 | probe_admitted，wall_ms=1794105660318 |
| C收到完整分片 | 1/1片，fragments_complete_us=643861980 |
| C完成hash核验 | verified_us=643861991 |
| C核对实际认证发送者及目标 | probe_received_integrity_verified，wall_ms=1794105660253 |
| C原对象反馈 | feedback_sent.packets=3，首个窗口missing=0，first_us=643877382 |
| A反馈进展与终态 | acknowledged_best=1、send_terminal_state=delivered，transport_done_us=4638863404 |
| A原owner终态 | probe_object_done，wall_ms=1794105660376 |

各机壁钟及本地monotonic分别保留。C壁钟记录早于A部分记录，跨机壁钟差不作为网络时延。
字段按所属owner的阶段核对，完整性、会话、身份、原反馈及发送终态共同形成关联。

- A原progress前快照无本次Object ID，后快照有且仅有本次owned/admitted/done阶段。
- A/C的Object、Session、nonce、sender、recipient逐字节相同。
- A实际peer为C，C实际认证peer为A，同一carrier binding逐字节相同。
- 原算法按Session、NVBFT001控制流、Object生成transfer tag，双方底层记录均唯一匹配。
- C接收记录transport_done_us为null，原值保留。C的完整性/接收者核验及缺片为0反馈，
  与A发送端真实TransportDone、delivered及probe_object_done配对签收。
- A与C的原始数据均保留覆盖计数，目标tag存在。C全观察器expired=40与本对象分开记账。
- A原八份文件逐项SHA256匹配其SHA256SUMS。双方原ZIP SHA256及规范化字段见匹配结果。

## 原件与范围

[matched-result.json](matched-result.json)包含完整独立关联结果、原阶段及底层记录。
原件保留在[A公开发送证据](../validator-a-c-object-send-20261108/README.md)和
[C公开接收证据](../validator-c-ddd-A-C-receiver-20261108/README.md)。
C更新及失败保留见[本轮原库更新](../validator-c-ddd-existing-update-20261108/README.md)、
[前次失败与回滚](../validator-c-ddd-update-rollback-20261108/README.md)。

本轮签收的是固定ddd89e8e下A→C单条固定非业务对象的认证carrier双端有界接管。
原生UDP直连、单/双中继拓扑、四机完整通信、Business Receipt、共识最终性和TPS继续各自验收。
C保持当前existing节点在线，D及NOVOCHAT处于本轮操作范围之外。
