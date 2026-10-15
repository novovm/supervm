# 原生客户端共享传输交接（2026-10-16）

## 原对象 owner 的异步载体接线（2026-10-16）

`novorudp::carrier_io::NativeCarrierPump::attach` 消费结构化 Finished 后的原通道，
把实际 CarrierStream 接入原 NativeObjectLink。驱动方在同一 owner 内轮询
`link.poll` 与 `pump.progress`；短暂取消 progress 等待不会丢失半帧 future，
销毁 pump 则终止载体。排队不算写成功，待写帧及配额保留到完成或明确取消。
正文/反馈公平交替，取消中的未知写入不能重用旧流。原UDP同步路径保持。
本机实际QUIC双向两批对象和回归通过，详见生产台账顶部。

原attach保留有限IO契约；新增attach_continuous_v7在现有持续owner内选择原生
分帧：首字节前空闲等待仍检查撤销，首字节后固定30秒完成整帧，不能逐字节续时。
原NativeMailbox业务期限不变。有限scope、过期握手和已选择旧协议的流不能升级；
半帧取消/失败终止载体。没有新增worker、SDK、信箱或调度器。

重要边界：BoundSecureChannel只证明Finished及实际exporter绑定，本身不携带
structured codec或NVMBOX07选择证明；正式入口须显式选择服务并调用结构化握手。
当前实际QUIC对象收发和TLS分帧测试不是正式信箱或Android接线验收。
下一片将product_rendezvous_native现有jobs/active/pending执行体复用于UDP与
carrier readiness；复用入站已取得的连接permit，避免同一额度重复申请死锁。
同时增加明确签名的原生carrier能力，正常声明换代须保留它，旧Iroh记录不能
被静默解释为NVMBOX07。之后接原Application owner并重新验收两台手机。

## 结构化载体认证接线（2026-10-16）

`bound_session::authenticate_structured_bound_session_observed` 和
`authenticate_structured_service_client_observed` 进入同一个既有认证状态机，
仅显式选择原生编码；Offer/Response 复用 APFL 批格式（NVBOFR01/NVBRES01），
Finished 复用严格 NVRLY002 Data 二进制封装，不自动探测或回退 JSON。
原节点签名、公钥绑定、真实 TLS exporter、重放缓存及双方 Finished 不变；
本机 Finished 实际写出且对端 Finished 验证后才返回 channel。
旧入口供现有调用者保留，原生入口不能把旧入口当作失败后备。

编码器不是新的身份或权限系统，解码成功不代表认证成功。握手期限仅约束
建立过程，不成为健康会话的周期终止条件。开放服务继续证明密钥持有，无需
固定用户名单；接收者信箱能力、资源边界和撤销检查仍由原业务执行。

尚未交付：原生 IO 与 Iroh owner 的实际收发连接、明确服务版本的加密绑定及
原生节点身份对应、旧流取消后的排队数据清理、手机跨 NAT 与独立副本验证。
仅完成本节认证不能直接放开 NativeRoute::Carrier；原生帧必须继续绑定到
这个已认证流及其 channel，不能改走裸流、伪造 SocketAddr 或本地 UDP 代理。

## 最新分工：手机归 NOVOCHAT 设备，主链归七月恢复设备

用户本轮明确：iPhone/Android 复用同一 NovoRUDP/APFL 协议与 Rust 核心，但
平台封装/构建、NOVOCHAT Application owner 装配、通知/后台/换网及手机实测
交给另一台开发 NOVOCHAT 的设备。本机不接手手机功能，不另造移动 SDK。
两端都沿本仓共享实现，不复制成独立分叉；不能将桌面通过宣称为 iOS/Android 通过。

本机完成本节这笔在途通用重连修复后，回到 **NOV 新创世 → 原生结构化签名交易
→ AOEM 统一代数执行 → 共识耐久发布 → 原生余额/回执查询 → 重启恢复**。
同时验证 NovoRUDP/APFL 真实结构批路径的 TPS/P95/P99：保持真实签名、经济规则、
冲突条件、验证者数量与耐久终点，单列业务失败；检查中途是否又出现 JSON、逐笔
展开或 Host 业务接力。七月高吞吐是复用/对照来源，不是现行最终确认成绩。
不再以移动配套、网络组件数字或旧 HTTP 观察代替主链交付。

### 本次共享接口增量：显式客户端重连 v1

