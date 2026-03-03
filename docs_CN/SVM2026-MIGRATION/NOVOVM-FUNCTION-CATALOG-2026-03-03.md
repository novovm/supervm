# NOVOVM 功能分类与模块归属（基于 SVM2026 审计）- 2026-03-03

## 1. 分类目标

- 把历史能力拆成“底座能力 / 核心能力 / 扩展能力 / 应用能力”四类。
- 每项能力必须有 NOVOVM 目标模块归属。
- 明确迁移方式：`复用`、`重构`、`暂缓`。
- 将 `AOEM` 的 `ZK + MSM` 能力纳入核心对接清单（区块链生产必需能力）。

## 2. 功能分类矩阵

| 编号 | 能力域 | SVM2026 来源 | NOVOVM 目标模块 | 迁移方式 | 优先级 |
|---|---|---|---|---|---|
| F-01 | AOEM 执行入口 | `aoem/crates/core/*` | `novovm-exec` + `aoem-bindings` | 复用 | P0 |
| F-02 | AOEM 运行时配置 | `AOEM runtime profile` | `novovm-exec::AoemRuntimeConfig` | 复用 | P0 |
| F-03 | 执行回执标准 | `supervm-node + vm-runtime` | `novovm-protocol`（已落地骨架） | 重构 | P0 |
| F-04 | 状态根一致性 | `vm-runtime/state_db` | `novovm-protocol`（已落地骨架） | 重构 | P0 |
| F-05 | 共识引擎（核验约80%） | `supervm-consensus` | `novovm-consensus`（已落地骨架） | 复用后重命名 + 收口 | P1 |
| F-06 | 分布式协调 | `supervm-distributed`/`supervm-dist-coordinator` | `novovm-coordinator`（规划） | 重构 | P1 |
| F-07 | 网络层（核心完成，生产待收口） | `supervm-network` + `src/l4-network` | `novovm-network`（已落地骨架 + `UdpTransport`） | 重构 + 收口 | P1 |
| F-08 | Chain Adapter 接口 | `supervm-chainlinker-api` | `novovm-adapter-api`（契约）+ `novovm-adapter-novovm`（native）+ `novovm-adapter-sample-plugin`（plugin） | 复用后裁剪 | P1 |
| F-09 | zk 执行与聚合 | `src/l2-executor` | `novovm-prover`（规划） | 重构 | P1 |
| F-10 | Web3 存储服务 | `src/web3-storage` | `novovm-storage-service`（规划） | 重构 | P2 |
| F-11 | 域名系统 | `src/domain-registry-sdk` | `novovm-app-domain`（规划） | 暂缓到生态层 | P3 |
| F-12 | DeFi 核心 | `src/defi-core` | `novovm-app-defi`（规划） | 暂缓到生态层 | P3 |
| F-13 | 多链插件能力 | `plugins/*` | `novovm-adapters/*`（规划） | 暂缓（最后） | P4 |
| F-14 | 历史 vm-runtime 杂糅能力 | `src/vm-runtime/*` | 分拆到 D2/D3/D4 | 重构优先，不整体迁 | P1 |
| F-15 | AOEM ZK 能力契约（prove/verify） | `crates/optional/zkvm-executor` + `aoem-runtime-cli(zkvm-executor feature)` | `novovm-prover`（规划）+ `novovm-exec` 能力探测面 | 重构为稳定契约 | P0 |
| F-16 | AOEM MSM 加速能力（BLS12-381） | `aoem-engine` + `aoem-ffi`（`BlsMsmBackend/BlsMsmDecision`） | `novovm-prover`（规划）+ `novovm-exec` 能力探测面 | 复用并标准化输出 | P0 |

## 3. 可先完成项（不做“逐项能力迁入”）

这些任务可立即做，且不违反“逐项迁入最后做”的策略：

1. 冻结 `novovm-*` 目标模块边界与 crate 命名。
2. 固化执行结果契约（`state_root`、`receipt_hash`、`error_code`、`metrics`）。
3. 接通已有一致性/性能脚本与新契约字段。
4. 建立迁移台账（每个能力单独验收记录，不再写大而全进度百分比）。
5. 统一核心发布口径（D0-D4）与生态口径（D5）。
6. 冻结 `ZK/MSM` 能力契约字段（能力探测、回退原因码、性能指标口径）。

## 3.1 ZK+MSM 最小契约（建议）

`novovm-exec` 对外最少应提供以下能力字段：

- `zkvm_prove` / `zkvm_verify`
- `msm_accel` / `msm_backend`
- `fallback_reason`
- `proof_ms` / `verify_ms` / `msm_ms`

