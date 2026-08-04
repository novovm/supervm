# NOVOVM Unsealed Block Candidate Graph V1

状态：活动实现契约；仅覆盖 Proof-Sealed BFT Finality 路线的 Slice 1，
不构成 proof seal、链级 canonical 或主网最终性签收。

相关基础契约：
[`NOVOVM_VERIFIABLE_BLOCK_CANDIDATE_DURABLE_LEDGER_V1.md`](NOVOVM_VERIFIABLE_BLOCK_CANDIDATE_DURABLE_LEDGER_V1.md)

## 1. 目标和信任边界

本切片在既有 AOEM-owned 本地执行账本旁边增加一个持久候选图，使节点能够：

- 保留同一高度的多个完整候选；
- 查询候选的父子关系和同高度竞争关系；
- 区分本机 AOEM 权威执行结果与从其他节点观察到的结构化候选；
- 对未选中分支写入可审计、重启后仍存在的 abort tombstone；
- 在不污染既有本地执行投影的前提下，为后续验证、投票和 fork choice
  准备确定性输入。

候选图的所有响应和记录都属于：

```text
trust_class = unsealed_candidate_graph
chain_canonical = false
proof_sealed = false
safe = false
finalized = false
```

`commitment_bindings_verified = true` 只表示候选的 header、body、receipt
commitment、execution evidence 和 hash 在结构上互相闭合。它不表示执行已由
接收节点重放，不表示 AOEM 状态已在接收节点落账，也不表示任何验证者已经投票。

## 2. Sidecar keyspace：与本地执行投影严格隔离

候选图是现有 `novovm-native-block-ledger/v1` 的独立扩展，不替换原有线性
执行账本。它使用独立 schema 和 keyspace：

```text
native_block_ledger/v1/extension/candidate_graph/v1/schema

native_block_ledger/v1/chain/<chain_id>/candidate_graph/v1/
  record/<block_hash>
  artifact/<block_hash>
  height/<height>
  children/<parent_block_hash>
```

其中：

- `record` 保存候选来源、验证级别和可变生命周期元数据；
- `artifact` 保存 observed 候选的完整不可变 block artifact；
- `height` 保存同一高度按 hash 排序、去重后的候选集合；
- `children` 保存同一 parent 下按 hash 排序、去重后的子候选集合。

本机 AOEM 执行候选的不可变 artifact 继续复用既有 header/body/evidence
存储；sidecar 只登记它的候选记录和图索引。数据库升级采用兼容读取：旧数据库
中已有的本地执行块可以合成等价 local record，后续重复提交可原子回填图记录。

注册 observed 候选只能写上述 sidecar。它不得写入或推进：

```text
execution_head
legacy height -> selected local block
tx_hash -> selected local transaction location
receipt location
AOEM batch/result external-id index
AOEM authoritative state or receipt store
```

因此，接收到一个更高、同高或 hash 更大的候选，都不能改变本机 AOEM 执行
head，也不能让旧查询把该候选误报为本机已执行交易。只有本机 AOEM-owned
commit 路径可以更新旧 head、height、tx、receipt 和 external-id 索引；该路径
同时原子登记对应的 local graph record。

## 3. 不可变 artifact 与可变生命周期记录

完整 block artifact 由 header、ordered body 和 execution evidence 组成。
它以 `block_hash` 标识；任何 context、parent、body、交易顺序、root、receipt
commitment 或 AOEM evidence 绑定变化，都必须产生另一个 hash 或验证失败。

候选生命周期不得通过重写 immutable header 表达。sidecar record 单独保存：

```text
source and lifecycle status
revision and optional abort_reason
height / slot / timestamp / parent / block hash
body, transaction, state, receipt and evidence commitments
commitment_bindings_verified
parent_continuity_verified
body_data_available
local_aoem_readback_verified
execution_selected_local
fork_choice_selected
chain_canonical / proof_sealed / safe / finalized
```

Slice 1 中只有 `active_unsealed -> aborted_unsealed` 是允许的生命周期变化。
变更只增加 `revision` 并写入 tombstone；artifact、height index 和 children
index 都保留，保证审计和重启恢复能够看见历史事实。

## 4. 两类候选不能混淆

### 4.1 `local_aoem_owned_execution`

该来源只能由本机既有 AOEM-owned 执行、持久化和 readback 闭环产生：

