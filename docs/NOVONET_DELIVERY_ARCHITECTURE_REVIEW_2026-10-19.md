# NOVONET 交付架构对照：持续消息、临时寄存、对象传输

核查日期：2026-10-19。源码基线：SUPERVM `b3d335fc`、NOVOCHAT `4f084d8` / Android v77；通讯实现仍为 `07bed362`。本轮是官方资料、实际源码和已有实测的交叉审查，不是新版手机性能验收。没有替换手机安装、修改消息数据库或引入外部框架。

## 决定

保留既有身份、Application owner、持续认证载体、原生 APFL 信箱和 NativeObjectLink。三项职责应分清，但不建设三个网络：

| 职责 | 使用现有能力 | 成功的含义 |
| --- | --- | --- |
| 在线交付 | 合格的持续连接；已有 L4/节点路径与事件驱动接收 | 收件设备持久保存并返回有效 DeviceReceipt |
| 临时寄存 | 接收设备信箱、TTL、能力、StoreReceipt、Consume | 节点承担约定保管义务；不等同设备已收到 |
| 大对象 | 原生对象流、缺块修复、完整性验证；离线时对象级寄存 | 对象完整、必要寄存已持久化、最终设备确认分别判断 |

“在线必须绕过信箱才能快”不是成熟系统的共同规则。已有合格设备路径可减少中转，但隐私、离线保障和真实路径成本决定选择。先消除现有链中的多余等待，不为这个判断另建实时协议。健康会话检查、资源能力验证与完整连接握手也必须区分。

## 第一方资料对照

以下是有针对性的跨项目检索，不声称穷尽互联网或找到适用于所有网络的唯一最优方案。网页按本次访问内容，历史规范不当作所有当前客户端的部署证明。借鉴机制，不移植账号、运营准入或整套运行时。

