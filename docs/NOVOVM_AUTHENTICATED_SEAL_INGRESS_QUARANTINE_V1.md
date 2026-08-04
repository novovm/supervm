# NOVOVM Authenticated Seal Ingress & Quarantine V1

状态：活动实现契约；覆盖 Proof-Sealed BFT Finality 路线的 Slice 2B1，
不构成自动共识传播、proof seal、链级 canonical 或主网最终性签收。

前置契约：

- [`NOVOVM_SEAL_CONTRACT_VALIDATOR_SAFETY_V1.md`](NOVOVM_SEAL_CONTRACT_VALIDATOR_SAFETY_V1.md)
  定义 NOV seal subject、proposal、vote、weighted QC 和本机 persist-before-emit
  签名安全；
- [`NOVOVM_UNSEALED_BLOCK_CANDIDATE_GRAPH_V1.md`](NOVOVM_UNSEALED_BLOCK_CANDIDATE_GRAPH_V1.md)
  定义 AOEM-owned、可恢复但未封印的候选图。

本切片增加的是一个 fail-closed 的远端接收边界：来源已认证、格式和密码学有效的
seal artifact 先进入独立 quarantine；只有本机 AOEM-owned ledger 能重建完全相同的
subject 时，proposal 或 QC 才能进入本机 seal store。

必须始终按下式理解当前能力：

```text
authenticated transport
  -> pinned authority + canonical wire + signer/source verification
  -> durable quarantine
  -> exact local AOEM candidate reconstruction
  -> locally verified seal-store object

locally verified QC != proof_sealed block != canonical chain != finality
```

## 1. 当前已实现边界

Slice 2B1 已实现：

1. operator-pinned genesis epoch authority；
2. 最大 192 KiB 的版本化 canonical seal wire；
3. proposal/vote signer 与已认证 transport peer 的绑定；
4. 独立 RocksDB quarantine、同步写入和重启读回；
5. canonical replay 计数和冲突 replay 拒绝；
6. proposal、vote 和 QC slot 的有界 equivocation 记录；
7. 本机 validator identity guard；
8. remote proposal/QC 与本机 AOEM-owned candidate 的 exact-match reconcile；
9. reconcile 后 seal object/QC 的持久化，同时保持所有链级 finality 标志为 false。

它没有修改 AOEM。validator epoch、transport identity、seal artifact、quarantine、
equivocation 和 QC 都属于 NOVOVM Host 产品协议；AOEM 仍是第三方通用执行与持久化
内核，不承载 NOV 专属共识业务。

## 2. Operator-pinned epoch authority

`NovNativeSealEpochAuthorityV1` 把以下事实承诺为一个不可变 authority manifest：

```text
chain_id
genesis_block_hash
AOEM protocol_config_commitment
epoch / activation_height
validator_set and validator_set_hash
validator_id -> transport_peer_id bindings
leader_schedule
authority_commitment
```

authority 从本机 durable genesis、AOEM ownership metadata、validator-set snapshot 和
operator 提供的 transport bindings 派生。quarantine 第一次绑定时还必须收到 operator
预期的 `authority_commitment`；首次同步写入后，同一 chain/epoch 只能读回完全相同的
manifest。重启不会重新选择 authority，重复绑定相同 manifest 幂等，不同 manifest
或不同 operator commitment 一律 fail closed。

该 authority 不是链上治理结果，也不是动态发现协议。V1 的硬边界是：

```text
epoch = 1
activation_height = 1
round = 0
validator_count <= 64
leader_schedule = round-robin-validator-id/v1
```

每个 validator 必须恰有一个唯一、合法的 transport peer id；peer id 也不能被两个
validator 共用。proposal signer 还必须等于该 height/round 的 authority-selected
leader。

V1 没有协议协商或滚动混部兼容层。所有 validator 必须以同一 validator manifest、
subject/proof version、authority schema、wire version 和 leader schedule 锁步部署。
任一版本或 commitment 不同都应被拒绝，不能降级解释。

## 3. Canonical 192 KiB seal wire

`novovm-native-seal-overlay-wire/v1` 对 proposal、vote 和 QC 使用固定 header 与 canonical
Postcard payload。整个 seal wire 的硬上限为 192 KiB，包含 header、payload 和 32-byte
checksum。

header 绑定：

