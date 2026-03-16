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
| F-06 | 分布式协调 | `supervm-distributed`/`supervm-dist-coordinator` | `novovm-coordinator`（已落地骨架 + 2PC smoke） | 重构 | P1 |
| F-07 | 网络层（核心完成，生产待收口） | `supervm-network` + `src/l4-network` | `novovm-network`（已落地骨架 + `UdpTransport`） | 重构 + 收口 | P1 |
| F-08 | Chain Adapter 接口 | `supervm-chainlinker-api` | `novovm-adapter-api`（契约）+ `novovm-adapter-novovm`（native）+ `novovm-adapter-sample-plugin`（plugin） | 复用后裁剪 | P1 |
| F-09 | zk 执行与聚合 | `src/l2-executor` | `novovm-prover`（已落地骨架 + contract smoke） | 重构 | P1 |
| F-10 | Web3 存储服务（按裁剪口径） | `src/web3-storage` | `novovm-node` query/rpc + governance audit persistence（已落地） | 重构（裁剪口径已闭环） | P2 |
| F-11 | 域名系统（按裁剪口径） | `src/domain-registry-sdk` | `novovm-consensus` governance domain policy（已落地） | 重构（裁剪口径已闭环） | P3 |
| F-12 | DeFi 核心（按裁剪口径） | `src/defi-core` | `novovm-consensus` token economics + treasury spend + market governance（已落地） | 重构（裁剪口径已闭环） | P3 |
| F-13 | 多链插件能力（按裁剪口径） | `plugins/*` | `novovm-adapter-api` + `novovm-adapter-novovm` + `novovm-adapter-sample-plugin`（已落地） | 重构（裁剪口径已闭环） | P4 |
| F-14 | 历史 vm-runtime 杂糅能力 | `src/vm-runtime/*` | 分拆到 D2/D3/D4 | 重构优先，不整体迁 | P1 |
| F-15 | AOEM ZK 能力契约（prove/verify） | `crates/optional/zkvm-executor` + `aoem-runtime-cli(zkvm-executor feature)` | `novovm-prover`（已落地）+ `novovm-exec` 能力探测面 | 重构为稳定契约 | P0 |
| F-16 | AOEM MSM 加速能力（BLS12-381） | `aoem-engine` + `aoem-ffi`（`BlsMsmBackend/BlsMsmDecision`） | `novovm-prover`（已落地）+ `novovm-exec` 能力探测面 | 复用并标准化输出 | P0 |

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

- 共识层：按迁移生产口径由 `~80%` 收口到 `Done`（`novovm-consensus` 已补齐 QC 批量签名验证，2026-03-15）。
- 网络层：核心功能已完成并通过主体测试，按生产封盘口径建议 `90~95%`。
- 证据文档：`SVM2026-LAYER-STATUS-VERIFIED-2026-03-03.md`。

## 3.3 Phase B 自动化进展（2026-03-03）

- `state_root`：已切换为硬一致性门禁（`state_root.available=true`，`method=hard_state_root_parity`），代理门禁仅作为降级路径保留。
- baseline：已新增 `scripts/migration/import_svm2026_baseline.ps1`，可将 `SVM2026` TPS 证据转换为 `run_performance_compare.ps1` 基线 JSON。
- 台账：已新增 `scripts/migration/generate_capability_ledger_auto.ps1`，可自动回填报告证据路径与状态快照。

## 3.4 迁移批次路线（Batch A-E）

| Batch | 目标闭环 | 对应能力 | 当前状态 |
|---|---|---|---|
| A | 交易入口 -> `ops_v2` -> AOEM 执行 -> 状态提交 -> 批次输出（最小真链） | F-01/F-03/F-04 + F-05 最小接线 | Done（MVP；`tx_codec_signal` / `mempool_admission_signal` / `tx_metadata_signal` / `batch_a_closure` / `block_wire_signal` / `block_output_signal` / `commit_output_signal` 持续通过，`state_root` 硬一致性门禁已开启） |
| B | 共识与终局（执行-共识解耦） | F-05 | Done（MVP；`novovm-consensus` + `consensus_negative_signal` 持续通过） |
| C | P2P / gossip / 同步 | F-07 + F-08 | Done（MVP；`novovm-network` + Adapter 双后端闭环稳定，`compare + ABI/符号/注册表` 负向门禁均已默认开启并通过） |
| D | ZK 证明路径（prover/verifier） | F-15/F-16 | Done（迁移契约、门禁与 runtime 能力已就绪；`zkvm_prove/zkvm_verify/msm_backend` 已回填） |
| E | RPC / CLI / DevEx | F-10~F-13（裁剪后） | Done（按裁剪口径，见能力台账 F-10~F-13） |

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
