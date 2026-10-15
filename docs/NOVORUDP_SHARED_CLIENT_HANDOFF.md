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

- **完整 APFL 信箱接线还没有完成。** 此片传输测试的正文是 opaque binary，
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
