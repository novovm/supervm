# NOVOVM Product Relay Admission & Resource Bounds v1

状态：`SEALED`（本机实现与门禁）。本切片只治理 Product Relay 和 Product Mainline
Overlay 的进程内资源所有权，不把 relay 提升为 NOVOVM 信任根，也不改变 AOEM 边界。
公网四机、VPS、NAT/CGNAT/VPN、Linux 安装包实机和 self-hosted nightly 长跑不在本次
签收范围内，继续标记为 `NOT EXECUTED`。

## 1. 威胁模型

公网 relay 必须假设连接方、已认证 node identity、目标 peer 和消息时序都可能是恶意的。
单靠 WebSocket 读侧 1 MiB 或 `VecDeque` 的 count 上限不能形成完整内存边界，原因包括：

- TLS/HTTP/signed-handshake 完成前的连接和 OS thread 可能被慢连接占满；
- 同一 identity 可通过重连尝试重置速率窗口；
- 多个 identity 可绕过单来源预算并共同耗尽 relay；
- data/control 两类队列若分别计数，会让声明的 per-peer 上限翻倍；
- 小数量的大 frame 仍能越过只有 count 的队列策略；
- mesh fan-out 会把一份 logical payload 变成多个 recipient obligation；
- relay 拒绝一帧后，如果 sender 只看本机 socket write，会产生假的
  `RelayAdmission(admitted=true)`。

## 2. Relay admission

relay 在三个层次独立拒绝资源：

```text
TCP accept
  -> bounded physical connection permit
  -> wall-clock TLS + HTTP + signed-handshake deadline
  -> bounded authenticated session admission
  -> per-identity and aggregate ingress budgets
  -> bounded active/offline delivery ownership
```

约束如下：

- `max_connections` 在创建 connection worker 之前执行；permit 随 worker 退出自动释放。
- `max_connections` 必须大于 `max_sessions`，保证 authenticated session 自身不会占满所有
  physical connection slot。这只是正常连接 headroom，不是 identity-aware reservation；大量
  慢速未认证连接仍能吃满物理上限，公网部署仍需要上游 DDoS/连接准入保护。
- 新 identity 在 `max_sessions` 已满时被拒绝；相同 identity 的新 authenticated session
  可以原子替换旧 session，旧 connection loop 随即失效并退出。
- TLS、HTTP upgrade 和 signed node offer 共用一个绝对握手 deadline；deadline enforcement
  下沉到 rustls 之下的 TCP read/write，慢速 TLS record 或逐字节发送不能无限续期。认证后
  每次 WebSocket frame、对应 outcome/ACK 和同轮 bounded egress 也共用绝对 I/O deadline。
- 普通 wire message 的读侧和写侧都受 1 MiB 上限约束；握手消息另有更小上限；
  WebSocket control frame 不得超过 125 bytes。
- 本版本运行报告中的 `daemon_version` 为 `2`；Rust `V1` 名称只表示当前类型/范围标签，
  不承诺旧 source、wire 或 report schema 可混跑。
- frame-count budget 按 identity 计数；只要 identity 的 budget 仍被跟踪，重连就不会重置
  窗口内预算。达到 `max_tracked_sources` 时可确定性淘汰最旧 inactive identity，因此该限制
  不是抗 Sybil 身份系统；aggregate frame 和 byte budget 始终提供进程级硬上限。
- `source_bytes_per_minute` 限制单 identity；`max_bytes_per_minute` 限制整个 relay，
  因而批量创建 identity 不能绕过总预算。

## 3. Relay queue ownership

active session 的 data/control channel 共享同一 byte account。进入 channel 前必须同时满足
per-session count/bytes，以及独立的 `active_queue_total` / `active_queue_bytes_total` 全局
上限；默认全局上限为 16,384 条、256 MiB，不再使用 `max_sessions * session_queue_*` 推导。
消息被取出或 channel 被丢弃时，RAII account 必须精确释放。

session replacement、disconnect 或 TTL expiry 只撤销旧路由。旧 inbox 中尚存的真实 payload
必须继续计入 global active budget，直到 recv/drop 才释放；不能提前把计数清零后允许新
generation 再占一份实际内存。

offline data/control queue 共享以下限制，不能各自获得一份额度：

```text
per target: count + bytes
per source: count + bytes
process total: count + bytes
local TTL: offline_queue_ttl_ms
```

