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

## 剩余阶段

| 阶段 | 目标 | 退出标准 |
| --- | --- | --- |
| v1 | 插件产品面协议可观察等价 | 本文 3 个聚合 gate 全绿 |
| v2 | 真 geth/reth 黑盒差分 | 同一批 raw tx/block 输入，state root、receipt root、logs bloom、gas/failure/RPC 输出一致 |
| v3 | 网络可观察等价 | devp2p/eth handshake、tx/block broadcast、import/reorg 行为被其它节点接受 |
| v4 | 长稳生产封口 | 多节点 devnet soak、重启恢复、恶意/边界输入、BAL/receipt/RPC 长稳无漂移 |

## 下一步

下一步不再继续堆官方 fixture 子集，除非某个 v2/v3 差分暴露具体缺口。

优先顺序：

1. 先让 v1 三个聚合 gate 进入固定回归清单。
2. 再做 v2：用 geth/reth 对照节点喂同一批 raw tx/block，比较 state root、receipt root、logs bloom、RPC 输出。
3. 最后做 v3：真实 eth/66-eth/71 peer handshake 和 import/broadcast 可观察行为。

这会把“等价”从开放式 fixture 堆叠改成有限的协议验收。
