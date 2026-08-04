# NOVOVM Seal Contract & Validator Safety V1

状态：活动实现契约；覆盖 Proof-Sealed BFT Finality 路线的 Slice 2A。

本切片完成 NOV 专用 seal subject、proposal、vote、weighted QC 以及本机
persist-before-emit 安全存储，但尚未接入 Product Overlay、leader 调度、超时证书、
fork choice 或 canonical promotion。

必须始终按下式理解当前状态：

```text
seal subject != signed proposal != vote != QC != proof_sealed block != finality
```

即使独立 seal store 中已经存在密码学有效的 QC，candidate graph 中仍保持：

```text
fork_choice_selected = false
chain_canonical = false
proof_sealed = false
safe = false
finalized = false
```

相关前置契约：
[`NOVOVM_UNSEALED_BLOCK_CANDIDATE_GRAPH_V1.md`](NOVOVM_UNSEALED_BLOCK_CANDIDATE_GRAPH_V1.md)

## 1. 所有权边界

本实现位于 `novovm-node::native_block_seal`，没有修改 AOEM。AOEM 继续只提供
第三方通用执行内核能力；NOV 的链域、验证者 epoch、区块 seal、投票和 QC 都属于
宿主产品协议，不能下沉到 AOEM DLL。

旧 `novovm-consensus` 的 `BFTProposal`、`Vote` 和 `QuorumCertificate` 不构成
NOV proof-seal signing artifact，也不能包装后升级使用。旧 vote 没有完整绑定 chain、epoch、
round、phase、validator-set、NOV block roots 和 AOEM evidence；旧 QC artifact hash
也没有绑定完整排序票集合。本切片只复用 Ed25519、唯一 voter 和 weighted quorum
等算法思想，所有 NOV 签名都使用新的域和消息。

## 2. 本机签名资格

节点内部 Rust seal API 不接受调用者自报的候选生命周期布尔值。它必须从 durable candidate
ledger 重新加载 record 和 immutable block artifact，并通过 ledger 原有校验重算
其 block/body/receipt/AOEM 绑定。

只有同时满足以下条件的候选可构造本机签名 subject：

```text
candidate_source = local_aoem_owned_execution
lifecycle_status = active_unsealed
commitment_bindings_verified = true
parent_continuity_verified = true
body_data_available = true
local_aoem_readback_verified = true
execution_selected_local = true
fork_choice_selected = false
chain_canonical = false
proof_sealed = false
safe = false
finalized = false
abort_reason = none
```

此外，ledger 必须已有持久 AOEM ownership/protocol-config binding。observed 候选
即使结构闭合、生产者自报 AOEM evidence，也不能获得本机投票资格。observed 候选
经本机独立执行后晋级为可投票候选尚未实现。

这些本机资格字段不进入四机共同签名的 subject；它们只是每个 validator 在签名前
必须独立完成的前置检查，避免不同机器的本地路径或可变 metadata 改变共识消息。

## 3. Canonical seal subject

`NovNativeSealSubjectV1` 使用固定字段顺序、固定宽度大端整数、变量字段 u64
长度前缀和 SHA-256 域分离构造 `subject_hash`，不 hash JSON，不依赖 Rust 默认
序列化布局。

subject 明确绑定：

- protocol、proof、verification-profile 和 vote phase 版本；
- chain、genesis block、protocol-config network domain、validator epoch/set hash；
- height、slot、round、timestamp、parent block 和 justify QC；
- block hash、candidate id、execution-context commitment；
- pre/post state root、state version和 state-root codec；
- ordered transaction root、transaction count；
- 当前 inline full-body digest、body bytes 和独立 inline-body commitment；
- block receipt root、cumulative receipt root、receipt count 和 receipt codec；
- AOEM parent commitment、batch id、batch-result commitment、expected-output
  commitment、execution evidence commitment 和 evidence kind。

当前 `data_availability_scheme` 明确为：

```text
inline-full-body-digest/v1
```

它表示 validator 在本地拿到了完整 body 并验证了既有 body digest；它不是独立 DA
网络、erasure coding 或 DA Merkle root。代码因此没有把一个尚不存在的 DA 协议
伪装成完成状态。

