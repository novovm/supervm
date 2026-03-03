# NOVOVM 能力迁移执行台账（2026-03-03）

## 状态约定

- `NotStarted`: 未开始
- `InProgress`: 进行中
- `Blocked`: 被阻塞
- `ReadyForMerge`: 当前迁移闭环达成，可并入主线（不等于生产全量 Done）
- `Done`: 已完成

## 台账

| ID | 能力名称 | 来源模块 | 目标模块 | 状态 | 本轮进展 | 下步动作 | 最近更新 |
|---|---|---|---|---|---|---|---|
| F-05 | 共识引擎（核验约80%） | `supervm-consensus` | `novovm-consensus` | InProgress | 已完成 `novovm-node` 的 Batch A 闭环接线并升级为真实交易前置链路（`accounts=2`、`fee=1~5`、`demo_txs=8`、`target_batches=2`）：tx ingress -> `tx_codec` -> `mempool_out` -> tx metadata verify -> ops_v2 -> batch partition -> proposal/vote/qc/commit -> block_wire -> block_out -> commit_out；并将本地 tx wire codec 下沉到 `novovm-protocol::tx_wire`（`novovm_local_tx_wire_v1`）、block header wire 下沉到 `novovm-protocol::block_wire`（`novovm_block_header_wire_v1`）；一致性报告已覆盖 `tx_codec_signal` / `mempool_admission_signal` / `tx_metadata_signal` / `batch_a_input_profile` / `batch_a_closure` / `block_wire_signal` / `block_output_signal` / `commit_output_signal`（当前均通过） | 在 AOEM FFI 暴露 `state_root` 后切换为硬一致性门禁，并继续协议化 block/batch wire | 2026-03-03 |
| F-07 | 网络层（核心完成，生产待收口） | `supervm-network` + `l4-network` | `novovm-network` + `novovm-protocol` | ReadyForMerge | 已在 `novovm-network` 落地 `UdpTransport`，`novovm-node` 探针改为调用网络层；`run_network_two_process.ps1` 已升级为可配置 N 节点 mesh + 多轮探针，当前 `NodeCount=3`、`Rounds=2`、`pairs=6/6`、`directed=12/12` 通过并回填 `network_process_signal`；并新增跨进程 `network_block_wire` 校验（`sync payload`= `novovm_block_header_wire_v1`，接收端执行 `consensus binding` 校验），当前 `block_wire=12/12` 通过；新增 `TamperBlockWireMode` 负例（`hash_mismatch/class_mismatch/codec_corrupt`）并接入 `network_block_wire_negative_signal` 门禁，当前负例口径 `verified=0/2`（预期失败）；同时新增 `udp_transport_mesh_three_nodes_closure` 回归样本 | 进入生产硬化收口：长压、异常恢复、真实同步与观测告警 | 2026-03-03 |
| F-08 | Chain Adapter 接口 | `supervm-chainlinker-api` | `novovm-adapter-api` + `novovm-adapter-novovm` + `novovm-adapter-sample-plugin` | InProgress | 已完成双后端接线：API 层仅保留 IR + Trait；原生后端 `novovm-adapter-novovm`（`create_native_adapter`）作为默认路径；插件后端 `novovm-adapter-sample-plugin` 通过 C ABI 动态加载；`novovm-node` 支持 `NOVOVM_ADAPTER_BACKEND=auto|native|plugin`、`NOVOVM_ADAPTER_CHAIN`、`NOVOVM_ADAPTER_PLUGIN_PATH`，新增 ABI 门禁 `NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI` / `NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS` 与注册表门禁 `NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH` / `NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT` / `NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256`（配合 `allowed_abi_versions`）；新增共识绑定 `adapter_consensus`（`plugin_class=consensus` + `consensus_adapter_hash`），并把 `consensus_adapter_hash` 写入 block header，提交阶段执行强校验（不匹配拒块）；功能一致性已覆盖 `adapter_signal`（backend）+ `adapter_plugin_abi_signal`（ABI/caps）+ `adapter_plugin_registry_signal`（strict/hash/abi whitelist）+ `adapter_consensus_binding_signal`（class/hash）+ `adapter_backend_compare_signal`（native/plugin 同输入对照）+ `adapter_plugin_abi_negative_signal`（ABI/caps mismatch 负例必须失败）+ `adapter_plugin_symbol_negative_signal`（坏插件/缺符号负例必须失败）+ `adapter_plugin_registry_negative_signal`（hash/whitelist mismatch 负例必须失败） | 增加插件注册表与版本兼容矩阵，并补齐非 `novovm/custom` 适配实现样本 | 2026-03-03 |
| F-15 | AOEM ZK 能力契约 | `optional/zkvm-executor` | `novovm-prover` + `novovm-exec` | InProgress | `zkvm_prove/zkvm_verify` 已接入能力快照与自动台账回填，当前探测值 `false/false` | 与 AOEM 侧对齐正式 ZK 开关字段与 fallback 原因码 | 2026-03-03 |
| F-16 | AOEM MSM 加速契约 | `aoem-engine` + `aoem-ffi` | `novovm-prover` + `novovm-exec` | ReadyForMerge | MSM 能力字段已接入能力快照、性能报告与自动台账（当前 `msm_accel=true`） | 与 AOEM FFI 对齐 `msm_backend/fallback_reason_codes` 正式字段 | 2026-03-03 |