在既有 open-client endpoint 上显式调用 `enable_client_reconnect_v1()`，再装配
原 `NativeObjectLink`。默认及固定验证者 peer 仍不接受此替换。客户端使用原
`connect_client` 重新握手；服务端保留旧连接，直到新源发来有效新会话 AEAD 帧，
才接替原 peer 索引。只有签名 offer 不会断开旧会话。每 peer 最多一个有期限的
待确认握手，重试不延长；未完成/过期不影响旧连接。无握手或 AEAD 格式变更。

同一 poll 返回 `session_changes`，先处理代次事件再消费应用消息；旧分片、DONE
缓存及待发反馈不能进入新代次。旧出站对象以 `ObjectSendState::SessionReplaced`
连同原 Arc 交还，表示结果未知，不是业务失败或回滚。完成仍按 poll quantum
有界交付，额度交还后可明确重试；不要自动重放整个已执行的信箱批次。
`session_validity(peer)` 是只读本地代次句柄，替换/释放 link 后永久失效；
共享 mailbox adapter 已将它接入在途任务检查，原业务权限检查仍必须提供。
已经进入存储提交的操作不会被撤销；按原 Object ID/签名回执恢复未知结果。
已合并 NOVOCHAT 设备的 `try_send_until`，不延长业务期限或恢复失败槽。

移动 owner 应依据该事件退休旧适配器/任务，真实新会话再 attach；旧请求 context
不得重标新 session。这里只实现并验证通用 socket 生命周期，不替手机实现发现、
选路、重试决策或 NAT 穿透。新身份槽仍有原上限，尚无闲置身份回收；已持有输出的
应用必须保留自己的总预算。能力缓冲全面安全擦除、平台构建/实机尚未验收。

本片把七月恢复线 `fb8fc69efbbe01177a24f82ca44b6d1ab569a99e` 已验证的
会话、对象传输与反例迁入共享 `main` 的原 `novovm-network`，不是另建 SDK。
新增显式有界、无需预置对端的应用监听器。原帧、加密、窗口修复、pacing、
身份/源地址绑定、重放检查不改协议。无新依赖，无金融、执行或共识修改。

## 消费者现在可以调用什么

继续从现有 SUPERVM 源码依赖 `novovm-network`，不复制传输源码到 NOVOCHAT。
以下接口无需 `iroh-transport`，也不会连接 HTTP、Iroh 或 Relay：

| 原 crate 内接口 | 用途 |
| --- | --- |
| `novorudp::endpoint::NativeEndpoint::listen_clients` | 原 socket owner 的显式开放密钥持有认证；指定客户端上限和允许的 stream |
| `NativeEndpoint::connect_client` | 复用本机身份/用途密钥，绑定目标节点公钥；不要求验证者身份 |
| `novorudp::object_link::NativeObjectLink::new` | 在同一 socket/session 上装配有界 lane，不另开调度器 |
| `novorudp::fragments::OutgoingMessage::with_profile` | 一次准备结构正文，使用 `FragmentProfile::NativeUdpV2` |
| `NativeObjectLink::try_send / poll / poll_complete` | 非阻塞发送、进度/修复、取回原始二进制 chunks |

完整可编译的公共 API 消费例在
[native_application_transport.rs](../crates/novovm-network/tests/native_application_transport.rs)。
它是外部 integration test，不访问 private 实现、不持有验证者密钥，复用同一
会话完成三轮 12,000 字节双向分片传输，包括超过握手有效期后的连续收发。
测试 stream/domain 是测试值，不能当作已分配的生产信箱协议。

`try_send=false` 表示未接纳，调用者保留同一 immutable handle；
`Delivered` 只是传输对象完成，不能替代信箱持久 `StoreReceipt`、设备回执或
链最终性。`Expired` 显式归还原对象，由原业务 owner 决定重试，不能静默丢工作。
按 node-wide/per-peer byte/message 上限消费；不能建无限重试队列或每消息握手。

空 pinned 列表仍拒绝；只有 `listen_clients` / `allow_clients` 显式开放策略才能
接受未知客户端，且签名/目标/时间/重放检查通过后才占槽。认证不是付费资格、
全网准入或既有链上 UCA 绑定证明；保留原 UCA 及用途密钥，不新造账户。

## 尚未交付，不能冒称已经可上线

- **完整 APFL 信箱接线还没有完成。** 初片传输测试的正文是 opaque binary，
  不把任意 bytes、二进制帧或 JSON 包裹当作 APFL 结构执行。
