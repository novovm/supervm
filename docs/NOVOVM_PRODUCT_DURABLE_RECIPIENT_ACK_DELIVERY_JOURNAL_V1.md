# NOVOVM Product Durable Recipient ACK & Delivery Journal v1

状态：实现候选与本机定向门禁。该切片为 Product Mainline Overlay 增加 Host-owned、
per-recipient 的持久投递 obligation、签名 recipient ACK 和重启恢复边界，但不把 Relay、
Host journal 或 ACK 提升为 AOEM、共识或最终性证明。Product Overlay 仍为 opt-in；真实公网
拓扑与安装包证据必须独立签收。

## 1. 语义边界

三种容易混淆的成功事件必须分开：

```text
relay ForwardOutcome accepted
  = relay 已转发 envelope，或接管到有界内存队列

signed recipient ACK: journal_persisted
  = 指定 recipient 已把 delivery 以 Accepted 状态同步写入 Host journal，且 payload
    已通过 native ingress 并进入 pending-only 生命周期

AOEM receipt / proof seal / QC / finality
  = 独立的执行与共识生命周期
```

因此：

- relay admission **不等于** recipient durable ACK；
- recipient ACK 只证明 Host 已持久记录对应投递，且 payload 已被 pending-only native
  ingress 接受；`execution_owner=aoem_runtime` 是后续所有权声明，不是执行证据；
- recipient ACK 不证明 AOEM 已执行、状态已持久化、receipt 已生成或状态根一致；
- recipient ACK 不证明区块候选、proof seal、validator vote、QC、fork choice、safe 或
  finalized；
- Relay 仍只搬运 E2E ciphertext，不解释 payload，也不是交易、执行或共识 authority。

ACK 必须绑定 `chain_id`、payload class、object hash、payload SHA-256、稳定
`delivery_id`、original sender、recipient、journal evidence timestamp 和 disposition，并由
recipient identity 签名。Wire 字段名仍为 `accepted_at_ms`；活动 Accepted record 使用原始
acceptance timestamp，而 completed duplicate 当前使用 tombstone 的 `terminal_at_unix_ms`。
因此该字段不能被解释为所有 replay 上都精确保留的原始 ingress acceptance time。Sender 只有
在验证签名、身份、chain 与完整投递绑定后，才能终结该 recipient 的 outbound obligation。

## 2. 所有权边界

该 journal 属于 NOVOVM Host transport/ingress 层。它持久化 opaque payload 与投递状态，
不解释 NOV 余额、nonce、治理、seal 合法性或 AOEM operation：

```text
Product Overlay / Host
  -> delivery identity
  -> per-recipient retry and expiry
  -> durable inbound acceptance intent
  -> signed ACK and duplicate suppression
  -> restart recovery

AOEM Engine
  -> authoritative business execution
  -> state persistence
  -> execution receipt and state evidence

Consensus / seal lifecycle
  -> candidate validation
  -> vote / QC / fork choice
  -> safe / finalized
```

AOEM 不得感知 Relay、WSS、peer route 或 ACK wire；journal 也不得伪造 AOEM receipt 或共识
状态。

## 3. 稳定投递身份

`delivery_id` 是 recipient-specific，必须由下列稳定字段确定性派生：

```text
chain_id
payload_class
object_hash
payload_sha256
original_sender_peer_id
recipient_peer_id
```

session sequence、relay endpoint、机器名、盘符、工作区路径和临时连接 generation 都不得进入
投递身份。相同 payload 面向三个 recipients 时产生三个独立 obligation；一个 recipient 的
relay admission 或 ACK 不得清除其他 recipient 的状态。

## 4. Outbound 生命周期

Host 必须先同步持久化 fanout 与全部新 recipient obligation，之后才能把 payload 交给
Overlay worker：

```text
prepare fanout atomically
  -> Pending
  -> submit to bounded Overlay queue
  -> relay ForwardOutcome accepted
  -> RelayAdmitted (仍是未完成 obligation)
  -> verified signed recipient ACK
  -> recipient tombstone
  -> every recipient ACKed
  -> completion claim
  -> replayable local propagation projection
  -> completion observed
  -> bounded terminal retention and cleanup
```

