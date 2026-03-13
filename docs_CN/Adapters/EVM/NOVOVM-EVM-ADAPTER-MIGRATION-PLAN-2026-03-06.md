# NOVOVM EVM/Adapter 迁移方案与实施步骤（SUPERVM）- 生产主线版（2026-03-11）

## 1. 目标与边界

- 终局目标：`EVM Rust 全功能镜像节点`（不是兼容层终局）。
- 源规则基线：`D:\WEB3_AI\go-ethereum`。
- 架构边界：
  - 外部：允许 `HTTP/JSON-RPC`。
  - 内部：固定二进制流水线（`opsw1 -> novovm-node -> AOEM`）。
- 迁移原则：
  - 以“规则直迁/协议直迁”为主，尽量不做二次抽象和工程化包装。
  - 仅在与 `SUPERVM/AOEM` 高性能底座对接时做必要改造。

## 2. 核心原则（性能优先）

1. 不改主入口去适配插件，插件必须适配主入口和二进制内核。
2. D1/D2/D3/AOEM 内部不引入 RPC/HTTP/JSON 二次传输。
3. 非必要观测、门禁脚本、包装层不进入生产主线。
4. 进度判定只看生产代码接线与可复现实跑闭环。

## 3. 实施策略（按 go-ethereum 能力面拆解）

## 3.1 协议与执行面（先做）

- 交易类型与签名规则：按 go-ethereum 行为迁移。
- EVM 执行语义与状态变更：按链规则对齐。
- precompile/gas/错误码：按链配置与 fork 规则迁移。
- 结果要求：插件对外呈现与目标链一致的执行行为。

## 3.2 交易池与打包面

- txpool 入池、替换、排序、淘汰规则迁移。
- nonce、费用与冲突处理按以太坊语义实现。
- 结果要求：本地交易生命周期与以太坊节点一致。

## 3.3 网络与同步面

- P2P/discovery/sync 逐步迁移到 Rust 插件侧。
- 区块/交易传播、同步状态机按上游规则实现。
- 结果要求：节点具备真实镜像节点能力，而非仅 RPC 兼容。

## 3.4 查询与接口面

- `eth_*` 常用读写接口按上游语义补齐。
- 边界层保持 JSON-RPC 兼容，内部不改变二进制主线。
- 结果要求：钱包/SDK 可直接使用，且不牺牲内部性能。

## 3.5 节点收益与原子能力面

- 收益归集 -> 换汇 -> 发放：形成最小可运行闭环。
- 原子跨链 intent：本地检查通过后再广播。
- 结果要求：交易收入可对账，原子流程可控可审计。

## 4. 当前工作包（生产口径）

| WP | 名称 | 状态 | 代码锚点 | 下一步 |
|---|---|---|---|---|
| WP-01 | EVM 外部入口归一化与主线消费 | InProgress | `crates/gateways/evm-gateway/src/main.rs` + `crates/novovm-node/src/bin/novovm-node.rs` | 继续补齐 `eth_*` 生产接口与语义一致性。 |
| WP-02 | 插件执行主路径（apply_v2/self-guard） | InProgress | `crates/plugins/evm/plugin/src/lib.rs` | 强化执行语义与异常路径，减少分支包装。 |
| WP-03 | 内存 ingress 队列与策略数据面 | InProgress | `crates/plugins/evm/plugin/src/lib.rs` | 对齐真实 txpool/广播前策略消费。 |
| WP-04 | 收益归集/换汇/发放闭环 | InProgress | `crates/plugins/evm/plugin/src/lib.rs` | 补齐对账字段与宿主接线。 |
| WP-05 | 原子 intent 门控后广播 | InProgress | `crates/plugins/evm/plugin/src/lib.rs` | 固化门控条件与失败补偿。 |
| WP-06 | go-ethereum 能力直迁（网络/同步/txpool） | NotStarted | `D:\WEB3_AI\go-ethereum` 对照实现 | 从 txpool+sync 开始分模块直迁。 |

## 5. 完成定义（不再工程化）

以下条件同时成立才算“完成”：

1. 生产代码已接线到主路径。
2. 能在本地/集群复现真实运行闭环。
3. 行为与目标链规则一致（以 go-ethereum 为基线）。
4. 不依赖额外 gate 脚本或临时观测层才能成立。

## 6. 非目标（本阶段）

- 不把脚本产物通过当作主完成标准。
- 不在 `novovm-node` 内恢复多入口/多模式分叉。
- 不为“看起来完整”而增加高开销中间层。

## 7. 文档关系

- 进度台账：`NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md`
- 全功能镜像规范：`NOVOVM-EVM-FULL-MIRROR-NODE-MODE-SPEC-2026-03-11.md`
- 100% 收口清单：`NOVOVM-EVM-FULL-MIRROR-100P-CLOSURE-CHECKLIST-2026-03-13.md`
- 上游缺失能力对照：`NOVOVM-EVM-UPSTREAM-REQUIRED-CAPABILITY-CHECKLIST-2026-03-11.md`
- 边界铁律：`NOVOVM-EVM-PLUGIN-BOUNDARY-IRON-LAWS-2026-03-13.md`