## 全量扫描快照（F-01 ~ F-16）

来源：`NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-03.md` 的 `Full Scan Matrix (F-01~F-16)`（由脚本自动生成）。

| ID | 状态 | 说明（自动证据摘要） |
|---|---|---|
| F-01 | ReadyForMerge | `exec=True, bindings=True, adapter_signal.pass=True` |
| F-02 | ReadyForMerge | `exec=True, variant_digest.pass=True` |
| F-03 | ReadyForMerge | `protocol=True, tx_codec=True, block_wire=True, block_out=True, commit_out=True` |
| F-04 | InProgress | `state_root.available=False, state_root.pass=True` |
| F-05 | InProgress | `consensus=True, batch_a=True` |
| F-06 | NotStarted | `coordinator=False` |
| F-07 | ReadyForMerge | `network=True, process=True, block_wire=True, block_wire_negative=True` |
| F-08 | InProgress | `adapter=True, abi=True, registry=True, consensus=True, compare=True` |
| F-09 | InProgress | `prover=False, zk_ready=False` |
| F-10 | NotStarted | `storage_service=False` |
| F-11 | NotStarted | `app_domain=False` |
| F-12 | NotStarted | `app_defi=False` |
| F-13 | NotStarted | `adapters_multi=False` |
| F-14 | InProgress | `protocol=True, consensus=True, network=True, adapter=True, legacy_vm_runtime_present=False` |
| F-15 | InProgress | `zkvm_prove=False, zkvm_verify=False` |
| F-16 | ReadyForMerge | `msm_accel=True, msm_backend=` |

## 自动回填快照

- 快照文档：`docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-03.md`
- 生成脚本：`scripts/migration/generate_capability_ledger_auto.ps1`
- 关键证据：
  1. `artifacts/migration/functional-smoke37-native-abi/functional-consistency.json`
  2. `artifacts/migration/network-two-process-f34/network-two-process.json`
  3. `artifacts/migration/performance/performance-compare.json`
  4. `artifacts/migration/capabilities/capability-contract-core.json`
  5. `artifacts/migration/baseline/svm2026-baseline-core.json`
  6. `artifacts/migration/functional-smoke38-plugin-abi/functional-consistency.json`
  7. `artifacts/migration/functional-smoke39-backend-compare-abi/functional-consistency.json`
  8. `artifacts/migration/functional-smoke40-abi-negative/functional-consistency.json`
  9. `artifacts/migration/functional-smoke48-protocol-binding/functional-consistency.json`
  10. `artifacts/migration/functional-smoke49-registry-negative/functional-consistency.json`
  11. `artifacts/migration/network-two-process-smoke50/network-two-process.json`
  12. `artifacts/migration/functional-smoke50-network-wire-full/functional-consistency.json`
  13. `artifacts/migration/network-two-process-smoke51-normal/network-two-process.json`
  14. `artifacts/migration/network-two-process-smoke51-negative/network-two-process.json`
  15. `artifacts/migration/functional-smoke51-network-wire-negative/functional-consistency.json`

## 当前阻塞项

1. AOEM 仓库 `vendor/curve25519-dalek` 缺失，影响 AOEM 侧完整构建核验。
