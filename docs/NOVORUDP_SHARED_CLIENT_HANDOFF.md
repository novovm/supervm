# 原生客户端共享传输交接（2026-10-16）

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
