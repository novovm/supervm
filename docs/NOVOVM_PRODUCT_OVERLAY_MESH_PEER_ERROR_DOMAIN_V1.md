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

## 4. RelayAdmission 与 RecipientAck 的精确定义

当前事件：

```text
RelayAdmission { admitted: true }
```

经 `NOVOVM Product Relay Admission & Resource Bounds v1` 收紧后，只表示 relay 返回了与
该请求相关的 accepted `ForwardOutcome`：消息已交给目标 active session，或进入有
count/bytes/TTL 上限的 relay 内存队列。它不证明：

```text
recipient received
recipient decrypted/authenticated
quarantine persisted
Host ingress accepted
artifact executed/voted/QC-sealed
```

NativeTransaction 主线现在使用 Host-owned、per-recipient RocksDB delivery journal，并要求
验证 recipient 签名的 `journal_persisted` ACK 后才终结对应 obligation。Relay admission 只释放
Overlay worker 的进程内 queue permit；未 ACK obligation 可以在重启后从 journal 恢复并按
retry/TTL 策略重投。该 ACK 只证明 recipient Host journal 已 durable Accepted，且 payload 已
进入 pending-only native ingress；它不证明 AOEM 已执行、receipt 已生成或任何 seal/QC/finality
状态。ACK 可以在 AOEM 执行前发送；recipient inbound terminal tombstone 仍要求 ACK 已获得
relay admission（`ack_pending=false`）与 AOEM execution receipt 两个条件同时满足。

这仍不是 exactly-once delivery。准确语义是 retry-until-ACK-or-expiry、duplicate-tolerant；
completion projection 也可能在 claim 已落盘而 observed 尚未落盘时重驱。NativeSeal 虽有
transport payload class 和 quarantine API，但活动 main runtime 仍跳过 NativeSeal，尚没有把
durable verification/quarantine owner 接到 ACK 生成路径，因此不得外推 transaction ACK 的
完成状态。

`RelayAdmission(admitted=false)` 只更新 `peer_delivery_failure_total`、
`peer_delivery_failure_counts[remote_peer_id]` 和 `last_peer_error`。它不会写入全局
`worker_error` / `failed_total`，也不会因单个 peer 的 TTL 或 relay admission 失败冻结其他
健康 peer 的广播。

## 5. 资源边界

上述资源风险已由 `docs/NOVOVM_PRODUCT_RELAY_ADMISSION_RESOURCE_BOUNDS_V1.md` 接管：
outbound fan-out、pre-auth、event channel、relay physical/authenticated admission、active/
offline queues、per-source 与 aggregate ingress 都具有 count/byte/TTL 或速率边界。该签收
仍只覆盖进程内 ownership。NativeTransaction 的 Host durable journal/recipient ACK 由
[`NOVOVM Product Durable Recipient ACK & Delivery Journal v1`](NOVOVM_PRODUCT_DURABLE_RECIPIENT_ACK_DELIVERY_JOURNAL_V1.md)
单独治理；NativeSeal durable owner、source fairness、wire capability 协商和公网容量实测
不在本资源切片范围内。

## 6. 准确签收语言

当前可以说：

> NOVOVM Product Overlay 已把 multiplexed WSS relay 上可归因的 peer 协议错误收敛到
> 独立 peer cooldown；单个坏 peer 不再必然轮换共享 relay 或断开健康 peer。

当前不得说：

```text
full Peer-Local Fault Isolation = SEALED
RelayAdmission means recipient ACK
NativeTransaction recipient ACK means AOEM execution or receipt
NativeSeal durable verification/quarantine ACK owner is active
relay resource governance itself proves recipient durability or source fairness
NativeSeal automatic runtime is activation-ready
public four-machine topology has passed
```

后续至少仍需：third-flight key-confirm、NativeSeal per-recipient durable
verification/quarantine ACK owner、bounded body/DA、identity guard 原子签名接入，以及公网
四机与 Linux package/long-soak 证据。