## 3.2 共识/网络核验口径（2026-03-03）

- 共识层：按迁移生产口径约 `80%`（核心可运行，仍有批量验证收口项）。
- 网络层：核心功能已完成并通过主体测试，按生产封盘口径建议 `90~95%`。
- 证据文档：`SVM2026-LAYER-STATUS-VERIFIED-2026-03-03.md`。

## 3.3 Phase B 自动化进展（2026-03-03）

- `state_root`：已在一致性报告引入 `state_root_consistency` 标准字段；当前 AOEM FFI 未暴露真实 `state_root`，暂以 `proxy_digest` 代理门禁。
- baseline：已新增 `scripts/migration/import_svm2026_baseline.ps1`，可将 `SVM2026` TPS 证据转换为 `run_performance_compare.ps1` 基线 JSON。
- 台账：已新增 `scripts/migration/generate_capability_ledger_auto.ps1`，可自动回填报告证据路径与状态快照。

## 3.4 迁移批次路线（Batch A-E）

| Batch | 目标闭环 | 对应能力 | 当前状态 |
|---|---|---|---|
| A | 交易入口 -> `ops_v2` -> AOEM 执行 -> 状态提交 -> 批次输出（最小真链） | F-01/F-03/F-04 + F-05 最小接线 | InProgress（已接入 `tx_codec_signal` / `mempool_admission_signal` / `tx_metadata_signal` / `batch_a_input_profile` / `batch_a_closure` / `block_wire_signal` / `block_output_signal` / `commit_output_signal`，并将 tx wire codec 下沉到 `novovm-protocol::tx_wire`、block header wire 下沉到 `novovm-protocol::block_wire`；当前口径 `accounts=2`、`fee=1~5`、`demo_txs=8`、`target_batches=2`、`block_out.batches=2`） |
| B | 共识与终局（执行-共识解耦） | F-05 | InProgress（已落 `novovm-consensus` 骨架） |
| C | P2P / gossip / 同步 | F-07 + F-08 | InProgress（已落地 `novovm-network` + Adapter 双后端：`native-first + plugin-optional`；`adapter_signal` 已接入 `backend` 维度，并支持 `NOVOVM_ADAPTER_BACKEND=auto|native|plugin`、`NOVOVM_ADAPTER_CHAIN`、`NOVOVM_ADAPTER_PLUGIN_PATH`；新增插件 ABI 门禁 `NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI` / `NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS`、注册表门禁 `NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH` / `NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT` / `NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256`（配合 `allowed_abi_versions`），以及 `adapter_plugin_abi_signal` / `adapter_plugin_registry_signal`；并增加 `adapter_plugin_abi_negative_signal`（ABI/caps mismatch）、`adapter_plugin_symbol_negative_signal`（坏插件/缺符号）与 `adapter_plugin_registry_negative_signal`（hash/whitelist mismatch）负例门禁；新增共识绑定信号 `adapter_consensus_binding_signal`（`plugin_class=consensus` + `consensus_adapter_hash`），并将 `consensus_adapter_hash` 写入区块头并在提交阶段强校验（`block_consensus` / `commit_consensus`）；当前 mesh 压力口径 `NodeCount=3`、`Rounds=2`、`pairs=6/6`、`directed=12/12`，并新增跨进程 `network_block_wire`（`novovm_block_header_wire_v1` + `consensus binding`）校验通过（`12/12`），且新增 `network_block_wire_negative_signal`（篡改 payload 负例必须失败，`verified=0/2`）；`adapter_signal` 已覆盖 `backend=native/plugin`，并新增 `adapter_backend_compare_signal` 覆盖 native/plugin 同输入对照） |
| D | ZK 证明路径（prover/verifier） | F-15/F-16 | InProgress |
| E | RPC / CLI / DevEx | F-10~F-13（裁剪后） | NotStarted |

## 4. 不建议做法

- 把 `src/vm-runtime` 作为整体搬到 NOVOVM。
- 在多个 crate 直接加载 AOEM DLL，绕过 `novovm-exec`。
- 在规划阶段继续沿用 `ROADMAP.md` 的历史百分比作为唯一决策依据。
- 把 `ZK` 或 `MSM` 路由逻辑硬编码进 D0 内核，破坏 AOEM 能力边界。

## 5. 迁移准入标准（按能力项）

每个能力进入“开始迁移”前必须同时满足：

1. 有目标模块归属（谁接收）。
2. 有输入输出契约（怎么接）。
3. 有回归脚本（怎么验）。
4. 有失败回退路径（怎么撤）。
