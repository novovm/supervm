# NOVOVM 外部入口边界与二进制流水线架构约束（2026-03-09）

## 1. 目标与背景

`superVM` 是公链底座与统一结算/协议平台。

EVM、SVM、Solana、BTC、BNB 等链能力在 `superVM` 中以“插件/扩展能力”接入，而不是反向改造 `superVM` 内核入口。

核心目标：

1. 保持 `superVM + AOEM` 内部高速流水线与持久化性能上限。
2. 避免“每增加一个外部插件就修改内部主入口”的结构性耦合。
3. 将 `HTTP/JSON-RPC` 限定在外部边界层，仅用于生态兼容调用。

## 2. 分层原则（强约束）

### 2.1 外部边界层

仅在外部边界层接受：

- `HTTP`
- `JSON-RPC`
- 第三方 SDK/钱包/节点生态协议

边界层职责是“协议兼容与归一化”，不是内部执行主线。

### 2.2 内部执行层

`D1/D2/D3` 与 `AOEM` 之间禁止使用 `HTTP/JSON/RPC` 作为内部传输协议。

内部统一使用：

- 二进制结构
- AOEM 运行时 ABI
- 高速流水线数据通道（如 ops wire / binary IR）

### 2.3 插件适配层

插件适配器负责：

1. 将外部语义映射为内部统一语义（IR/二进制）。
2. 在进入 `superVM` 主链路前完成必要归一化。
3. 保持与 `AOEM` 的二进制接线，不在主链路做重复文本解析。

## 3. 入口稳定性规则

`novovm-node` 主入口是内部生产入口，不承担第三方生态协议兼容职责。

规则：

1. 不因新增插件（EVM/SVM/Solana/BTC/BNB 等）修改 `novovm-node` 内部主入口语义。
2. 外部协议新增需求，优先在边界层组件新增或扩展。
3. 插件升级依赖 registry/caps/hash/abi 治理，不依赖主入口改造。

## 4. 本次收敛决策（2026-03-09）

1. 回退将 `public_rpc` 直接并入 `novovm-node` 主入口的方案。
2. 明确“外部 RPC 在边界层、内部走二进制流水线”的架构边界。
3. 将该约束纳入 EVM/UA 迁移台账，作为后续迁移执行基线。

## 5. 后续实施清单（按优先级）

1. 建立独立边界层组件（网关/边车），承接 `eth_*` / `web30_*` 外部调用。
2. 网关到内核仅输出二进制 ingress（禁止文本协议穿透 D1/D2/D3）。
3. 在 adapter/plugin 内完成一次性解析与归一化，避免二次解析开销。
4. 为“入口稳定性规则”增加 CI 约束（防止插件需求反向污染主入口）。

## 6. 已落地实现（2026-03-09）

1. 边界层组件 `crates/novovm-edge-gateway` 已落地：对外受理 `ua_createUca` / `ua_bindPersona` / `eth_sendRawTransaction`，对内输出 `ops_wire_v1` 二进制。
2. `novovm-node` 生产 bin 保持唯一入口，并新增通用 `NOVOVM_OPS_WIRE_DIR` 批量消费能力（仅 `.opsw1`），用于承接边界层二进制落盘队列。
3. `NOVOVM_OPS_WIRE_DIR` 与 `NOVOVM_TX_WIRE_FILE` / `NOVOVM_OPS_WIRE_FILE` 互斥，避免入口语义歧义；`ops_wire_dir` 场景下禁止 `repeat` 压测语义混入生产消费语义。
4. 一键生产路径脚本已落地：`scripts/migration/run_gateway_node_pipeline.ps1`（边界网关 -> `.opsw1` -> `novovm-node`）。
5. gateway 已补齐 `eth_sendTransaction`（对象/`tx` 子对象/数组参数）与 `web30_sendTransaction` 的生产接线，统一落 `.opsw1` 并进入同一主链路消费。
6. gateway 已补齐 `eth_getTransactionByHash` / `eth_getTransactionReceipt` 查询入口；ETH 查询索引后端支持 `memory|rocksdb`（默认 `memory`），通过 `NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND` / `NOVOVM_GATEWAY_ETH_TX_INDEX_PATH` 控制。默认保持性能优先，必要时可切 rocksdb 获得重启后可查询能力。`eth_sendRawTransaction/eth_sendTransaction` 对外结果已收敛为标准哈希字符串，`eth_getTransactionCount` 收敛为标准 hex quantity，并补齐 `eth_chainId/net_version/eth_gasPrice/eth_estimateGas/eth_getCode/eth_getStorageAt` 边界兼容（均不进入内部 `.opsw1` 主线；其中 `eth_getCode/eth_getStorageAt` 当前提供 M0 占位只读返回）。
7. EVM 迁移脚本已支持“插件二进制优先、源码可选”模式：`run_evm_backend_compare_signal.ps1` 优先从外部二进制路径解析插件（支持 `-PluginPath`/`NOVOVM_EVM_PLUGIN_PATH`/`NOVOVM_ADAPTER_PLUGIN_PATH`），`run_evm_tx_type_signal.ps1` 可通过 `AllowPluginSourceTests` 关闭源码插件单测，以便主仓库开源时剥离插件源码而不破坏主线验证。
8. AOEM sidecar 插件目录支持“自适应热插拔”解析：`novovm-exec` 新增 `NOVOVM_AOEM_PLUGIN_DIRS`/`AOEM_FFI_PLUGIN_DIRS`（分号/逗号分隔）并按插件匹配度与最近修改时间自动选目录。该能力仅在启动阶段选择目录，不引入运行期轮询，保持主线性能稳定。

示例：

```powershell
.\scripts\migration\run_gateway_node_pipeline.ps1 -BuildBinaries $true
```

最小真实链路冒烟：

```powershell
.\scripts\migration\run_gateway_node_smoke.ps1
```