Relay admission 只允许释放 Overlay worker 的进程内 queue permit；不得删除 Host journal 中的
recipient obligation。Relay 断线、节点重启或进程崩溃后，未 ACK obligation 必须从 journal
恢复并按本地 retry/TTL 策略重投。

Fanout completion projection 在 claim 已落盘而 observed 尚未落盘的窗口允许重驱。
`completion_observed` 只记录 journal 已调用本机 propagation projection，不证明跨 RocksDB
与外部系统的 exactly-once transaction；当前 projection 也不是一个 durable、transactional
external owner。当前 transport 的准确语义是 retry-until-ACK-or-expiry 与 duplicate-tolerant，
不承诺 exactly-once delivery。未来若接入 durable external consumer，必须以 `fanout_id`
作为幂等键。

## 5. Inbound 生命周期

Recipient 收到并验证 E2E data frame 后，必须先持久化 inbound delivery，再进入 Host ingress：

```text
authenticated E2E frame
  -> verify delivery binding
  -> durable inbound Prepared
  -> native ingress Accepted (pending_only=true)
  -> durable Accepted + ack_pending
       |-> ACK branch
       |    -> enqueue signed recipient ACK
       |    -> correlated ForwardOutcome accepted
       |    -> RecipientAckRelayAdmission(admitted=true)
       |    -> mark ACK emitted; ack_pending=false
       |
       |-> execution branch
       |    -> asynchronous AOEM execution
       |    -> execution receipt becomes observable
       |
       `-> terminal join
            -> ack_pending=false AND AOEM receipt observable
            -> complete inbound and retain terminal tombstone