- 原 `MailboxClientSessionV2` 仍用 CarrierStream、原协议和 JSON 正文；未改动
  正在运行的手机、信箱服务、数据或策略。旧 `NVAPFLV3` 是转账专用，聊天不能
  冒用其经济语义，也不需要逐消息链共识。
- 下一项在此共享 crate 中接原信箱 owner：明确通用结构 envelope/schema 的
  版本和界限，保留 capability、Message ID、TTL、E2EE 密文、StoreReceipt、
  消费/重试语义；与客户端协调显式版本，不在旧 selector 下换字节，不静默降级。
  不能让 NOVOCHAT 自行复制另一套网络/SDK 或修改财务状态。
- 当前客户端槽位 append-only，满额明确拒绝；同 key 换源重连、槽回收/代次隔离
  尚待接入。不得为了移动网络切换清空身份/日志、沿用旧会话 nonce，或假装已支持。
- 原 listener 只解决已知可达 UDP 地址；发现/NAT/Relay 策略与 Android 网络生命周期
  继续由原 owner 适配，不能把本机 UDP 当公网移动可达性。
- Android ARM64 linking、现有 APK 接入、双手机运行均 **NOT EXECUTED**。
  Windows Rust 公共 API 测试不是 Android SDK/主链/TPS/生产签收。

## 本机验证命令

在仓库根目录运行（无需固定盘符）：

```powershell
cargo test --locked -p novovm-network --lib novorudp -- --test-threads=1
cargo test --locked -p novovm-network --lib product_overlay -- --test-threads=1
cargo test --locked -p novovm-network --test native_application_transport -- --test-threads=1
cargo clippy --locked -p novovm-network --lib -- -D warnings
```

主体恢复仍按原路线执行。本片仅把共享、无金融依赖的传输组件提前发布给等待中的
消费者，不能理解为改回“以十月主链为主体”。恢复现场的同名组件应保持与这份
共享实现一致；主链查询草稿不属于本次交付。

## 2026-10-16 增量：结构信箱编码与原 owner 已连接（非完整移动 SDK）

本节更新上述“schema/owner 待接”项，不改其他未完成项。公共接口位于
`novovm-network::mailbox_structured`，当前随原 `MailboxServiceV1` 的
`iroh-transport` feature 编译；**feature 依赖不表示新路径运行时使用 Iroh**。
没有打开任何 HTTP/Iroh/Relay listener，没有改 NVMBOX01–06 的字节或协商。

| 公共接口 | 契约 |
| --- | --- |
| `encode_requests` / `RequestBatch::decode` | 原操作类型的有界 APFL 列编码与借用视图；全部行先验证，后执行 |
| `encode_replies` / `decode_replies` | 原回复与 StoreReceipt 无损映射；context/数量匹配不代替回执验签 |
| `execute_local_batch` | 原 MailboxService 的适配，不是远程准入；调用者在每次 await 前后提供实际 authority 检查 |

NVMBOX07 是新 schema/stream，不将旧 BeginSession/Continuous 票据换标签使用。
每批 1–16 行、8 列、单字段最多 65,536 字节、编码及展开各最多 1,100,000 字节；
Watch 最多4个信箱、10秒。上下文必须非零，远程驱动仍须将它绑定到认证会话和
唯一请求，落实重放与响应关联；一个调用者自填的 context 本身不提供这些保证。
数据层使用 `novovm-protocol::apfl_batch` 的 NVAPFLC1/v1，header/数值小端，
magic/schema 固定，保留字为零；原 NVAPFLV3 转账格式不变。

编码复用从原 Transfer 提取的同一 `Column` 实现：共享列只编码一次、重复值用
首见顺序字典与索引、独立密文无损保留。解码在行数/长度/展开预算内验证规范性，
值借用原缓冲；调用原 owned 信箱接口时才物化该行密文。**不是全程零拷贝、不是
加密密文压缩算法，也不是 AOEM 交易执行计划。** 生产 codec/adapter 不经过 JSON；
测试中的 JSON 只作旧类型的逐字段等价 oracle。

批内操作有序，但不是原子事务；取消或 authority 丢失时，之前存储可能已提交。
不得称回滚或盲重试整个批次。沿原 Object ID/Consume/StoreReceipt 处理未知结果；
Registered、传输 DONE 和 Stored 含义不同，Stored 仍须验证原节点签名及密文摘要。
原 node-wide 限速、能力、TTL、贡献开关、资源可用性、托管责任和消费墓碑均复用。

