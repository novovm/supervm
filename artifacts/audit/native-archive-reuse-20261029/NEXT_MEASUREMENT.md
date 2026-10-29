# 下一轮最小测量：原主循环与正文复用

本文件是未完成性能任务的交接，不是新协议实现批准或发布签收。当前修复不能证明恢复历史约 12K TPS；不再用组件通过数替代整链成绩。

## 已核实的边界

- 固定基线 `0897c596` 的资产消息上限 M 为 8 MiB＋128 KiB；数据／入站 lane 每 peer 为 4M、4 个对象，全局为 16M、32 个对象。应用 lane 复制入站预算；控制 lane 保持原限制。
- 因此当前数据 lane 是每 peer 32.5 MiB、全局 130 MiB，不存在“5.4 MiB 合法正文被旧 4 MiB 限制永远挡住”的源码依据。发送与重组确实按完整对象保留额度，但不是只能保留一个 3.44 MB 对象。
- 来源：`crates/novovm-node/src/native_pipeline/service.rs` 的 `native_decode_limits`、`native_lane_limits`、`start_native`；`crates/novovm-network/src/novorudp/object_link.rs::try_send_until`、`fragments.rs::push`。
- 原增量 hash 进度 API 源于 `7a8b22d8b`（2026-10-23）；native 消费者仍保留 `f5c2fcd6c` 的 Option-only 接线。本轮补回真实 hash 进展到原空闲提示，不是 AOEM 密码运算回退修复。
- 普通消息与查询现共用 `consensus/native.rs::poll_complete`；缺片和空队列不能制造进展。没有改变量子、超时、容量、共识或发布规则。

## A：原控制线程是否被同步正文工作占住

原调用链为 `bin/novovm-node.rs` 原 tick → `service/rpc.rs::RpcLifecycle::poll` → `service.rs::ResidentNode::poll`：先一次 `poll_io`，随后完整 assembly、投影、准入、查询、gossip、选批。

- 网络发生实际进展时不会固定休眠；不能描述成“每处理 64 片强制睡 1 ms”。
- `consensus/native.rs::prepare_kind`：完整消息编码与整对象摘要。
- `consensus/native.rs::receive_stream`：完整对象拼接、解码及证据检查。
- `consensus/bodies.rs::offer_inner`：正文身份计算，以及为资源计量再次完整编码。
- `service/rpc/gossip.rs::poll_gossip`：选取已持有行、组批与准备传播。
- 上述同步工作位于下一次 I/O 机会之前；存在机制依据，但尚未测出各段对慢高度的贡献。

现有可复用材料，不新增缓存或所有者：

- `service/rpc.rs::Pending` 持有原批次／行索引和本 pipeline 的验签凭证；`rpc/uca.rs::selected_uca_batch` 已复用不可变行视图。
- `rpc.rs::admitted_for_body` 按完整签名对象和 owner 匹配，不是凭交易摘要授予花费权。
- `consensus/native.rs::PreparedMessage` 已持有规范字节与摘要，同一消息的重试及多 peer 传播可以复用；`AssetTransactions` 与 `AssetBody` 的类型／context 不同，不能直接混用同一发送对象。
- `consensus/bodies.rs` 与 `bodies/successor.rs` 已持有原正文、精确父状态及执行票据；任何复用都不能把私有 Ready 变成已发布链头。

## B：原 APFL early／bind 能力是否适合迁接

- `consensus/transport.rs` 已有 `ApflEarlyBody` 与 `BindBody`；`consensus/controller/early_body.rs` 保存不可变 announcement、来源／轮次、认证结果和精确绑定。
- 当前类型是 V3／`ApflTransferBatch`，不是现成 AssetV2 引用入口；活动 `NativeLifecycle` 尚未接纳该可选消息。不能以“接口已存在”为由直接启用或复制旧 controller。
- 只有先证明重复完整正文传播确为主要成本，才评估沿原机制迁接 AssetV2；本交接不授权新增 wire、缓存、账户或发布通道。

## 第一项测量及选择条件

沿现有 `profile` span／有界事件，只选首高度和一个慢高度：对齐同一正文的入队、传输准入、接收、候选完成及 journal ACK，并记录相邻 `poll_io` 的间隔。

最小补充仅是在原 `NativeIo` 边界记录轮次间隔，以及为现有同步 prepare／decode／charge-encode 调用各加一个阶段耗时；不建立新诊断框架，不改变负载或确认终点。

- 若最长 I/O 空档与同步重复准备重合，先在原持有者中消除对应重复工作。
- 若 I/O 持续及时而重复完整正文传播占主导，再提出 B 的受限适配方案及反例。
- 逻辑入队字节不是物理网络流量：须分别记录实际分片发送、复制、重传；早发计数增加也不是提速证据。
- 保留同条件 CPU／GPU、慢样本、四方收齐终点；不承诺下一改动必然达到 12K。