```text
local_aoem_readback_verified = true
execution_selected_local = true
```

这仍然只表示本机线性执行投影被选择。它不是网络 fork choice，也不能将
`chain_canonical`、`proof_sealed`、`safe` 或 `finalized` 设为 `true`。

### 4.2 `observed_unsealed_candidate`

该来源表示从其他节点取得了一个完整、结构上自洽且父子连续的 artifact：

```text
commitment_bindings_verified = true
parent_continuity_verified = true
body_data_available = true
local_aoem_readback_verified = false
execution_selected_local = false
```

observed artifact 可以携带生产者自报的 `aoem_readback_verified`、AOEM batch
id、result id 和 evidence commitment。接收节点只能检查这些值是否被 candidate
hash 和 artifact 内部字段一致地绑定，**不得**因此把
`local_aoem_readback_verified` 设为 `true`。后续验证者投票前，必须在自己的
权威验证路径中取得或重放父状态、核验业务状态转换与 AOEM evidence。

旧 header 中的 `canonical_local = true` 描述的是该 artifact 在生产节点的本地
执行形式，是既有 immutable header schema 的生产者局部元数据。它不表示接收
节点选择了该分支；接收节点的有效判断必须以 sidecar record 的
`execution_selected_local = false` 和四个链级 false 标志为准。
按 hash 返回完整 artifact 的查询还会显式标记
`artifact_header_canonical_local_scope = producer_local_claim_not_receiver_selection`，
防止客户端把生产者局部字段误读为接收节点选择或链级 canonical。

## 5. 同高度竞争与父子连续性

候选图允许同一个活动 parent 产生多个下一高度候选，并将它们全部保存在
height/children 索引中。索引以 block hash 严格排序、去重，并受 4,096 条链接
的防御上限约束；该上限是资源保护，不是协议规模或吞吐声明。

observed 候选准入必须 fail closed：

1. 完整 artifact 的 schema、hash、body、roots、receipt 和 evidence 绑定有效；
2. `chain_id`、height、slot、timestamp 和 parent 字段有效；
3. 非 genesis 候选的 parent record 和完整 parent artifact 已存在；
4. parent 必须仍为 `active_unsealed`；
5. child height、parent hash、pre-state/AOEM parent continuity 与 parent 完全一致；
6. 竞争 genesis 不被本切片接受；
7. 相同 block hash 的重复登记只有在 artifact 完全一致时才幂等成功，冲突
   artifact 必须拒绝。

登记竞争候选不是 fork choice。Slice 1 不按照到达顺序、slot、最高高度、hash、
AOEM id 或本地时钟自动选择分支，`fork_choice_selected` 必须保持 `false`。

## 6. Abort 是 tombstone，不是状态回滚

`abort_unselected_candidate_branch` 只允许处理未选中、未封印的 observed
候选。操作从指定 root 开始，沿 children 索引递归覆盖全部后代，最大遍历
4,096 个候选节点，并在一个同步 Host ledger batch 中写入：

```text
lifecycle_status = aborted_unsealed
revision = revision + 1
abort_reason = validated non-empty reason
```

它必须拒绝任何包含以下状态的目标或后代：

```text
execution_selected_local = true
fork_choice_selected = true
chain_canonical = true
proof_sealed = true
safe = true
finalized = true
```

重复 abort 是幂等读取，不能删除 artifact 或覆盖首次 tombstone 的审计事实。
被 abort 的 parent 不能再接受新 child。重启后，record、原因、revision、完整
artifact 和图索引都必须保持可查询。

这里的 abort 不是 reorg，也不是 AOEM rollback。当前 NOV 集成推进的是一条
本机 AOEM authoritative latest-state 执行投影；候选图没有 AOEM 历史 snapshot
checkout、branch promotion 或原子 revert 原语。只回退 Host 的 head/index 而不
同时回退 AOEM 权威状态，会造成双重真相，因此严格禁止。

真实 rollback/reorg **NOT IMPLEMENTED**。它至少需要：

- AOEM 侧可验证、可持久化的 prepared/committed state version 或 snapshot；
- candidate hash 与 AOEM state version 的双向绑定；
- AOEM promotion/rollback 与 Host canonical pointer/index 的崩溃一致事务协议；
- 对已晋升分支的 transaction、receipt、nonce 和查询投影重建规则；
- 每个崩溃点的幂等恢复和冲突隔离测试。

