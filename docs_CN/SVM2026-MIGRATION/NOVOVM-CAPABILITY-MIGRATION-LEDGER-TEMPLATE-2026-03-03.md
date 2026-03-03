# NOVOVM 能力迁移台账模板（2026-03-03）

## 使用规则

- 一条能力一条记录，不写模糊“大阶段百分比”。
- 每条记录必须有：来源、目标模块、契约、验证、回退。
- 状态只允许：`NotStarted` / `InProgress` / `Blocked` / `ReadyForMerge` / `Done`。
- 本文件是“模板/示例”，状态字段默认可保留 `NotStarted`，不代表实时进度；实时状态以 `NOVOVM-CAPABILITY-MIGRATION-LEDGER-2026-03-03.md` 与 `NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-03.md` 为准。
- 每次推进前，先执行一次全量扫描（F-01~F-16），再推进单项开发；全量扫描结果写入自动台账的 `Full Scan Matrix`。

## 台账表

| ID | 能力名称 | 来源模块 | 目标模块 | 当前状态 | 契约是否冻结 | 验证脚本 | 回退方案 | 负责人 | 最近更新 |
|---|---|---|---|---|---|---|---|---|---|
| F-05 | 共识引擎（约80%） | `supervm-consensus` | `novovm-consensus` | NotStarted | No | `scripts/migration/run_functional_consistency.ps1 -BatchADemoTxs 8 -BatchABatchCount 2 -BatchAMempoolFeeFloor 1`（检查 `tx_codec_signal` + `mempool_admission_signal` + `tx_metadata_signal` + `batch_a_input_profile` + `batch_a_closure` + `block_wire_signal` + `block_output_signal` + `commit_output_signal`） | 旧路径开关回退 | TBD | 2026-03-03 |
| F-07 | 网络层（核心完成） | `supervm-network` + `l4-network` | `novovm-network` + `novovm-protocol` | NotStarted | No | `scripts/migration/run_functional_consistency.ps1`（`network_output_signal` + `network_closure_signal` + `network_process_signal`）+ `scripts/migration/run_network_two_process.ps1 -ProbeMode mesh -NodeCount 3 -Rounds 2` | 网络模块独立回滚 | TBD | 2026-03-03 |
| F-08 | Chain Adapter 接口 | `supervm-chainlinker-api` | `novovm-adapter-api` + `novovm-adapter-novovm` + plugin crate | NotStarted | No | `cargo test --manifest-path crates/novovm-adapter-api/Cargo.toml` + `cargo test --manifest-path crates/novovm-adapter-novovm/Cargo.toml` + `scripts/migration/run_functional_consistency.ps1 -AdapterBackend native -AdapterExpectedBackend native -AdapterExpectedChain novovm -AdapterPluginExpectedAbi 1 -AdapterPluginRequiredCaps 0x1 -AdapterPluginRegistryStrict:$true -AdapterPluginRegistrySha256 <sha256>` + `scripts/migration/run_functional_consistency.ps1 -AdapterBackend plugin -AdapterPluginPath <dll> -AdapterExpectedBackend plugin -AdapterExpectedChain novovm -AdapterPluginExpectedAbi 1 -AdapterPluginRequiredCaps 0x1 -AdapterPluginRegistryStrict:$true -AdapterPluginRegistrySha256 <sha256>` + `scripts/migration/run_functional_consistency.ps1 -AdapterBackend native -AdapterExpectedBackend native -AdapterExpectedChain novovm -AdapterPluginExpectedAbi 1 -AdapterPluginRequiredCaps 0x1 -AdapterPluginRegistryStrict:$true -AdapterPluginRegistrySha256 <sha256> -IncludeAdapterBackendCompare:$true -AdapterComparePluginPath <dll> -IncludeAdapterPluginAbiNegative:$true -AdapterNegativePluginPath <dll> -IncludeAdapterPluginSymbolNegative:$true -IncludeAdapterPluginRegistryNegative:$true`（检查 `adapter_signal.backend` + `adapter_plugin_abi_signal.pass` + `adapter_plugin_registry_signal.pass` + `adapter_plugin_registry_signal.ffi_v2.hash_match` + `adapter_plugin_registry_signal.ffi_v2.abi_allowed` + `adapter_consensus_binding_signal.pass` + `adapter_backend_compare_signal.pass/state_root_equal` + `adapter_plugin_abi_negative_signal.pass` + `adapter_plugin_symbol_negative_signal.pass` + `adapter_plugin_registry_negative_signal.pass`） | `NOVOVM_ADAPTER_BACKEND=native` 强制回退原生路径 | TBD | 2026-03-03 |
| F-15 | ZK 能力契约 | `optional/zkvm-executor` | `novovm-prover` + `novovm-exec` | NotStarted | No | `scripts/migration/...` | 关闭 zk 通道降级 | TBD | 2026-03-03 |
| F-16 | MSM 加速契约 | `aoem-engine` + `aoem-ffi` | `novovm-prover` + `novovm-exec` | NotStarted | No | `scripts/migration/...` | 强制 CPU/禁用加速 | TBD | 2026-03-03 |

## 单能力验收卡（复制模板）

### [ID] [能力名称]

- 来源：
- 目标：
- 输入契约：
- 输出契约：
- 能力探测字段：
- 回退原因码：
- 验证命令：
- 验证结果：
- 风险：
- 回退步骤：
- 验收结论：