公共 API 测试新增 `structured_mailbox_over_authenticated_udp_returns_verified_custody`：
本机实际 UDP、同一原生认证会话、结构注册/9KB密文存储/读取，服务端调用同一个
MailboxService，客户端独立验签及篡改拒绝。单元测试另验重启OFF保留托管、消费
墓碑、坏能力、整批预检、提交后权限失效与原ID重试。这不是生产 server/client
驱动：移动重连、活动身份/撤销策略接合、请求重放与响应关联、transport保留缓冲
中的能力擦除、Android ARM64/实机仍未交付。现有手机不要据此切换协议。

```powershell
cargo test --locked -p novovm-protocol apfl_batch
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --lib mailbox_structured -- --test-threads=1
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --test native_application_transport -- --test-threads=1
```

## 2026-10-16 增量：原生认证会话上的实际 SDK 调用链

`mailbox_structured::native_session` 新增可调用适配，不新建 socket 或后台网络运行时。
可编译消费例见 [native_mailbox_session.rs](../crates/novovm-network/tests/native_mailbox_session.rs)。
它替代上一节夹具中手工拼 context、手工匹配回复的做法；仍未接入部署中的手机 owner。

1. 使用原 `NativeEndpoint` 建立并验证同一身份会话；配置 NVMBOX07 `STREAM`、
   `native_session::domain()` 与显式 node-wide/per-peer 收发预算。
2. 认证建立后，`NativeMailboxClient::attach` 校验节点 pin；
   `NativeMailboxServer::attach` 取得同一 peer 的服务接合。客户端和服务角色分别
   认领，同一个 L4 peer 可以提供和消费服务，不是排他客户端/开发者许可证。
3. 客户端 `begin(slot, requests, now)` 只准备有界批；`try_send` false 保留同一待发
   对象。服务端从原 link 的 NVMBOX07 完整消息调用 `admit`，再异步执行
   `MailboxWork::execute`；原 socket owner 在等待存储/Watch时继续轮询 link。
4. `MailboxResponse::try_send` false 时保留原 handle，有额度再试；客户端
   `receive` 校验完整回复、请求操作、数量、摘要及 Put 对应的原 StoreReceipt 签名。
   `transport_complete` 只处理原传输完成/过期，不制造业务成功。

每方向每会话最多4个逻辑请求槽、每槽1个批次，每批原有1–16行/1.1MB上限。
槽各自持有u64序号；context以固定域、认证session、请求者/服务者身份、槽和序号
构造。方向分离，旧会话、旧序号和错误操作回复不能充当当前结果。
`recipient_role` 只作同一lane的请求/回复路由提示，必须继续交给对应适配器完整验证。
原link按peer/stream/role保留有界owner记录，不能丢掉适配器再从序号1重来。
没有每消息握手、链RPC、全网许可或第二账户；本地身份持有仍不是链上UCA绑定证明。

Watch与Put可以占不同槽并行，批内操作仍有序且非原子。30秒是**一次有界业务操作**
的期限，不是监听器寿命或网络许可证；空闲健康会话不按这个时间拆除。
超时、撤销、取消和未知结果锁住受影响槽，不清空日志或静默重试。服务签名key必须
是实际本地传输身份，不能在一个节点会话里发另一个节点的持久回执。
丢回复后按原Object ID明确重试可以读回已提交的回执；不会重新算成未提交。

应用集成仍必须提供原活跃身份/撤销/路径授权检查，持续驱动同一link，并限定节点级
任务与回复队列，不将完成对象转移后遗忘资源预算。该适配没有另建调度器或数据库。
**未完成：**同key换源重连、会话代次回收、transport中全部能力缓冲安全擦除、现有
服务发现/手机owner的正式装配、Android ARM64/实机验证。不得启用旧HTTP/JSON或
Iroh fallback来把这些项冒充原生验收；已有在用协议也没有被本片替换。

```powershell
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --test native_mailbox_session -- --test-threads=1
```

## 2026-10-16 消费端合并：只保留共享native_session

另一机器af77af16与本机未推送faf108c5发生请求状态实现重叠。当前仅保留
`native_session`：节点签名回执与请求类型核验、四个并发槽、原link唯一owner
认领均沿共享实现；本机另一份session模块及重复公钥getter已移除，不形成两套SDK。
保留本机`NativeObjectLink::try_send_until`并接到共享请求/回复发送：原对象sender
取业务剩余期限与transport TTL较小者，重入队不延长原期限。没有延长30秒操作
期限或把它变成连接寿命；原认证会话与临时操作仍分开。