```text
magic / wire_version / artifact_kind / reserved_flags
chain_id / epoch
authority_commitment
object_hash
payload_length
```

payload 后附带域分离的 SHA-256 wire checksum。decoder 在分配或接受 artifact 前检查
总长度、magic、版本、保留位、payload 长度和 checksum，再执行 canonical decode /
re-encode byte equality、artifact 密码学验证，以及 header/object/chain/epoch 的反向绑定。

checksum 只用于传输损坏和 canonical framing 检测，不替代 Ed25519 签名、QC 权重重算
或 transport peer authentication。QC wire 可以压缩重复 vote 字段，但 decode 后必须
恢复完整 vote，并重新验证每个 voter、签名、proposal binding、排序、唯一性、权重、
阈值和 QC hash。四个等权 validator 下仍是 2/4 拒绝、3/4 才成立。

超长、截断、追加尾字节、非 canonical payload、未知 kind、错误 authority、错误 object
hash、坏签名或坏 QC 都不能进入 quarantine。

## 4. 已认证来源绑定

Product Overlay relay 只搬运 E2E ciphertext，永远不是 validator、signer、执行或共识
authority。seal ingress 使用上游已经认证的 `source_peer_id`，并按 pinned authority
执行以下绑定：

```text
proposal source peer = proposer validator transport peer
vote source peer     = voter validator transport peer
QC source peer       = any authority-bound validator transport peer
```

QC 是多签集合，没有唯一网络 signer，因此不把 QC 错误绑定到 proposal proposer；但
QC 必须来自 authority-bound validator peer，且内部每张 vote 仍逐一验证。未绑定 peer、
proposal/vote signer 与来源不一致、或非预期 leader 的 proposal 均在持久化前拒绝。

接收面还限制为 authority activation height 之后、相对本机执行高度最多落后 128 块、
最多领先 2 块，并且 V1 只接受 round 0。这是无 durable pacemaker/timeout certificate
阶段的有意限制，不能通过网络输入预锁未来 round。

## 5. Durable quarantine 与 replay

远端 artifact 存入独立 schema：

```text
novovm-native-seal-overlay-quarantine/v1
```

quarantine 与本机 seal signing store、candidate ledger 和 AOEM state 分离。接收远端
artifact 不会生成本机 proposal/vote safety lock，不会写本机 outbox，不会改变 AOEM
状态，也不会改变 candidate finality。

一次 admission 同步持久化并 readback：

```text
artifact kind / object hash / proposal hash
chain / epoch / height / round
first and last authenticated source peer
canonical wire hash
first and last receive time
receive_count
admission state
equivocation evidence hashes
```

完全相同的 canonical artifact/wire replay 是幂等接收：保留原对象，递增
`receive_count`，更新最后来源和时间，并返回 `duplicate=true`。相同 object key 指向
不同对象、不同 canonical wire 或缺失持久对象时拒绝，不能以 replay 覆盖历史事实。

主要 admission state 为：

```text
crypto_verified_awaiting_local_execution
qc_crypto_verified_quarantined
equivocation_quarantined
locally_matched_vote_eligible
qc_locally_verified_durable
```

这些是接收与本机验证状态，不是 fork choice 或 finality 状态。

## 6. Equivocation 与本机 identity guard

quarantine 为三类 artifact 保存严格 slot index：

```text
proposal: chain / epoch / height / round / proposer
vote:     chain / epoch / height / round / phase / validator
QC:       chain / epoch / height / round
```

同一 proposal 或 vote slot 出现不同 object 就形成 equivocation evidence。同一 QC slot
出现不同 subject/proposal 的有效 QC 也形成 evidence。每个 slot 最多保留 4 个不同
object；超过上限直接 fail closed，不能让未界定输入无限增长 quarantine。

evidence 绑定 slot、排序后的两个 object hash、artifact kind、chain、epoch、height 和
round，并同步持久化。如果冲突 signer 是本机 validator identity，还会写 durable
`signing_blocked=true` identity guard。只要 slot index 已经 contested，该 slot 中任何
proposal/QC 都不能 reconcile；早到 admission 是否已经显示 quarantine 不构成绕过，
durable slot index 和 evidence 才是最终阻断依据。

## 7. AOEM exact-match reconcile