subject 明确排除路径、candidate source/revision/abort reason、producer-local
`canonical_local`、本机 readback 布尔值以及全部最终性布尔值。

## 4. Validator-set snapshot 与 quorum

`NovNativeSealValidatorSetV1` 是不可变 epoch snapshot，绑定：

```text
chain_id
epoch
activation_height
sorted (validator_id, Ed25519 public_key, weight)
total_weight
strict quorum_weight
validator_set_hash
```

validator id 从公钥经独立域确定性派生。构造时拒绝空集合、零权重、重复
validator/key、弱或非法 Ed25519 key、未排序存储和权重溢出。输入顺序不影响最终
snapshot/hash。

quorum 规则是严格大于总权重三分之二：

```text
quorum_weight = floor(2 * total_weight / 3) + 1
```

计算使用 u128 中间值；四个等权 validator 的 total=4、quorum=3，所以 2/4 必须
拒绝，3/4 才能形成 QC。这里没有硬编码“必须四节点”，weighted set 仍按相同公式
重算。

validator-set 的治理来源、历史 epoch 激活和 finalized ancestor 驱动的轮换尚未接入
节点生命周期。本切片只持久化并严格核对调用者提供的不可变 snapshot；生产网络
不得把 RPC 参数当作权威 validator set。

## 5. Proposal、Vote 和 QC

Proposal 由 validator-set 成员使用 Ed25519 对独立 proposer domain 下的
`subject_hash + proposer_id` 签名。proposal artifact hash 再绑定 subject、proposer、
signature scheme 和签名字节。

Vote 使用不同的 vote domain，并签署：

```text
chain / epoch / height / round / phase
validator_set_hash
subject_hash
proposal_hash
validator_id
```

因此 proposal signature 不能重放为 vote，vote 不能跨 chain、set、round、phase、
subject、proposal 或 voter 重放。

QC 保存完整 subject、proposal hash，以及按 validator id 严格排序的 individual
votes/signatures。验证时重新完成：

- subject 和 validator-set hash 校验；
- proposal binding 校验；
- validator membership 和每张 Ed25519 签名校验；
- duplicate voter 拒绝；
- signed weight、quorum weight 和 signature count 重算；
- QC artifact hash 重算。

`signed_weight`、`quorum_weight`、`signature_count` 和
`threshold_satisfied` 只是可审计字段，不能替代重算。

非创世 subject 必须引用 seal store 中已经持久化并验证的父块 QC；该 QC 必须属于
同 chain/epoch、正好是前一高度并绑定候选的 parent block。v1 尚不支持跨 epoch 的
父 QC/validator-set rotation。

## 6. Persist-before-emit 与防双签

seal store 使用独立 RocksDB schema：

```text
novovm-native-block-seal-store/v1
```

它持久化 validator snapshots、基于 genesis + AOEM ownership/config 的逻辑
store-to-ledger 绑定、proposal/vote objects、
round lock、height lock、role lock、outbox、QC objects/indexes 和 competing-QC
evidence。

本机 proposal/vote API 的顺序固定为：

```text
从 ledger 重验本机候选和 justify QC
-> 获取 seal-store 进程写锁
-> 检查 durable store/validator/safety binding
-> 完全相同的 role lock 返回原对象
-> 冲突 subject/candidate fail closed
-> 生成确定性签名
-> object + role lock + round lock + height lock + outbox
   写入同一个 RocksDB WriteBatch
-> WriteOptions::set_sync(true)
-> 完整 readback
-> 才向调用者返回可发送对象
```

因此写前崩溃不会向网络返回签名；同步写后、发送前崩溃可从 outbox 恢复相同对象；
重复调用返回完全相同的签名字节。ACK/outbox 清理尚未接入网络，且未来 ACK 绝不能
删除 safety lock。

v1 height lock 是故意保守的：同一 validator 可以在更高 round 为同一 block 再签，
但不能在同一 epoch/height 切换到竞争 block。未来如果引入标准 HotStuff unlock
规则，必须用新版本明确验证更高 justify/locked QC，不能把“round 更高”本身当作
解锁条件。

## 7. QC 持久化和冲突处理

QC 以 `qc_hash` 作为不可变 object key。索引分别按 subject、block 和
chain/epoch/height 保存排序、去重后的 QC hash 集合；绝不使用单值 height key
覆盖先前 QC。