在这些能力完成前，不能通过修改 Host 标志模拟 reorg。

## 7. 只读查询面

本切片增加以下只读查询：

```text
nov_getNativeBlockCandidatesByNumber
nov_getNativeBlockCandidateByHash
nov_getNativeBlockCandidateChildren
```

查询分别返回同高度候选记录、候选记录及完整 artifact、指定 parent 的直接
children。空数据库和未找到结果也必须显式返回 `trust_class` 和四个 false
最终性标志，不能依靠字段缺失暗示状态。

公网 RPC 不开放 observed registration 或 abort mutation。本切片的 mutation
入口属于可信节点内部生命周期；候选传播、身份认证、速率/大小限制、DA 获取和
validator 权限将在后续网络接入切片定义。

## 8. 现有 consensus/QC 不能直接充当 proof seal

仓库已有 `novovm-consensus` 的 BFT、vote、QC、validator weight、equivocation
和 fork-choice 原语，可以作为后续实现材料，但现有 QC wire 不是 NOV block
proof seal 协议。

当前 vote 的签名消息只绑定通用 `proposal_hash + height`，QC 记录也主要包含
`proposal_hash`、`height`、votes 和 total weight。现有 pre-commit 路径还保留
“假设已验证”的迁移实现。它没有独立、版本化地证明以下 NOV 候选字段及验证
前置条件：

```text
chain_id
height / slot / parent_block_hash
block_hash / ordered_tx_root / body or DA root
pre_state_root / post_state_root
block_receipt_root / cumulative_receipt_root
AOEM execution evidence commitment
validator_set_hash and epoch
protocol_version / proof_version / signature_domain
body_data_available and local execution verification result
```

因此，不能把一个旧 `QuorumCertificate` 写入 sidecar 后就设置
`proof_sealed = true`。后续可以复用经过审计的签名验证、权重计算、重复投票和
equivocation 检测实现，但必须先定义 NOV 专用、域分离、版本化的 seal
commitment，并让每一票签署该 commitment。QC 证明法定人数对同一消息作证；
它本身不替代候选 body/DA 获取、NOV 业务执行验证或 AOEM evidence 验证。

## 9. Slice 1 验收标准

实现只有在以下正反向测试均通过时才可签收：

1. 本机 AOEM commit 原子登记 local record，来源和两个本机验证字段为 true；
2. 同一 parent 的两个同高度候选可以共存，height 和 children 查询稳定排序；
3. observed 登记不改变旧 execution head、height、tx、receipt、external-id
   索引或 AOEM 状态；
4. observed 自报 AOEM readback/evidence 不会把
   `local_aoem_readback_verified` 变成 true；
5. 错误 hash、body/root/evidence、缺失/aborted parent、错误 height/pre-state
   continuity 和竞争 genesis 全部拒绝；
6. 递归 abort 保留 artifact/index，重启后 tombstone 和后代状态一致；
7. selected local、fork-choice selected 或任何 sealed/finality 状态不能走普通
   abort；
8. 重复登记和重复 abort 幂等，hash 相同而 artifact 冲突时 fail closed；
9. 旧数据库在没有 candidate-graph schema 时仍可读取本地执行块，且不会虚构
   observed 分支或最终性；
10. 所有候选记录与查询中的 `chain_canonical`、`proof_sealed`、`safe`、
    `finalized` 始终为 false。

## 10. 下一切片：Proof-Seal/QC 的入口条件

Slice 2 才能新增 proposer/validator vote/QC。它必须以本候选图中的 immutable
`block_hash` 为对象，并至少完成：

```text
authenticated proposal and bounded body/DA acquisition
candidate artifact commitment verification
parent and validator-set/epoch verification
independent NOV execution and AOEM evidence verification before vote
domain-separated, versioned seal message
weighted quorum and signature verification
equivocation persistence and restart recovery
durable QC bound to exactly one candidate hash
```

Slice 2 的完成最多允许把经过完整验证和有效 QC 的候选标记为
`proof_sealed = true`。`chain_canonical`、`safe` 和 `finalized` 的晋升、同高度
双 QC 处理、fork choice、AOEM state promotion、reorg/rollback 与跨崩溃点恢复，
仍属于后续 canonical-promotion 切片，不能在 QC 写入时顺带宣称完成。