offline byte accounting 以 delivery wire bytes 为下界，并额外保守计入 queue item 与可能新增的
source/target map-key/String ownership；共享 map key 也按每 item 重复计费。因此 snapshot 是
安全的 memory-accounted 上界口径，不会因超长 target id 或内部 String clone 低估队列预算。

目标重连时只 drain 尚未过期且能获得 active byte permit 的项目。过期、per-target、
per-source 和 global rejection 分别进入 telemetry；snapshot 同时报告 active/offline 的
count 与 bytes，不再用一个含糊的 `queued_frame_count` 代表全部资源。

离线满队列的 ingress rejection 是 O(1)，不会在持有全局写锁时扫描攻击者控制的队列。
过期项目由独立的 1 秒 maintenance sweep，以及目标重连/drain 路径清理；因此到期容量最多
滞留一个 maintenance 周期，不能靠重复 rejection 放大成逐帧 O(queue depth) 工作。

## 4. Mainline fan-out ownership

主节点在接受一个 outbound logical payload 前，按全部目标 peer 原子预留 obligation：

```text
required_count = recipient_count
required_bytes = logical_payload_bytes * recipient_count
```

只有 per-peer 和 global count/bytes 全部满足，`try_submit` 才返回 `true`。payload body 使用
共享 `Arc`，避免 mesh fan-out 深拷贝；每个 recipient obligation 持有独立 RAII permit，
在 relay admission 成功、TTL 过期、worker 停止或 queue drop 时释放。这里释放的只是
Overlay worker 进程内 permit；Host-owned delivery journal 中的 durable obligation 必须继续
保留，直到验证对应 recipient ACK 或进入显式 terminal expiry。即使 relay 断线或当前无可用
route，worker 也必须持续 drain bounded ingress 并清理 TTL，不能让过期项目永久占用预算。

peer E2E handshake 完成前的 opaque delivery buffer 同样具有 per-peer/global count、bytes
和本机 age 上限。该 age 不信任 relay 提供的 `received_at_ms`。

Overlay event channel 按 event-owned bytes（accounted wrapper、String capacity 和 frame
payload capacity）预留 RAII permit；错误文本先截断到 4 KiB。该上限保护 runtime-owned
channel backlog，不声称覆盖已经转移给调用方的返回值或 allocator metadata；`drain_events`
每次最多转移 256 个 event，形成独立的调用方批量边界。

## 5. Relay admission 与 durable recipient ACK 下界

通过 raw wire/速率 admission 并成功解码的 `Data` 和 peer-handshake write 后，client 必须等待与该请求相关的
`ForwardOutcome`。只有 disposition 为 `forwarded`、`queued_target_offline` 或
`queued_backpressure`，Overlay worker 的进程内 pending item 才能移除并产生
`RelayAdmission(admitted=true)`。超限、旧 session、route mismatch 或 shutdown rejection
均保留 worker item 与 Host durable obligation，并令当前 relay lifecycle 失败。

因此本切片完成后：

> `RelayAdmission(admitted=true)` 表示 relay 已明确接管该 envelope（已转发到目标
> session，或已放入受限内存队列），不再只表示 sender socket write/flush 成功。

它仍然不表示：

```text
recipient received or decrypted
recipient ingress accepted
disk journal persisted
restart-safe delivery
exactly-once or at-least-once
transaction executed or proof-sealed
```

Host-owned durable delivery journal 与签名 recipient ACK 是独立于
`ForwardOutcome` 的后续状态机，定义见
[`Product Durable Recipient ACK & Delivery Journal v1`](NOVOVM_PRODUCT_DURABLE_RECIPIENT_ACK_DELIVERY_JOURNAL_V1.md)。
`journal_persisted` ACK 只证明 recipient Host journal durable acceptance 与 pending-only
native ingress acceptance，仍不表示 AOEM 执行、AOEM receipt、proof seal、vote、QC、safe
或 finalized。Sender 收到并验证 ACK 前，不得终结对应 journal obligation。

等待 outcome 时，client 最多临时保存 64 个、合计 16 MiB 的交错入站事件，超限即失败。
整个 outcome 等待、每个 protocol item 和认证后写入都有不可被事件/Ping/Pong 重置的绝对
时限；单次 protocol item 最多处理 64 个 control frame。daemon 先返回 outcome/ACK，再按
data/control 交替偏好最多公平下发一个 queued item；只有 idle read timeout 才额外以 4+4
上限 drain。这个调度关闭主节点连续发送时的确认饥饿，但不把单 TCP 流变成
无限可靠队列：调用方长时间不消费事件时，已经写入 TCP 的 backlog 仍可能位于后续 outcome
之前。

