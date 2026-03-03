# NOVOVM 新全景架构（替代旧五层图）- 2026-03-03

## 1. 架构原则

- 内核底座化：AOEM 作为独立执行底座（Binary + Manifest + Runtime Profile）。
- 接口单一化：宿主侧只经 `novovm-exec` 进入执行路径。
- 核心与生态解耦：核心发布与应用生态分开打包与评估。
- 迁移后兼容：允许历史 `supervm-*` 模块阶段性并存，但目标命名统一 `novovm-*`。

## 2. NOVOVM 六域全景图

```text
┌────────────────────────────────────────────────────────────────────────────┐
│ D5 应用生态域（可选发布）                                                 │
│ Domain Registry / DeFi / Browser / SDK / CLI / 行业应用                   │
└────────────────────────────────────────────────────────────────────────────┘
                                  │
┌────────────────────────────────────────────────────────────────────────────┐
│ D4 扩展服务域（核心可插拔）                                                │
│ ZK Prover Service / MSM Acceleration Service / Web3 Storage / Adapters    │
└────────────────────────────────────────────────────────────────────────────┘
                                  │
┌────────────────────────────────────────────────────────────────────────────┐
│ D3 共识网络域（核心发布）                                                  │
│ Consensus / P2P / Shard Coordinator / Mempool / Block Propagation         │
└────────────────────────────────────────────────────────────────────────────┘
                                  │
┌────────────────────────────────────────────────────────────────────────────┐
│ D2 协议核心域（核心发布）                                                  │
│ Tx Lifecycle / Gas & Fees / State Root / Block Builder / Governance       │
└────────────────────────────────────────────────────────────────────────────┘
                                  │
┌────────────────────────────────────────────────────────────────────────────┐
│ D1 执行门面域（核心发布）                                                  │
│ novovm-exec + aoem-bindings + RuntimeConfig + Error/Metric/Capability     │
└────────────────────────────────────────────────────────────────────────────┘
                                  │
┌────────────────────────────────────────────────────────────────────────────┐
│ D0 AOEM 底座域（核心发布）                                                 │
│ AOEM Engine / Runtime / Persistence / Variants(core|persist|wasm)         │
└────────────────────────────────────────────────────────────────────────────┘
```

## 3. 核心发布范围

- 核心发布仅包含：`D0 + D1 + D2 + D3 + D4`
- 可选生态：`D5`

这等价于旧口径中的“核心与生态分离”，但边界更清晰，且 AOEM 被明确下沉为独立底座。

## 4. 与旧五层映射关系

| 旧层 | 旧定位 | 新域映射 | 说明 |
|---|---|---|---|
| L0 | 内核执行 | D0 + D1 + D2（拆分） | 旧 L0 中的执行/接口/协议职责拆开 |
| L1 | 接口层 | D1 + D4 | 统一门面与插件接口分离 |
| L2 | zk 执行 | D4 | 作为扩展服务接入核心 |
| L3 | 应用层 | D5 | 保持可选生态属性 |
| L4 | 网络层 | D3 + D4 | 共识网络与存储服务解耦 |

## 5. 关键边界（必须遵守）

1. D2/D3 不直接调用 AOEM FFI；必须走 D1 门面。
2. D4 插件不侵入 D0/D1 内核路径。
3. D5 应用不携带共识与底层执行逻辑。
4. 状态根、回执、性能指标口径由 D2 统一定义，D1 负责采集与透传。
5. ZK 与 MSM 能力均通过 D1 能力契约暴露，D4 不直连 AOEM 私有符号。

## 6. 生产版最小可用形态（MVP）

- D0：AOEM core/persist 变体可用，manifest 校验启用。
- D1：`submit_ops` + 统一错误码 + 统一指标输出。
- D2：交易生命周期、状态根汇总、块内回执标准化。
- D3：单网络域共识可运行，具备基础观测能力。
- D4：至少接入一个证明服务或存储服务能力。

达到上述形态后，再进入历史能力逐项迁入阶段。

## 7. ZK+MSM 落位说明

- `ZK` 与 `MSM` 都是区块链生产必需能力。
- 在 NOVOVM 中，二者归属 `D4`（扩展服务编排）并通过 `D1`（统一能力契约）接入。
- AOEM 继续作为能力提供底座，不承载宿主路由策略。

## 8. Adapter 双后端策略（Native First + Plugin Optional）

- 契约层（`novovm-adapter-api`）：仅保留 `TxIR/StateIR/ChainAdapter` 语义定义，不放实现。
- 原生后端（`novovm-adapter-novovm`）：作为生产默认路径（低开销，主链路）。
- 插件后端（`novovm-adapter-sample-plugin`）：通过稳定 C ABI 动态加载，供外部交付与可替换实现。
- 插件分类：
  - `consensus`（共识关键）：影响 `state_root`/交易有效性，必须受共识约束。
  - `local`（本地扩展）：仅本地策略/体验，不进入区块有效性判断。
- 当前 `adapter` 路径固定归类为 `consensus`，并输出 `adapter_consensus` 信号（`plugin_class` + `consensus_adapter_hash`）。
- 选择策略：
  - `NOVOVM_ADAPTER_BACKEND=auto`：默认，原生优先，必要时回退插件。
  - `NOVOVM_ADAPTER_BACKEND=native`：强制原生。
  - `NOVOVM_ADAPTER_BACKEND=plugin`：强制插件（需 `NOVOVM_ADAPTER_PLUGIN_PATH`）。
- 观测要求：`adapter_out` 必须包含 `backend` 字段，确保 native/plugin 证据可追踪。
- 共识绑定：`consensus_adapter_hash` 写入 block header（`block_consensus`），提交时强校验（`commit_consensus`），不匹配拒块。
- 协议化落点：block header 编解码下沉到 `novovm-protocol::block_wire`（`novovm_block_header_wire_v1`），提交前先 decode 再做共识绑定校验。

## 9. 编解码与 JSON 边界（生产铁律）

- 运行时关键路径（`tx -> adapter -> AOEM -> state_root`）必须保持二进制协议，不允许 JSON 编解码进入热路径。
- 插件 ABI、网络传输、交易编解码、AOEM FFI 均使用二进制接口（bytes + 固定字段语义）。
- JSON 仅允许用于：启动配置、插件 registry、调试日志、证据报告（`functional-consistency.json` 等）。
- 插件 registry 仅在启动/切换时加载，不参与单笔交易执行循环。