| 来源 | 可采用的经验 | 不能照搬或误读 |
| --- | --- | --- |
| [SimpleX SMP v20](https://github.com/simplex-chat/simplexmq/blob/stable/protocol/simplex-messaging.md) | 持续订阅队列，消息出现即交付，设备保存后 ACK，按队列隔离标识 | 在线仍可经队列；它甚至要求一条确认后继续该队列下一条，不能把它当成无限并发范本。命令验证不等于每条重建连接。 |
| [SimpleX XFTP v3](https://github.com/simplex-chat/simplexmq/blob/stable/protocol/xftp.md) | 文件与小消息分工；加密分块、文件描述、多个位置与缺块恢复 | 使用 HTTP/2 + TLS 不代表我们改走 HTTP/JSON。文档说明当前客户端不用 FACK、依赖期限清理，不能笼统宣称所有文件收到即删。 |
| [Signal Sesame](https://signal.org/docs/specifications/sesame/) | 每设备异步交付、会话复用，处理重复、延迟、乱序与状态恢复 | 是会话算法模型，不是 Signal 当前全部传输/存储实现，也不是无需持久队列的证据。 |
| [Signal sealed sender](https://signal.org/blog/sealed-sender/) | 保护消息外层的身份元数据，内容加密与发送者元数据分开考虑 | 该历史设计含服务签发身份凭据，不适合复制为 NOVONET 全网许可；也不隐藏全部网络元数据。 |
| [Matrix client/server sync](https://spec.matrix.org/latest/client-server-api/#syncing) | 游标增量同步；等待中的请求收到新事件立即返回 | 长轮询不是固定间隔轮询；不复制 homeserver 房间状态/历史联邦，也不据 HTTP 名称推断延迟。 |
| [Briar](https://www.briarproject.org/how-it-works/) / [Mailbox](https://www.briarproject.org/mailbox/) | 可用本地链路与互联网路径互补；双方不同时在线时仍需可达寄存设备 | 不能保证所有设备关机或全部路径断开时仍可即时投递。 |
| [BitTorrent BEP 3](https://www.bittorrent.org/beps/bep_0003.html) | 多个块请求在途，尾部选择性重复并取消多余请求 | 不为一张小图构造公开 swarm，不向每个节点全量重复传输。 |
| [libtorrent settings](https://www.libtorrent.org/reference-Settings.html) | 在途队列按实测速率调整，读盘预取，磁盘背压有界 | 队列太小会限速，过大浪费资源；它的默认数量/超时不直接套入手机。 |
| [Iroh blobs](https://www.iroh.computer/proto) / [range request API](https://www.iroh.computer/blog/iroh-blobs-0-90-changes) | 可恢复、可验证对象；范围和批请求，减少无用复制 | 参考其接口边界，继续使用已有 NOVOVM 对象通道；不再加第二套 blob store。 |
| [QUIC RFC 9000](https://www.rfc-editor.org/rfc/rfc9000.html#section-2) | 一个连接多流、独立流进度与流控 | 同一个有序流仍可能排队；单流包裹多个业务槽不自动等于独立 QUIC 流，流控不能当作人工限速删除。 |
| [SQLite WAL](https://www.sqlite.org/wal.html) / [synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous) | 合并事务、合理读写和 checkpoint 可减少开销 | WAL+NORMAL 在断电时可能回退已提交事务，不能用它冒充已有持久寄存承诺。当前实现是 DELETE+EXTRA，不能按 WAL 假设分析。 |

## 当前源码：哪些参考已过时

源码索引：[NOVONET_REUSE_INDEX](NOVONET_REUSE_INDEX.md)。

- v77 的 1,600,067 字节、49 块图片已是一个有界对象寄存，不是 49 次信箱寄存。`NativeObjectLink → NativeCarrierPump → CarrierStream` 已接通；大文件仍按有界对象段处理，不是无限大文件一次读入内存。
- [app_mailbox_fast.rs](../../NOVOCHAT/crates/chat-core/src/app_mailbox_fast.rs) 的 `supports_bulk` / `receive_request` 将已协商的 Fetch/WatchFetch 转到 bulk 版本；[native_session.rs](../crates/novovm-network/src/mailbox_structured/native_session.rs) → [mailbox_structured.rs](../crates/novovm-network/src/mailbox_structured.rs) → `request_local_v1` 调共同服务。`mailbox_rpc.rs` 里保留的 JSON helper 不是这条原生路径。
- `Binding::check` 只检查会话有效性及身份绑定。不是每请求重新握手。[app_mailbox.rs](../../NOVOCHAT/crates/chat-core/src/app_mailbox.rs) `RequestAuthorization` 的 25 秒缓存/本地短证明不是向外部机构申请通讯许可。
- 当前小消息已经有独立 receive/notify/send 槽和 WatchFetch 预取，不需要另建 Push 系统才能在线接收。
- [app_attachment_pipeline.rs](../../NOVOCHAT/crates/chat-core/src/app_attachment_pipeline.rs) 仍有两个实际调用者，仅处理 `attachment && !object_segment`。它是未采用对象格式的有效兼容路径，不是 v77 bulk 图片主线，也不是可直接删除的死代码。

## 普通文字的实际链路

源码位置按上述基线，后续以函数名定位。

1. `local_conversation_controller.dart:281,466` → `native_message_store.dart:337`：生成原 Message ID、原加密 journal enqueue；本地提交唤醒发送。
2. `app_mailbox.rs:1742` 的 `run_outgoing` 先订阅再读待发；`send_pending` → `app_mailbox_media.rs::prepare_window` → `offline_delivery.rs::prepare_message_record`。空闲等待事件，不是两秒定时发送。
3. `app_mailbox.rs::deposit_window/rpc_window` → `NativeMailboxHandle.request` → native session/APFL → `MailboxServiceV1::request_local_v1`。Put 事务完成后签发 StoreReceipt，并唤醒既有 Watch。
4. `app_mailbox.rs:1550–1611`：WatchFetch 携带密文或 Fetch → 完整性/E2EE 验证 → 接收 journal 持久化 → inbox_committed。
5. `:1613–1617`：寄存设备回执，再 Consume 原密文。发送端 `:1531–1544` 验证并持久保存设备回执后更新 Delivered，再清理回执对象。

一次“持久设备回执”包含正向投递和反向回执；不能与“对方屏幕已出现”混用，也不能拿 Wi-Fi 标称速率当整条互联网路径速度。

## 热点与证据

| 当前源码 | 已确认的工作 | 结论与最小改动方向 |
| --- | --- | --- |
| [mailbox_store.rs](../crates/novovm-network/src/mailbox_store.rs) `begin` | Immediate 写事务、高水位更新、过期密文与信箱清理 | 读取也承担维护。先合并同一操作的重复事务；不能直接删除时钟防护或到期拒绝。 |
| 同文件 `put_bounded` | 新 Put 统计本信箱和全节点 count/sum；重复 Put 提前返回 | 有扩容成本，但没有证据是当前小库秒级延迟根因。增量计数须涵盖消费、到期、撤销、迁移和恢复。 |
| [mailbox_rpc.rs](../crates/novovm-network/src/mailbox_rpc.rs) `watch_inner_bounded` | 一次 Watch 每个信箱分别 observe，再独立 fetch；同一 store 锁内多次事务 | 第一候选：一次事务验证全部能力/游标并返回快照及一个对象，复用原服务与响应。 |
| 同上 | 先 subscribe，再观察；Put 的 revision 与密文原子提交，提交 worker 内唤醒 | 未发现“先 Fetch 后订阅导致永久漏通知”漏洞，不为假设漏洞重造订阅系统。 |
| 同上 | 一秒空闲期限/撤销复核与消息事件竞争唤醒 | 是重复维护成本，不是每条消息必须睡一秒。应减少无效 DB 工作，不全局缩短定时器。 |
| NOVOCHAT `app_mailbox.rs:1613–1677,2212–2247` | 本条接收后等待回执寄存、Consume 及少量维护，才通知下一轮领取 | 连续积压时可能拖住下一条；首条与积压延迟应分测。若证实显著，在既有 owner 内重叠领取和可恢复收尾，保持有限在途、回执义务和去重。 |

本轮在独立内存 SQLite 3.41.2 中，直接提取当前 store 建表语句执行 [EXPLAIN QUERY PLAN](https://www.sqlite.org/eqp.html)，未接触节点/手机数据：

- 两项过期清理的目标表均为 `SCAN`，没有 expires 索引。
- 全节点 count/sum 为 `SCAN envelopes`。
- 单信箱统计与 pending 查询使用 `(mailbox,id)` 主键索引，不是全库扫描。

这是查询计划证据，**不是延迟测量**，也不等于 Android 所链接 SQLite 的最终执行计划。原始结果在 NOVOCHAT 本机 `artifacts/mailbox-architecture-query-plans.json`。后续过期索引、维护合并和增量计数要比较写入成本及真实规模，不一次全部改造。

已有 `MailboxTimingsV1` 可复用，不新建遥测系统。但 `watch` 包含合法等待，`response_enqueue` 不等于远端收到；当前 BulkPut/FetchBulk/WatchFetchBulk 在 `execute` 的统计归入 `other`。这些汇总不能冒充每阶段 SQL、加密耗时或 P95。

## 性能证据及仍未知的部分

本轮重新读取 v77 验收与原始产物，没有把旧样本当作新一轮测试：

- 两台 Wi-Fi、外部 Iroh 仍参与，1.6 MB 图片选择至设备回执 4.388 / 3.751 秒；本地待发唤醒前已有 1.840 / 1.252 秒。主机与设备混合时钟，仅作粗分段。
- 之后两条文字的发送操作至设备回执为 1.582 / 1.068 秒。现有事件不能可靠分离按钮、加密、每次 SQL、Notify、显示帧全部成本；不凭差值编造完整时延图。
- 同载体 release 本机对照：连续帧约 12–14 ms；原生对象完整性 ACK 约 210–217 ms；bulk 持久寄存约 212–216 ms。对象层成本在数据库前已出现，不能把全部时间归因 SQLite。
- 以上没有手机 P95、CPU/耗电 A/B 或当前跨运营商性能证明。A 独立 L4 拨号仍存在未就绪记录；B 发起的 L4 返回通道已工作，不能据此签收所有 L4 路径。

完整条件、失败样本和旧路径窗口区别见 [reuse index](NOVONET_REUSE_INDEX.md) 与 [v77 验收](../../NOVOCHAT/docs/implementation/MEDIA_REUSE.md)。

## 下一轮执行顺序与停止条件

执行更新：第一项已进入可靠载体反馈合并与写槽容量唤醒修正，详见
[生产台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md) 顶部记录。仅合并反馈的失败
对照也保留；不能将本机对象层收益等同 Android 图片总耗时下降。小消息事务与
容量改造仍按下列顺序后置。

1. **图片主线先收尾。** 复用既有对象测试，在原路径区分本地准备与 native 分帧/加密/载体推进成本。当前 768 字节片段把此图片变成 2,084 个 native 片段；这是真实处理量，不证明有固定逐片睡眠。可靠载体的适配优化不得顺手修改原生 UDP 的 MTU/重传语义。
2. **小消息独立一轮。** 优先比较原 Watch 与同 store owner 下合并 observe+prefetch 事务的版本。首先保留当前持久性和协议，仅减少重复事务；再依据积压样本决定是否重叠领取、回执寄存和 Consume，不把所有变化装进一个 APK。
3. **容量优化后置。** 大规模测试确认扫描占比后再加增量统计/有界清理；清理延后仍须拒绝过期对象，容量含义明确，回滚/崩溃不复活旧能力。
4. **验收固定边界。** 同路径、同载荷、同节点数据量；分别测首条、连续文字、图片期间文字、离线补收、断网与重启。保留每次失败，核对原 ID/TTL、持久回执、重复展示、实际载体。先小批正确性，再足量分布；不能由两次样本声称 P95。
5. **清理同步完成。** 删除经过调用/平台/兼容检查确认无用的旧实现；仍被调用的兼容路径标明用途。最后一个旧格式待发/部署入口退出并通过回归后才删除，不为了让目录显得新而破坏已有消息。

这份审查不批准降级 Privacy、取消撤销/完整性验证、降低 StoreReceipt 的持久承诺，也不扩大研发范围到新账户、信箱、路由器、支付或传输框架。