若 raw frame 在 JSON decode 前触发 aggregate/source frame/byte budget，或 frame 本身非法/
过大，daemon 会直接关闭当前连接：此时还没有可信 correlation fields，不能伪造
`ForwardOutcome`。Overlay 把它视为 relay lifecycle failure 并保留 pending obligation。
成功 decode 后的 queue/resource rejection 才返回 correlated outcome。

## 6. Signed directory 与 daemon 配置

`RelayEndpointV1.max_sessions` 和 `max_bytes_per_minute` 是 signed directory 中的容量承诺；
daemon 使用同名实际 enforcement 值并在 report 中导出。evidence verifier 要求 report
`daemon_version=2`、所有 limit 为正且层级自洽、runtime usage 不越界。当前不会让任意 client 把 manifest
值写进 relay 运行时，部署方必须保证已签名 endpoint、daemon config 和运行报告三者一致。
三者不一致时不得签发生产 relay record，也不得宣称该容量承诺已验证。
当前 evidence v1 尚未把 signed endpoint record 与 daemon report 放入同一个交叉证明，
所以“directory capacity 与运行配置一致”仍是部署前置纪律，不是本切片已经证明的事实。

这里的 byte-rate 口径是认证后的 relay ingress wire-message bytes，固定 60 秒窗口；不含
relay egress、WebSocket/TLS/TCP overhead。offline queue 有 target/source/global 总量边界，
但不承诺同一 target 内各 source 的公平份额。

本切片增加 `ForwardOutcome`，但 v1 signed handshake 尚未携带 relay protocol capability。
因此四机部署必须整体升级到同一提交；旧 client/new daemon 或 new client/旧 daemon 混跑
不属于支持的滚动升级路径。显式 capability/version 协商仍是后续协议治理项。

LAN 多机验证必须分发同一个校验后的 release/package，并核对 checksums/package fingerprint、
Overlay/ACK wire 与 journal schema compatibility。各机器可使用不同盘符、工作区和相对数据
路径；这些本机路径不参与协议或版本相等判断。

`node_key_bound_encrypted` 也不能作为公网绕过证书验证的模式：当前 signed relay handshake
尚未 channel-bind 后续 wire，主动 MITM 可转发一次合法握手后篡改 relay admission 结果。
代码因此将该兼容模式限制为 resolved loopback；公网必须使用 `native_web_pki` 或
`explicit_ca`，并继续执行 signed relay identity challenge。

## 7. 验收矩阵

必须至少覆盖：

```text
max_sessions + 1 distinct identity          -> rejected
same identity replacement at session cap    -> admitted; old loop exits
max_connections + 1 pre-auth socket          -> rejected before worker spawn
slow TLS/HTTP/signed offer                    -> absolute deadline closes socket
same identity reconnect in active window     -> budget is not reset
many identities over aggregate bytes         -> aggregate rejection
many identities over aggregate frame count   -> aggregate rejection
data + control share active/offline budgets   -> no double allowance
one source across many targets                -> per-source rejection
offline queue item beyond local TTL           -> expired, never drained
oversized inbound/outbound/control frame      -> rejected before allocation/write
mesh fan-out over per-peer/global budget      -> try_submit=false
pending/pre-auth item beyond local TTL        -> RelayAdmission(admitted=false)
predecode RateLimited/invalid wire            -> lifecycle fails; pending retained
decoded RejectedQueuePeer/Source/TotalLimit   -> pending retained
accepted ForwardOutcome                      -> worker pending released once
accepted ForwardOutcome                      -> durable journal obligation retained
verified signed recipient ACK                -> matching sender-side recipient becomes terminal
recipient ACK                                -> no AOEM/QC/finality inference
```

公网四机、VPS、NAT/CGNAT/VPN、Linux 安装包和 self-hosted nightly 长跑不由这些本机测试
替代，未执行时必须继续标记为 `NOT EXECUTED`。

本机封盘证据：

```text
novovm-network product_relay tests          19/19 PASS
novovm-node product_relay tests             20/20 PASS
novovm-node product_mainline_overlay tests  20/20 PASS
novovm-node product_delivery_journal tests   9/9 PASS
novovm-node product_evidence tests           4/4 PASS
cargo clippy --workspace --all-targets      PASS (-D warnings)
supervm-mainline-gate                       PASS
dual-node lifecycle gate                    PASS
```
