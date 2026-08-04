# NOVOVM Product Overlay Mesh Peer Error-Domain Containment v1

状态：实现候选；只签收 multiplexed mesh 的 peer 级错误域收敛，不构成完整
Peer-Local Fault Isolation、可靠投递或生产激活签收。

## 1. 已实现的状态与所有权

一个 `duplex` node 只建立一条已认证 WSS relay connection，但为每个 configured peer
独立维护：

```text
Idle
  -> Handshaking { initiator, expiry }
  -> Active { E2E channel }
  -> Cooldown { retry_at }
  -> Handshaking
```

peer 还独立拥有 handshake replay cache、pre-auth delivery buffer、NovoRUDP frame
sequence、session-failure count 和指数退避。切换一个 peer 的状态不得清空另一个 peer
的 session、replay 或待写队列。

## 2. 可归因 peer 故障

以下故障在来源能够可信绑定到已配置 peer 时，只关闭该 peer 的 E2E channel，并让其
进入独立 `Cooldown`：

- 无效或过期的 signed Offer/Response；
- 当前 session 的 MAC/authentication 错误；
- 无效 NovoRUDP 或 classified payload；
- 每 peer 最多 64 个 opaque envelope 的 pre-auth buffer overflow；
- peer handshake expiry 或 E2E seal/open failure。

隔离会清除该 peer 的 pre-auth buffer 和活动 channel，保留其尚未写入 relay socket 的
in-memory pending queue，并发出 peer-isolation telemetry。健康 peer 继续使用同一 WSS
relay session。未知、未配置来源直接丢弃，不能借此触发共享 relay rotation。
来自旧 E2E generation 的 session-id 不匹配 envelope 或迟到 Response 会直接丢弃；握手
期只有匹配当前本机 Offer session-id 的 envelope 才允许进入 pre-auth buffer，旧 Response
也不得消费当前 initiator，避免 relay 中的旧队列项把健康 peer 推入 overflow/cooldown。

session-failure count 在一次共享 relay lifecycle 内持续驱动 bounded exponential
backoff；成功建立短暂 channel 不立即洗掉恶意 peer 的累计失败成本。

E2E 和 classified framing 合法、但原生交易在签名、身份、nonce 或 chain-domain ingress
被拒绝时，该拒绝只计入 `peer_rejection_total / last_peer_error`，不会写入全局
`worker_error`、停止健康 peer 广播或轮换 relay。该交易不会进入 pending；当前也不会因此
关闭 E2E channel。入口使用 `Accepted / PeerRejected / LocalFault` 三态；durable store I/O、
nonce registry poison、无效本机配置和未知 verifier 错误保守地归为 `LocalFault`，仍会
fail-closed 停止 worker/signoff，不能被伪装成对端坏输入。针对语义滥用的 per-peer
CPU/byte/rate budget 仍属于资源治理门。

## 3. 共享 relay 故障

只有无法安全收敛到单个 peer 的 carrier/lifecycle 故障才允许轮换共享 relay：

- WSS read、write 或 closed；
- relay wire/framing 失败且没有可信 source attribution；
- relay node-key authentication、connect 或整体 session lifecycle 失败。

共享 relay 被轮换时，所有 peer 都必须在新 relay 上建立新 E2E channel；新 endpoint 仍
只能从已验证的 signed bootstrap pool 中选择。

## 4. Delivery 的精确定义

当前事件：

```text
Delivery { delivered: true }
```

只表示 encrypted envelope 的本机 relay socket write 返回成功。成功后对应内存项会被
弹出。它不证明：

```text
recipient received
recipient decrypted/authenticated
quarantine persisted
native transaction accepted
artifact executed/voted/QC-sealed
```

当前没有绑定 `(epoch, object_hash, recipient)` 的 durable ACK 或 journal。socket write
之后、recipient ACK 之前发生断线时，sent-but-unacknowledged obligation 可能丢失；进程
重启也不能恢复内存 pending queue。因此本切片不提供 exactly-once、at-least-once 或
restart-safe delivery 保证。

## 5. 尚未关闭的资源边界

当前 `pending_by_peer` 是无 count、byte 和 TTL 上限的进程内 `VecDeque`。64-envelope
上限只约束 handshake 完成前的单 peer opaque buffer，不约束：

- 已认证 peer 的 outbound pending queue；
- 多 peer fan-out 后的总 queue count/bytes；
- relay concurrent connection/session admission；
- per-identity/per-source reconnect 与 aggregate byte budgets；
- envelope write-side 上限和进程级总内存。

manifest 中出现 `max_sessions` 或 `max_bytes_per_minute` 不等于这些策略已经在全部接入
点完成 enforcement。资源 admission/session/byte/queue bounds 必须有独立实现和攻击测试。

## 6. 准确签收语言

当前可以说：

> NOVOVM Product Overlay 已把 multiplexed WSS relay 上可归因的 peer 协议错误收敛到
> 独立 peer cooldown；单个坏 peer 不再必然轮换共享 relay 或断开健康 peer。

当前不得说：

```text
full Peer-Local Fault Isolation = SEALED
per-peer obligation is bounded or durable
Delivery=true means recipient ACK
relay resource governance is complete
NativeSeal automatic runtime is activation-ready
public four-machine topology has passed
```

后续至少仍需：relay admission/resource bounds、third-flight key-confirm、per-recipient
durable ACK/journal、bounded body/DA、identity guard 原子签名接入，以及公网四机与 Linux
package/long-soak 证据。
