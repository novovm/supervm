# NOVOVM EVM 协议可观察等价 v1 收口标准（2026-06-07）

## 定义

这里的“等价”不是代码等价 geth，也不是把 SUPERVM 做成完整以太坊全节点。

目标定义为：

`同一链配置、同一交易/区块输入下，外部系统通过协议、RPC、receipt、log、gas、failure、BAL、block metadata 等可观察结果，无法区分 SUPERVM EVM 插件和以太坊 EVM 语义实现。`

因此可接受：

- SUPERVM 用 Rust 实现，geth 用 Go 实现。
- 内部执行、调度、AOEM artifact、存储结构可以不同。
- 产品面是 Novo 主网可控 EVM 插件，不是替代 geth 的全节点二进制。

不可接受：

- raw tx 验签、gas、fee、nonce、receipt/log、失败分类在可观察结果上偏离目标 fork 语义。
- block metadata、BAL、receipt ingest/export 在插件边界缺失或不可复验。
- 对外宣称完整 geth/full-node 等价，但没有 p2p peer sync、block import、state root/devnet 差分证据。

## v1 自动验收门禁

v1 不再继续按“每轮一个 fixture 子集”推进，而是把已具备的生产路径证据收口为 3 个聚合 gate：

```powershell
cargo test -p novovm-adapter-novovm evm_protocol_observable_equivalence_execution_gate_v1 -- --nocapture
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_fixture_gate_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin evm_protocol_observable_equivalence_plugin_receipt_bal_gate_v1 -- --nocapture
```

这 3 个 gate 覆盖：

- execution observable：raw tx、typed tx、fee/value debit、failure/revert/OOG、EIP-1559、access-list、SLOAD/SSTORE、CREATE/CREATE2、official state fixture projection subset、receipt/log/gas baseline。
- RPC/geth fixture observable：内置 geth parity fixture 和文件驱动 geth parity fixture，覆盖 block、receipt、logs、typed failure、canonical/noncanonical log ownership。
- plugin observable：receipt export/ingest、state mirror update、block metadata、contract call/deploy BAL completeness/hash、fee settlement、txpool replacement/reject。

## 当前 v1 口径

可以声明：

`NOVOVM/SUPERVM 当前具备 Novo 主网可控 EVM 插件的协议可观察等价 v1 门禁：执行语义、geth ethapi 样本 parity、receipt/log/BAL/plugin metadata 生产路径均有自动回归。`

不能声明：

`SUPERVM 是完整 geth 替代品。`

`SUPERVM 已作为 eth/71 p2p 全节点被其它 Ethereum 节点长期稳定识别为等价 peer。`

`SUPERVM 已通过 Ethereum execution-spec 全量 state tests 或 mainnet block replay。`

## v2 当前推进：RPC 黑盒投影根门禁

本轮 v2 不再继续增加官方 fixture 子集，而是先修复可被外部 RPC 客户端直接观察到的区块根字段：

- `eth_getBlockByNumber/eth_getBlockByHash` 的 `transactionsRoot` 不再返回 `null`，改为基于 canonical receipt projection 的 MPT 32-byte root。
- `eth_getBlockByNumber/eth_getBlockByHash` 的 `receiptsRoot` 不再返回 `null`，改为基于 receipt status、cumulative gas、logsBloom、logs 的 MPT 32-byte root。
- geth parity report 新增 `observableProjection` section，锁住 `transactionsRoot`、`receiptsRoot`、`stateRoot`、`gasUsed`、`cumulativeGasUsed`、`logsBloom`。
- 默认 11 个 geth parity 样本当前 `totalMismatchCount=0`，`observableProjection.mismatchCount=0`。

v2 projection gate：

```powershell
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_blackbox_projection_gate_v2 -- --nocapture
```

当前 v2 口径可以声明：

`SUPERVM EVM RPC/geth parity 样本已具备 block/receipt/log/gas/root projection 的黑盒可观察一致性门禁，避免区块对象根字段因 null 被外部客户端直接区分。`

当前 v2 不能声明：

`transactionsRoot/receiptsRoot 已等同真实 mainnet raw transaction trie root。`

`stateRoot 已完成 geth/reth devnet 同输入 replay 对齐。`

## v2b 当前推进：真实 geth block fixture 差分

本轮已接入一个真实 go-ethereum `ethapi/testdata` fullTx block fixture，新增 v2b 差分 gate：

```powershell
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_real_block_diff_gate_v2b -- --nocapture
```

该 gate 做两件事：

- 从 geth fullTx block fixture 的 legacy tx 字段复算 raw transaction RLP trie root，并确认复算值等于 geth fixture 的 `transactionsRoot`。
- 用同一 block/receipt 形态构造 SUPERVM canonical projection，和 geth block 对比 `number`、`gasUsed`、`logsBloom`、`receiptsRoot`、`stateRoot`、`transactionsRoot`。

当前结果：

- 已匹配：`number`、`gasUsed`、`logsBloom`、`receiptsRoot`、`stateRoot`。
- 已暴露 gap：`transactionsRoot`。原因是 SUPERVM 当前 canonical projection 还没有把 raw tx RLP 带入 block root 计算，仍使用 receipt/tx-hash projection root。

这一步的意义是把 v2b 从口头目标变成可运行的真实 geth block 差分报告。下一步不是继续加 fixture，而是把 raw tx RLP 或 native header `transactionsRoot` 接入 canonical block projection 路径。

## 剩余阶段

| 阶段 | 目标 | 退出标准 |
| --- | --- | --- |
| v1 | 插件产品面协议可观察等价 | 本文 3 个聚合 gate 全绿 |
| v2a | RPC 黑盒投影根门禁 | `evm_protocol_observable_equivalence_geth_rpc_blackbox_projection_gate_v2` 全绿 |
| v2b | 真 geth/reth 黑盒差分 | 已有真实 geth block diff gate；退出还需 raw tx RLP/native txRoot 接入后 `transactionsRoot` 也一致 |
| v3 | 网络可观察等价 | devp2p/eth handshake、tx/block broadcast、import/reorg 行为被其它节点接受 |
| v4 | 长稳生产封口 | 多节点 devnet soak、重启恢复、恶意/边界输入、BAL/receipt/RPC 长稳无漂移 |

## 下一步

下一步不再继续堆官方 fixture 子集，除非某个 v2/v3 差分暴露具体缺口。

优先顺序：

1. 先让 v1 三个聚合 gate 进入固定回归清单。
2. 已完成 v2a：RPC block root projection 不再返回 `null`，并进入 geth parity batch report。
3. 已推进 v2b：真实 geth fullTx block fixture 差分已接入，当前唯一明确 gap 是 `transactionsRoot` 缺 raw tx RLP。
4. 下一步补 raw tx RLP/native header txRoot 进入 canonical block projection，让 `transactionsRoot` 从 gap 变成 match。
5. 最后做 v3：真实 eth/66-eth/71 peer handshake 和 import/broadcast 可观察行为。

这会把“等价”从开放式 fixture 堆叠改成有限的协议验收。