密码学有效只允许 artifact 留在 quarantine，不能让远端声明变成本机执行事实。
proposal 或 QC 进入本机 seal store 前，节点必须从自己的 durable candidate ledger：

1. 加载 active、local AOEM-owned、readback-verified、unsealed candidate；
2. 重新验证 immutable block、body、receipt、parent、AOEM ownership/config 和 evidence
   commitments；
3. 使用本机 seal store 重建完整 `NovNativeSealSubjectV1`；
4. 要求重建 subject 与远端 proposal/QC subject byte-for-byte 相同；
5. 确认 proposal/QC slot 没有竞争 evidence；
6. 同步写入并 readback remote proposal，随后才允许持久化 locally verified QC。

远端 proposal bridge 不生成本机签名、role lock、round/height lock 或 outbox。它只说明
“本机 AOEM 执行结果与该远端已签 subject 完全一致”。缺少本机候选、candidate hash、
任一 root、receipt、state version、AOEM evidence、parent QC 或 protocol commitment
不一致时，reconcile 必须失败。

## 8. Finality 仍保持 false

quarantine admission、exact-match proposal 和 locally verified QC 都不执行跨库 canonical
promotion。当前 ledger/candidate 仍必须保持：

```text
fork_choice_selected = false
chain_canonical = false
proof_sealed = false
safe = false
finalized = false
```

`canonical_local=true` 只表示本机 AOEM execution projection 的父子连续性，不等于链级
canonical。Slice 2B1 没有 canonical pointer、promotion journal、AOEM branch promotion、
rollback/reorg、HotStuff commit rule 或 finalized ancestor 驱动的 epoch rotation。

## 9. Main runtime 的 NativeSeal 仍 dormant

Product Mainline Overlay 已有显式 `NativeSeal` payload class、192 KiB logical payload
边界、opaque E2E transport 编解码和 `try_submit_native_seal` API。transport 层不解析
seal bytes，避免把共识验证所有权塞进 relay/Overlay。

但是活动 `novovm-node` native execution pipeline 当前仍只把
`NativeTransaction` 送入交易 ingress；收到 `NativeSeal` event 时明确跳过，且节点没有
从 seal outbox/QC 自动调用 `try_submit_native_seal`。因此当前状态是：

```text
NativeSeal transport capability/API = implemented
authenticated seal quarantine API   = implemented
main runtime automatic receive route = dormant
main runtime automatic send route    = dormant
```

不得把 API 可调用或本地测试构造解释成已经运行四 validator 的自动 proposal/vote/QC
网络。

## 10. 自动发送与生产接入的阻断门

在自动发送或激活 main runtime 的 NativeSeal route 前，至少必须关闭以下六个门：

### 10.1 Mesh peer error-domain containment 与资源边界（已完成进程内边界）

Product Mainline Overlay 已实现
[`Mesh Peer Error-Domain Containment v1`](NOVOVM_PRODUCT_OVERLAY_MESH_PEER_ERROR_DOMAIN_V1.md)：
每个已配置 peer 独立维护 `Idle / Handshaking / Active / Cooldown`、replay、frame sequence、
pre-auth buffer 和 session-failure backoff。可归因到单个 peer 的坏 handshake、坏 MAC /
envelope、坏 NovoRUDP/classified frame、handshake 超时或 pre-auth overflow 只隔离该 peer，
不再关闭或轮换共享 relay，也不重置其余健康 peer 的 channel。WSS read/write/closed、无法
可信归因到 peer 的 relay wire 错误，以及 relay 自身认证/生命周期错误仍属于共享故障域。

[`Product Relay Admission & Resource Bounds v1`](NOVOVM_PRODUCT_RELAY_ADMISSION_RESOURCE_BOUNDS_V1.md)
进一步关闭了进程内资源门：`pending_by_peer`、pre-auth、event channel、物理连接、认证
session、identity/aggregate ingress 以及 active/offline relay queue 均具有明确的 count、
byte 和适用的 TTL/deadline 边界。`Delivery=true` 现在表示 relay 返回严格关联的 accepted
`ForwardOutcome`，不是单纯 socket write；但它仍不表示目标 validator 已接收、解密或
持久化。因此 durable recipient ACK/journal 与 restart recovery 仍是 NativeSeal 激活的
硬门。