移动换源重连/故障槽恢复、在途撤销定向取消、能力缓冲全面擦除和正式Application
接线仍待完成。四个请求槽是当前有界并发，不是聊天历史或累计发送条数上限。

## 2026-10-16 消费端补齐：原发送队列取消传播

`NativeObjectLink::try_send_cancellable_until`在原有队列中保留本地单向取消信号，
`cancel_send`仅接受原Arc对象进行定向取消。`Cancelled`与`SessionReplaced`、
`Expired`都是未知业务结果，必须保留原业务ID及恢复信息。它不是全网许可或新的
认证；不会撤回已经进入内核的包、远端已提交密文或已报告的Delivered。

共享native_session已直接接入：client Pending退出自动通知原发送队列；server
invalidate/drop停止该owner已排队回复。单个response授权/期限检查失败仅取消
该回复并锁住handle，不能通过再次返回Ok复活，也不会取消其他槽。原poll有界
回收额度，原owner需继续poll；远端未完分片保持原TTL与额度，不凭取消删除托管。
所有路径仍使用同一个endpoint、原加密与序号、防重放和StoreReceipt验证。

已通过桌面实际UDP和负向验证。原生平台接线、可达性/手机延迟、全能力缓冲擦除
仍未签收；取消接口不能冒充这些完成，也不支持用旧HTTP/JSON后备填补缺口。

## 2026-10-16 NOVOCHAT 消费方补齐原生信箱目录

`product_service_lease::NativeMailboxEndpointV7` 明确声明既有 NVMBOX07 服务。
通过原 `ServiceLeaseBookV1` 导入/验签/持久化后，调用
`native_mailbox_candidates_v7` 获取绑定公钥与新鲜候选；新拨号调用
`check_native_mailbox_route_v7`，健康会话调用 `check_continuous_native_peer_v7`，
并继续检查 NativeObjectLink 的实际身份/代次和业务能力。这些目录检查本身
不能证明连接在线、安全会话成立或匿名，也不会替代资源准入。

原 `ServiceLeaseHostV1::rotate_native_mailbox_v7` 发布或移除原生入口，签名序号
和撤销沿原持久owner。纯原生记录不再强制携带Iroh地址；混合记录正常轮换不丢
原生候选，旧载体API不把原生地址当自己的地址。载体Udp标签本身不等于信箱版本。

Windows真实UDP8项已从签名目录取得地址再调用原共享SDK；原TLS/QUIC宿主4项
继续通过。实际Android原生监听/拨号、自动路由及蜂窝NAT仍待原平台owner接线；
不把本片当作移动激活。详见生产台账同日目录记录，保留旧一秒清理计时失败。


## 2026-10-16 Node owner native mailbox listener

`product_rendezvous_owned::NativeMailboxListenerV7` now borrows the original
NativeObjectLink/NVMBOX07 codec/session and MailboxService. The same node owner
can attach this listener beside its existing carrier, or use
`run_self_owned_native_mailbox_v7` without constructing an Iroh endpoint.
The caller supplies the protected node key, same stop Arc, same persistent
MailboxService, and original durable ServiceLeaseHost activation/publication.
No new account, schema, JSON compatibility service or financial execution path.

The socket clone only peeks for readiness; NativeEndpoint alone receives/sends.
Concurrent borrowed work and retained responses are jointly bounded. Waiting
Watch is classified only after full APFL validation, limited to two globally
and one per peer; overload returns the existing typed Quota response without
executing rows or resetting sequences. Put/Fetch/Consume retain room for progress.
Same-key session replacement retires old work and responses; committed custody
survives cancellation and must be reconciled by original object identity.

Not yet a deployment release: cumulative native peer identities remain capped
at 64 without reclamation; signed bytes/min is not yet enforced at the original
wire boundary. Shared request quota and object memory bounds do not substitute
for that budget. Windows protected runtime settings, Android Application driver,
real network adaptation and latency remain pending. Keep current installed
carrier unchanged until these hooks are integrated and measured.

## 2026-10-16 Native UDP payload budget on the original owner

`NativeEndpoint::set_wire_budget_v1(bytes_per_minute, Instant)` may be set once,
before the first poll/send. The optional original-SDK budget covers admitted
UDP payload bytes plus successful UDP sends across initial/reconnect handshakes,
data, ACK and repair. Standalone original APIs retain their existing behavior.
The native mailbox listener always installs its explicit resource budget;
`bind_with_wire_budget` supplies it and `bind` defaults to 64 MiB/minute.
`NativeMailboxEndpointV7` preserves this same signed value, rather than losing
it when converting a directory record into a typed route. This is a node-wide
resource ceiling, not per-user admission, billing or a network license.