```

不得在 durable `Prepared` 之前调用业务 ingress，也不得仅因 ACK 已排入内存 channel 就清除
`ack_pending`。ACK 的 emitted 标记只能由与该 `delivery_id` 绑定的 relay-admission 成功事件
驱动。`RecipientAckRelayAdmission(admitted=true)` 仅表示 ACK envelope 获得了严格关联的
accepted `ForwardOutcome`；它可能已经 forwarded，也可能只是进入 relay 的有界内存队列，
不证明 original sender 已收到 ACK。`mark_inbound_ack_emitted` 因而只是“本机 relay 已接管
ACK”的记账，不是 peer-read receipt。失败 admission 不得清除 durable ACK intent。

ACK 分支与 AOEM execution 分支没有强制先后关系，但必须在 terminal tombstone 前汇合。
`complete_inbound` 对 `ack_pending=true` fail closed；只有 ACK 已被 relay admission 接管并由
`mark_inbound_ack_emitted` 持久化为 `ack_pending=false`，同时 AOEM execution receipt 已可读，
才能把 Accepted inbound record 压缩为 tombstone。若 AOEM 先完成，active record 保持
Accepted 并继续保留 ACK intent；若 ACK 先完成，record 保持 Accepted 直到 AOEM receipt 出现。
因此 ACK 可以在 AOEM 执行前发送，但仍完全不能证明 AOEM execution。

崩溃恢复规则：

- `Prepared` 必须按 `delivery_id` 重驱 Host ingress；
- `Accepted + ack_pending` 必须重发 ACK；
- AOEM receipt 已存在但 `ack_pending=true` 时不得生成 terminal tombstone；
- `ack_pending=false` 但 AOEM receipt 尚不存在时必须保留 Accepted record；
- ACK 已发送但 emitted 标记尚未落盘时允许重复 ACK；sender 必须幂等处理；
- emitted 已落盘但 sender 未收到时，sender 重发 payload 会从 active record 或 completed
  tombstone 再排入签名 ACK；
- terminal tombstone 在 retention 窗口内拒绝同一 ID 的 equivocation，并允许重复投递收敛；
- ACK 并不替代 AOEM、receipt 或 consensus 的独立恢复逻辑。

## 6. 持久化、容量和清理

Journal 使用独立 RocksDB path，并以同步 WriteBatch 原子更新 primary record、时间索引、usage
与 tombstone。公开资源配置至少包括：

```text
max_entries
max_bytes
obligation_ttl_ms
terminal_retention_ms
retry_interval_ms
```

`max_entries` 统计 fanout、active inbound/outbound 与 retained tombstone 等 logical primary
records；派生 RocksDB index 不伪装成额外 payload 配额。`max_bytes` 统计 active inbound /
outbound 中实际保留的 opaque payload bytes；fanout 面向多个 recipient 时按保留副本口径计入
容量。容量检查必须在原子插入前完成，失败不能留下部分 fanout、usage 或 index。

到期转换、tombstone retention 与 fanout cleanup 必须同时维护对应 index 和 usage。启动时需要
核对 schema、`chain_id + local_peer_id` scope 与 usage；不同 chain 或 local identity 不能复用
同一个 journal。

## 7. 路径与多机部署

Operator-facing journal path 必须支持相对路径，并从持有该配置项的 Product Overlay 配置文件
目录解析，不能从当前工作目录、仓库根或固定盘符推导。四台设备可以使用不同的盘符、工作区
和数据目录；这些本机路径不得进入 wire、`delivery_id`、package fingerprint 或拓扑一致性
判断。

同一个 LAN gate / deployment 必须使用同一已校验产品包和同一协议版本：

```text
same release/package artifact set
same checksums or package fingerprint
same Product Overlay and ACK wire version
same journal schema compatibility
same chain and signed bootstrap/relay records
```

当前没有 mixed-version capability negotiation。不得让旧 client、新 daemon、新 ACK wire 或
不同 journal 语义滚动混跑；升级采用同包分发、协调停机和整体重启。私钥与本机 journal DB
不得在机器之间复制。

## 8. Fail-closed 规则

- 空 payload、未知 payload class、零 object hash、错误 chain、无效 peer identity 或
  delivery-ID mismatch 必须拒绝；
- 同一 delivery ID 绑定不同 payload、sender、recipient 或 fanout 必须视为 equivocation；
- 未知、错误签名或字段不匹配的 recipient ACK 不得改变 outbound obligation；
- relay admission rejection、断线、超时或 queue rejection 不得伪造 recipient ACK；
- 未完成 Host ingress acceptance 的 inbound delivery 不得签发 durable ACK；
- journal 容量耗尽必须 fail closed，不能回退到无持久化的发送或接收路径；
- terminal retention 结束前，重复 delivery / ACK 必须幂等收敛；
- journal 故障不能降级为 legacy host mutation 或无证据继续传播。

## 9. 本机门禁与未执行范围

CI 必须至少运行 journal 定向测试：

```text
cargo test -p novovm-node --lib product_delivery_journal::tests
```

Mainline signoff 的 `received_total` / `delivered_total` 与对应 `EXPECTED_*` 只是当前进程计数，
进程重启后不会累加。跨重启恢复必须单独检查 `journal_recovered_outbound_total`、
`journal_recovered_inbound_total` 和实际 journal records。即使当前进程计数达到阈值，只要仍有
active outbound、`Prepared` 或 `ack_pending` inbound、尚未 relay-admitted 的 ACK，或 unobserved
fanout completion，signoff 仍必须拒绝；当前进程一旦观察到 outbound 或 inbound-Prepared
TTL expiry，也不得签收为零丢失运行。

定向测试覆盖 schema/scope reopen、outbound/inbound recovery、fanout 原子容量、per-recipient
ACK、completion recovery、duplicate/equivocation 与 terminal cleanup。它们证明本机代码路径，
不替代真实部署证据。

以下范围在取得对应签名报告之前继续保持：

```text
four physical machines / public VPS       NOT EXECUTED
public Internet topology                   NOT EXECUTED
NAT / carrier CGNAT / VPN / TUN            NOT EXECUTED
Linux install-package smoke                NOT EXECUTED
self-hosted nightly long soak              NOT EXECUTED
```

LAN 同包测试即使通过，也只能声明 LAN private topology 已执行；不能据此升级公网、跨 NAT、
Linux 安装或 nightly 的状态，更不能声明 AOEM execution、proof seal、QC 或 finality 已由 ACK
证明。