已认证且 framing 正确、但无法通过原生交易签名/身份/nonce/chain-domain ingress 的单笔
payload 现在只记录 peer rejection，不再写入全局 worker error 或停止其他 peer 的广播；
该交易仍 fail-closed，不会进入 pending。durable store、nonce registry、本机配置或未知
verifier 故障由 typed 三态保持为全局 `LocalFault`，不会被降级掩盖。重复语义拒绝的资源
预算仍归上述未关闭门。

### 10.2 第三航 key-confirm

现有 Offer/Response 两航建立不能直接作为自动共识发送的最终开门条件。必须增加显式
第三航（third-flight）key-confirm，由 initiator 使用新会话密钥证明 key possession；
responder 在验证 confirm 前不得把 session 标记为可接收共识 artifact，也不得发送 vote、
QC 或 delivery ACK。confirm 必须绑定双方 identity、session id、handshake transcript、
chain 和协议版本，并具备 replay/expiry 防护。

### 10.3 Relay byte/session bounds（已完成进程内边界）

node 侧 192 KiB seal/classified payload 上限与 relay/client 读写侧 1 MiB wire 上限共同
执行；handshake/control 具有更小独立上限。物理连接、绝对 handshake deadline、并发
session、每 identity/aggregate 速率、active/offline queue 与 pre-auth buffer 均已纳入
count/byte/time enforcement 和负向测试。该完成项只证明当前进程内资源所有权，不替代
上游 DDoS 防护，也不提供 durable delivery。

### 10.4 Per-recipient durable ACK/journal

relay socket write 成功不等于目标 validator 已接收，更不等于 quarantine 已持久化。
每个 `(epoch, object_hash, recipient_validator)` 必须有独立、可重启恢复的 delivery
journal；只有目标 peer 在认证 session 上返回绑定 artifact/wire hash 的 durable ACK 后，
才可关闭该 recipient obligation。一个 peer 的 ACK 不能清除其他 peer 的任务，ACK 也
绝不能删除 signer safety lock，或被解释成 vote、QC、proof seal 或 finality。

### 10.5 Body/DA 与本机执行验证

seal proposal/QC wire 承诺 inline body digest，但不自动向缺失该 block body 的 validator
提供完整 body/DA。节点在没有 bounded body acquisition、完整性验证和本机 AOEM execution
readback 前不能投票。缺 body/DA 的 artifact 只能留在 quarantine；不得因已有 3/4
signature 或生产者自报 evidence 绕过 exact-match。

### 10.6 Identity guard 必须进入签名原子边界

当前 identity guard 是 durable quarantine 审计/阻断状态，但 Slice 2B1 没有把网络
ingress 接入自动签名，也没有让 `sign_local_proposal` / `sign_local_vote` 自动读取该 guard。
激活前，Host 必须从本机受保护身份派生 `local_validator_id`，从 durable ledger 派生
本机高度，并在生成任何本机签名前与 seal safety lock 放在同一原子决策边界检查 guard。
调用方自报 identity/height 不能成为安全根，QC 内嵌 vote 也必须进入有界 vote-slot 与
本机双签检查。

## 11. 未执行与不得外推的范围

以下实机/长跑证据不属于本切片，当前必须保留为：

```text
four physical validator machines                NOT EXECUTED
public VPS topology                              NOT EXECUTED
NAT topology                                     NOT EXECUTED
carrier CGNAT topology                           NOT EXECUTED
VPN / TUN topology                               NOT EXECUTED
Linux install-package smoke                      NOT EXECUTED
self-hosted nightly long soak                    NOT EXECUTED
```

同样尚不能声称自动 proposal/vote/QC 广播、timeout/pacemaker、fork choice、canonical
promotion、reorg/rollback 或主网最终性已经完成。

## 12. 下一接入顺序

正确顺序是：

```text
mesh peer error-domain containment                       IMPLEMENTED
-> relay admission + byte/session/queue bounds
-> third-flight key-confirm
-> per-recipient durable delivery journal and ACK
-> bounded body/DA acquisition
-> identity guard integrated into the atomic signing boundary
-> authenticated NativeSeal runtime ingress
-> local AOEM exact-match vote eligibility
-> automatic proposal/vote/QC scheduling
-> fork choice and crash-recoverable canonical promotion
```

在最后一步完成前，即使 3/4 QC 已在本机 durable seal store 中通过验证，所有链级
finality 字段仍必须保持 false。