Accounting uses 61 fixed second buckets and monotonic time, conservatively
covering each rolling 60 seconds. It does not allocate or lock per packet.
A full UDP receive buffer counts oversized datagrams accurately before rejection;
unwanted incoming bytes already reached the socket and cannot be prevented by
application admission. Usage separately exposes received/admitted/sent totals,
receive drops and deferred sends; IP/link headers are not included. Successful
send accounting is not a storage or device delivery receipt. Quota backpressure
does not advance object transmission/repair success or recreate the session.

Resource limits are 2400 bytes through 1 GiB per minute; the low bound allows
only very limited traffic and is not a useful general chat capacity promise.
Changing a signed route does not reset the running socket budget; reconnects
within that owner cannot clear it. Process restart starts a new local window;
this is not durable billing or a global abuse defense. The cumulative 64-peer
identity capacity still needs lifecycle work before an open-network deployment.
Mobile driver integration and real Android native-path latency remain pending.

## 2026-10-16 Shared async mailbox request handles

`mailbox_structured::native_requests::{NativeMailboxDriver, NativeMailboxHandle}`
attach to ONE existing authenticated NativeObjectLink peer. The original owner
must call progress, poll that same link, dispatch transport completions and feed
mailbox replies; wake()/next_deadline() fit its existing IO selection. The adapter
creates no socket/runtime/thread/retry controller. Use the same Application owner,
not another app SDK or task pump.

request() reserves one of four original slots synchronously, including completed
results until consumed. The deadline starts before queueing (at most the original
30-second operation bound); it does not bound the healthy connection lifetime.
Watch and ordinary operations use independent slots. Do not interpret four slots
as a lifetime message limit, or require all four free while Watch is active.

Future cancellation directly marks the original link's retained-send token;
unknown outcomes retain original business identity/TTL and require reconciliation.
A failed active slot stays retired until a new legitimate session, without sequence
reset. Validated late replies are discarded using at most four retired contexts.
Protocol/Authority errors remain typed and cannot become Stopped during teardown.
Transport completion alone never fulfills a business request or device receipt.

Real Windows UDP tests cover concurrent Watch/Put, continuous Fetch, queued expiry,
capacity, active cancellation before/after partial send and other-slot progress.
The production Application/native candidate wiring and Android cross-network
latency are still pending. Test-only local IO pumping is not a second production
scheduler. Current installed phones/node remain unchanged.


## 2026-10-16 增量：原生单 owner 的类型化收发边界

输入 dcc6d1b6，NOVOCHAT335e9d1。NativeEndpoint 内部由直接 UdpSocket
改为同一 owner 的 NativeDatagramIo；原 bind/connect/listen/poll_with_receiver
公开 UDP 入口继续保留。握手、数据、修复和 ACK 都通过同一发送边界及原字节
预算；NativeObjectLink 的调度、重传、取消、会话代次和对象完成语义不变。
没有新线程、代理、SDK 或聊天协议。新 listen_clients_with_io 与
connect_clients_with_io 可接已有平台 IO owner；实现必须有界、非阻塞，提供
完整包和实际来源。WouldBlock 不推进发送；部分写入报告错误，不伪装整包成功。

NativeRoute 将真实 UDP 地址与载体身份分开；VerifiedCarrierRoute 只能由
已有 VerifiedTransportBinding 的本地认证结果派生，不用假的 SocketAddr。
它仅表示本地来源归属：原 NVHSHK01 没有签 TLS exporter，因此当前 Carrier
构造握手/业务会话仍明确拒绝，绝不能把类型化路由当作载体绑定已完成。
下一步复用原 BoundHandshake/Finished 语义并补结构化编码，再由原 Iroh
owner 驱动原生对象流；原 JSON 握手和旧 NVMBOX 不是静默后备。
实际 adapter 还须在部分流写入失败后退役该流、清理旧代次待发内容，并将
节点签名身份与实际载体身份分别验证。未完成这些步骤前不切换现场节点/手机。

上游4d0c16a2..dcc6d1b6另有原主链tick接线与native_query变化；NOVOCHAT
没有调用新增UdpTransport ingress或查询入口。静态核对未见手机调用链冲突，
不由此声称Android运行或主链吞吐已通过。构建及回归结果见生产台账最新条目。
