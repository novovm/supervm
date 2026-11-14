# C：满载记录、网络对象与状态查询的所有权审查

2026-11-14；基线 b55a4517；C 独立调查。**首次破坏指令仍未定位，C NOT READY。**

本轮改变调查策略：不再重复已通过的格式化/边界样例，先检验“满载淘汰导致网络
对象悬空，再由快照访问”的因果链。结论是：**所审查代码没有建立这条引用链，
不能再把 213,510 次淘汰作为该根因的支持证据。** 这不是证明整个节点内存安全。

未启动或附加节点、未构建/运行诊断目标、未运行 TTD、未访问原库或身份，未改生产
源码或二进制。已有失败转储和旧报告保留。本轮没有新增运行时“通过次数”。

## 1. 精确源码与核验方式

直接读取原 Git blob，而非把当前 main 或添加过诊断入口的临时源码当作原构建：

- 875：`875d3307a1fc2ca34b45464ac3bf51a03619adc2`。
- a254：`a25452f4c49be37cd984e650f2079642e6c273b3`。
- [只读索引脚本](collect-source-evidence.py)与[实际文件哈希/行号](source-evidence.json)。
  脚本执行退出码 0；12 个选定文件中 11 个源码字节相同。其输出是源码索引，
  不是安全检测、运行时证明或原故障复现。
- 唯一不同的选定文件为 `consensus/native_lifecycle/probe.rs`：诊断中继命令由
  单个 Hash 扩展为一至两跳 `Vec<Hash>`，增加长度/路径校验。实际 observation
  函数主体不变；未发现把网络正文指针带入 delivery record 的变化。

上述比较**不等于 EXE、机器码、布局、AOEM DLL 或全部依赖相同**。875 与 a254
故障仍按各自原 EXE/PDB/DLL 证据区分。所有下述行号以原 a254 为准。

## 2. 先分清四种所有者

| 容器/对象 | 容量与内容 | 回收与查询关系 |
|---|---|---|
| `profile::DELIVERY` | 2,048 条 `CriticalRecord`，标识、计数和时间 | 同一 Mutex 下淘汰/遍历；不保存正文借用 |
| `NativeTransferDiagnosticsV1::State.records` | 最多 256 条传输证据，120 秒期限 | 锁内过期/淘汰/clone；嵌套 Vec 拥有自己的数据 |
| 发送任务/生命周期队列 | `Arc<OutgoingMessage>`，内部拥有正文 Vec | 移除任务转移 Arc；其他强引用仍持有对象 |
| 重组器 | `BTreeMap<Key, Entry>`，Entry 拥有分片 Vec | 完成后移出 Entry、转移 chunks；过期时丢弃自己的 Entry |

原进度中的 delivery `evicted=213510` 和 transport `objects=245 /
evicted=1907 / expired=9739` 属于**不同诊断账目**。后者不是“245 个存活正文对象，
已释放 9739 个正文”的证明。不能把两种记录淘汰都解释为网络正文对象池复用。

## 3. 2,048 条环形记录：未发现可证明的悬空引用或手工越界