同高度、同 round、不同 block 的两个有效 QC 必须：

```text
保留两个完整 immutable QC
保留 height index 中的两个 hash
写入 competing-QC evidence
不得更新 canonical pointer
不得按较大 hash 静默 fork-choice
```

上述 object/index/evidence 存储逻辑是为后续 authenticated remote-QC ingress 预留的
防御分支；当前 local-only 入口只能接受一个本机 AOEM-selected block，因此尚没有
可达的“第二个竞争 block QC”产品入口，也尚未形成对应端到端测试门。不同 round 的
竞争 QC 不自动等价于同 round equivocation，必须交由后续 locked-QC/fork-choice
协议判断。

当前本机 QC 写入口要求 candidate 仍是本机 AOEM-verified candidate，并要求对应
proposal object 已持久化。远端 QC/body 的 authenticated ingress 和独立执行验证尚未
接入，因此当前公开节点不会通过 RPC 注入 QC。

seal DB 与 candidate ledger 是两个独立 RocksDB，不存在跨库原子事务。本切片绝不
在 QC 落盘后跨库修改 candidate：后续 `proof_sealed`/canonical promotion 必须使用
单独的 durable promotion journal 和崩溃恢复状态机。

## 8. 已实现的测试门

定向测试覆盖：

1. validator 输入排序无关、duplicate/zero-weight 拒绝、4 等权 quorum=3；
2. 本机 AOEM candidate 生成 deterministic golden subject，重复和重启一致；
3. chain/root 等字段被篡改时 commitment 或 validation 失败；
4. observed candidate 不能构造本机签名 subject；
5. proposal/vote Ed25519 验证、非成员、坏签名和 duplicate vote 拒绝；
6. 2/4 QC 拒绝、3/4 QC 通过；不对称权重下按 weight 而不是签名数判定；
7. proposal/vote lock、outbox 和 QC/index 同步持久化，重启幂等返回原字节并
   重新反查 outbox 的 object/role/safety binding；
8. 同 candidate 新 round 允许、同高度竞争 candidate 被 durable height lock 拒绝；
9. 非创世 candidate 缺父 QC 拒绝，正确父 QC 可构造 child subject；
10. QC 持久化前后 candidate 的 proof/canonical/safe/finalized 全部保持 false；
11. 旧 consensus QC JSON 不能解码为 NOV seal QC；
12. read-only open 不创建不存在的 store。

以上重启测试覆盖完整同步提交后的恢复和幂等；尚未提供 RocksDB batch 前、sync 后
到 API return 前的进程级 fault-injection harness，不能把原子写设计本身表述成已经
完成所有崩溃窗口的实机注入测试。

## 9. 明确未完成

以下能力均为 `NOT IMPLEMENTED` 或 `NOT EXECUTED`，不得从本切片推导完成：

```text
authoritative on-chain validator epoch/set activation  NOT IMPLEMENTED
leader schedule / timeout vote / timeout certificate   NOT IMPLEMENTED
Product Overlay proposal/body/vote/QC transport        NOT IMPLEMENTED
canonical transport codec / bounded wire decoder       NOT IMPLEMENTED
observed candidate local replay promotion              NOT IMPLEMENTED
remote vote equivocation ingestion                     NOT IMPLEMENTED
HotStuff multi-phase / 3-chain commit rule              NOT IMPLEMENTED
fork choice / canonical promotion journal              NOT IMPLEMENTED
AOEM branch promotion / rollback / reorg                NOT IMPLEMENTED
candidate proof_sealed mutation                         NOT IMPLEMENTED
chain safe / finalized promotion                        NOT IMPLEMENTED
public four-machine / VPS / NAT / CGNAT / VPN run       NOT EXECUTED
Linux package smoke / self-hosted nightly soak          NOT EXECUTED
```

下一切片应定义 canonical bounded transport wire，把权威 validator snapshot 和
authenticated proposal/vote/QC transport 接入
Product Overlay，同时保持签名私钥只能经过本模块的 durable safety API。只有完成
远端 body/AOEM evidence 独立验证、leader/timeout/locked-QC 规则和跨库 promotion
journal 后，才允许讨论把 candidate 从独立 QC 提升为 `proof_sealed`。