源：[profile.rs](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-node/src/native_pipeline/profile.rs#L51)。

1. `CriticalRecord`（51 行）包含整数、`Option<u64>`、`Option<[u8;32]>` 和
   `&'static str`。哈希按值保存；事件名静态存活。没有网络正文的借用、裸指针、
   自定义释放器或池对象句柄。
2. `CriticalBuffer`（64 行）使用标准 `VecDeque`。没有应用自写 head/tail
   算术。`push_latest`（89 行）在满容量时 `pop_front`，再调用 `push`；
   通过这些私有路径容量不超过 2,048。回绕由 VecDeque 管理，不由 seq 索引内存。
3. `delivery_event`（180 行）接收复制的哈希，取得同一 `DELIVERY` Mutex 后
   才构造并写入记录。竞争时记录丢失计数增加，不保存跨调用引用等待稍后写入。
4. `snapshot` 的 delivery 分支（586 行）在 guard 存活期间遍历、格式化、收集
   整个 JSON 数组。结果是拥有数据的 `Value`，迭代器与 record 引用不被返回。
   因此遵守此锁的生产者不能在这段遍历中同时淘汰当前记录。
5. `hash`（276 行）接收 `Option<[u8;32]>` 副本，逐字节格式化并收集为新 String。
   String 不借用网络分片内存。其同步 formatter 生命周期与此前汇编审查一致。

这里的 Relaxed 原子只用于观测计数/开关；记录的互斥由 Mutex 提供。序号饱和不是
内存下标回绕。没有据此修改计数、扩大容量、缩短保留或屏蔽格式化来绕开崩溃。

**边界：**安全 Rust 的所有权规则只描述所示代码在前提成立时的行为，不能排除
其他模块先前的未定义行为、已损坏的锁/容器/栈、编译结果问题或机器执行异常。

## 4. 真正的网络周转路径

### 4.1 UDP 暂存不被排队数据借用

源：[transport.rs](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-network/src/transport.rs#L11279)、
[native_ingress.rs](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-network/src/transport/native_ingress.rs#L241)。

`receive_packet_v1` 在锁内同步收包，使用 `scratch[..n].to_vec()` 复制到独立 Vec；
drain queue 类型为 `Arc<Mutex<VecDeque<(Vec<u8>, SocketAddr)>>>`（9607 行）。
出队转移拥有的 Vec，原 scratch 下一次复用不应改变已排队数据。ingress 的同步
FnMut 收包调用返回拥有的 Vec，不向另一个异步任务导出 scratch 借用。
`transport.rs` 显式禁止 unsafe，但这不构成所有底层依赖均无 unsafe 的结论。

### 4.2 发送：移除任务不等于把其他持有者的正文释放

源：[native.rs](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-node/src/native_pipeline/consensus/native.rs#L100)、
[object_link.rs](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-network/src/novorudp/object_link.rs#L551)、
[native_lifecycle.rs](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-node/src/native_pipeline/consensus/native_lifecycle.rs#L449)。

`prepare_kind` 编码产生 Vec，封装为 `Arc<OutgoingMessage>`。`PreparedMessage`
保留该 Arc，发送任务 clone 强引用；`OutgoingMessage` 自己拥有字节。发送完成时，
`process_poll` 移出 sender，`into_body` 转移 Arc 到完成报告，而非返回已释放裸指针。
生命周期读取完成报告、移除队列项后，仍持有移出的值，向 delivery recorder 只传
哈希/计数副本。过期/会话更换路径保留受所有权管理的对象供原规则处理。
最后强引用释放才可能销毁正文；所查路径未发现手工池复用正文地址。

### 4.3 接收：完成移交与过期丢弃互斥借用重组器

源：[fragments.rs](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-network/src/novorudp/fragments.rs#L158)、
[接收解码](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-node/src/native_pipeline/consensus/native.rs#L792)。

Entry 拥有 `Vec<Option<Vec<u8>>>`。`poll_complete_observed` 通过 `&mut self`
取出完成 Entry，核验哈希，将 chunks 移入 `CompletedMessage`。完成历史仅保留
键、长度和时间，不保存正文借用。同步 `hashed` 回调只提供身份/标识信息。
`expire(&mut self)` 淘汰自己仍拥有的未完成 Entry；没有与已移交 chunks 共用
同一所有权。接收端合并为新 Vec、解码为拥有数据的 Message，再移入 Arc；记录
`received` 时仅复制 peer/fragment 标识（native_lifecycle.rs 802–810 行）。

### 4.4 传输证据是元数据快照

源：[diagnostics.rs](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-network/src/novorudp/diagnostics.rs#L612)。

`State::record` 锁内先过期，再搜索索引；满 256 时仅按原条件移除终态项，否则
计 omitted 并返回。新建项后取得有效索引，不把删前索引用于新的异步访问。
返回的 `&mut` 受 State 借用约束，调用者在 guard 内更新。
`snapshot_transfer` 克隆一项；`snapshot_at` 锁内 expire、clone snapshot 和记录。
嵌套反馈窗口 Vec 被拥有式克隆，不含正文借用或指向原环槽的指针。

## 5. 周期查询与网络的共同执行过程

源：[原节点循环](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-node/src/bin/novovm-node.rs#L42601)、
[原 status](https://github.com/novovm/supervm/blob/a25452f4c49be37cd984e650f2079642e6c273b3/crates/novovm-node/src/native_pipeline/service/rpc.rs#L1326)。

同一 native tick owner 顺序执行：

`network.step(&mut controller) → network observation（到期时） → product.poll
→ product.status（到期时） → JSON 编码 → 写进度文件`。

这不是“网络线程持有正在回收的 record，状态线程借用该 record”的已证实模型。
其他 IO/compute/库线程确实存在，但存在线程不等于它们持有当前 record 的可写别名。
`status` 1374 行完成 `profile::snapshot()` 后，1375 行才调用
`network_probe_observation()`；该后续传输快照的 expire 不能在同一调用中先于
未完成的 profile 遍历执行。不同诊断账目之间不保证同一时点，不能拼成原子历史。

Watch-Relay 读取已写出的进度文件，不向上述 formatter 递交对象。观察窗口和崩溃
相关值得保留，但不是观察器向节点写坏内存的证明。旧 WER 等待栈也不能还原故障前
线程调度史。

## 6. 与原始 DMP 的直接关联及缺口

本轮复核的是此前保留的分析结果与源码，不声称重新采集或重新分析了全部 DMP：
[原对象/寄存器证据](../C-original-dmp-phase2-20261113/README.md)、
[精确 a254 符号限制](../C-symbolized-original-20261114/README.md)、
[原最后进度与堆页缺口](../C-real-load-phase6-20261114/README.md)。

| 现场 | 与本轮路径能连接到哪里 | 不能据此推出什么 |
|---|---|---|
| PID3228 | status 格式化；合法局部 String/Formatter 与最终 writer 接收地址不一致；writer 地址落在保存寄存器的栈槽 | 不是网络正文已释放地址的证据；没有入口/尾调用时序，不能确定首次错位指令 |
| PID37796 | 最终 delivery 迭代器 1340+708 项，record 地址可计算；局部 String 与 writer 不一致；复制源异常 | MiniDump 缺该 record 堆页；此前 progress 1530+518 不是故障瞬间；不能命名当时记录，更不能证明淘汰写坏栈 |
| PID31296 | 寄存器恢复指令处帧关系异常 | 上游调用未恢复，不能强行归入同一环形缓冲区或同一释放路径 |

**已证实的是最终现场参数/帧异常；未证实 UAF、越界写、竞争、ABI 违反或具体模块
首次破坏。** 环容量满和淘汰数大是运行条件，不是因果证据。源码相同也不证明三次
崩溃同根。

## 7. 执行停止条件：收回无证据的因果链，不再扩充正常样例

本轮没有找到首次破坏位置，按用户设定的停止条件，**停止以增加满载快照次数来
验证该根因的路线**。此前 Phase6 末尾建议追加 owner-loop 普通样例，不再作为
默认下一项。仅复制更多记录/反复 wrap 仍不能重现旧堆历史和真实失败执行。

当前没有可提交的最小生产修复。不修改 String/IndexMap/memcpy，不取消状态观测、
扩容或加空指针检查来掩盖症状；未发现违反所有权的具体源码位置。

进一步动态取证必须先满足新的证据门槛，而非再启动节点等待：

1. 针对原故障二进制确定可核验的调用边界和对象存活关系，不能拿最终保存的
   寄存器值代替历史入口值，也不能把最近 PDB 符号当精确源码。
2. 一次失败必须同时留有正确 Formatter 对象、writer 实际参数、相关栈/代码页与
   目标 record 堆页；任何“首次”结论必须有前后执行证据，完整 DMP 本身仍是快照。
3. 若只能取得又一个最终崩溃位置，就明确无法从当前证据反推出首条写坏指令；
   重新评估调用现场/构建/外部破坏等候选原因，而非继续预设环回收是根因。

以上是后续取证的准入条件，不是已实施方案或本轮重启授权。本轮不开发新的录制工具。
硬件、编译器、FFI 等均未证实或排除，不做根因概率排序。

## 8. Git 回传与验收状态

本轮开始及提交前 fetch 到的 origin/main 为 b55a4517，没有发现更新的 A 回执；
最近明确回执 0470f343 只确认此前 e1e3a999，不能覆盖本轮。
本报告通过 Git 通知 A；**推送不等于 A 签收**，不要求 A 代查或 D 操作。
根因未解决、无生产修复、未启动节点、未恢复双中继。
