# NOVOVM 发布候选（RC）流程手册（2026-03-05）

## 1. 目标

- 把 `full_snapshot_v1/v2/ga_v1` 固化为可重复执行的发布候选口径。
- 用单一 `rc_ref`（tag 或 commit-hash）关联一次完整快照，形成可追溯证据。

## 2. 冻结规则（必须遵守）

- `full_snapshot_v1` 语义冻结，不再改变含义。
- 若未来新增能力门禁，必须升级到 `full_snapshot_v2`（或更高）并生成新快照目录。

## 3. 目录命名规则

- RC 产物目录：`artifacts/migration/release-candidate-<rc_ref_normalized>/`
- 快照目录：`.../snapshot/`
- 核心产物：
  - `rc-candidate.json`（RC事实入口）
  - `snapshot/release-snapshot.json`（全量快照事实）
  - `snapshot/acceptance-gate-full/acceptance-gate-summary.json`（gate 明细）

## 4. 三行命令（可复现 full_snapshot_v1）

```powershell
git tag -a novovm-rc-2026-03-05-relfix -m "full_snapshot_v1 relfix green"
powershell -ExecutionPolicy Bypass -File scripts/migration/run_release_candidate.ps1 -RepoRoot . -RcRef novovm-rc-2026-03-05-relfix
Get-Content artifacts/migration/release-candidate-novovm-rc-2026-03-05-relfix/rc-candidate.json -Raw
```

说明：
- 若不想先打 tag，可把第二行 `-RcRef` 直接换成 commit hash（例如 `-RcRef 14be2b0ec65f`）。
- `run_release_candidate.ps1` 内部会执行 `run_release_snapshot.ps1`，并强制校验：
  - `snapshot_profile=full_snapshot_v1`
  - `snapshot_overall_pass=true`
  - `governance_param3_pass=true`
  - `adapter_stability_pass=true`
  - （GA profile）`governance_access_policy_pass=true`、`governance_token_economics_pass=true`、`governance_treasury_spend_pass=true`

## 5. 发布口径（GA-only）

- RC（含 `full_snapshot_v1/v2`）仅用于内部工程基线与回归锚点，不作为对外发布版本。
- 对外只发布 GA（完整主网经济治理版），避免中间版本造成口径混淆。
- RC 目录与 tag 继续保留，作为可追溯证据，不作为对外可用承诺。

## 6. 治理 RPC 安全发布铁律（默认行为）

- Public RPC 永不暴露治理方法：`governance_*` 在 public 口返回 `-32601`。
- Governance RPC 默认关闭：`NOVOVM_ENABLE_GOV_RPC=0`。
- 开启 Governance RPC 时默认仅本地绑定：`NOVOVM_GOV_RPC_BIND=127.0.0.1:8901`，并支持 `NOVOVM_GOV_RPC_ALLOWLIST` 限制来源 IP。
- 非回环地址启用治理端口时，若 `NOVOVM_GOV_RPC_ALLOWLIST` 为空，节点启动直接失败（防误开放）。

最小门禁：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_rpc_exposure_gate.ps1 -RepoRoot .
```

全量快照（含 RPC 暴露门禁）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_migration_acceptance_gate.ps1 -RepoRoot . -FullSnapshotProfileV2
```

对应 RC（v2）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_release_candidate.ps1 -RepoRoot . -RcRef novovm-rc-2026-03-05-rpc-exposure-v2 -FullSnapshotProfileV2
```

## 7. 正式 RC v2 指针（2026-03-05）

- `rc_ref`: `novovm-rc-2026-03-05-v2`
- `commit_hash`: `6d4bcf467f31f2de91d093e122c8390bc6a27e43`
- 产物入口：`artifacts/migration/release-candidate-novovm-rc-2026-03-05-v2/rc-candidate.json`

## 8. 正式 RC GA v1 指针（2026-03-06）

- `rc_ref`: `novovm-rc-2026-03-06-ga-v1-retryfix`
- `commit_hash`: `69a7742b733c7fb21399b5159aeec2dc66b3d815`
- `snapshot_profile`: `full_snapshot_ga_v1`
- `status`: `ReadyForMerge/SnapshotGreen`
- 关键门禁：`governance_access_policy_pass=true`、`governance_token_economics_pass=true`、`governance_treasury_spend_pass=true`、`rpc_exposure_pass=true`
- 产物入口：`artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-v1-retryfix/rc-candidate.json`
- 稳态说明：`scripts/migration/run_adapter_stability_gate.ps1` 已对 `registry_negative hash_mismatch reason_drift` 增加定向单次重试。

## 9. 可选：I-GOV-04（ML-DSA AOEM-FFI）纳入快照/RC

当需要把 `governance_rpc_mldsa_ffi_pass` 一并写入 `release-snapshot/rc-candidate` 时，使用以下参数：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_release_candidate.ps1 `
  -RepoRoot . `
  -RcRef novovm-rc-2026-03-06-ga-v1-mldsa `
  -FullSnapshotProfileGA `
  -IncludeGovernanceRpcMldsaFfiGate `
  -GovernanceRpcMldsaFfiAoemRoot ..\AOEM
```

说明：
- 该模式会把 `governance_rpc_mldsa_ffi_gate_enabled/pass/startup_pass` 写入 `snapshot.key_results` 与 `rc-candidate.json`。
- 若不启用该参数，`full_snapshot_*` 默认语义保持不变（ML-DSA 仍属于可选执行能力，不强制进入默认发布面）。

## 10. 可选：统一账户（UA-Gx）纳入快照/RC

当需要把统一账户 gate（UA-G01~UA-G16）结果写入 `release-snapshot/rc-candidate` 时，使用以下参数：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_release_candidate.ps1 `
  -RepoRoot . `
  -RcRef novovm-rc-2026-03-07-ua-gate `
  -FullSnapshotProfileGA `
  -IncludeUnifiedAccountGate
```

说明：
- 该模式会把 `unified_account_gate_enabled/pass` 写入 `acceptance-gate-summary.json`。
- `release-snapshot.json` 会额外包含：
  - `enabled_gates.unified_account`
  - `key_results.unified_account_pass`
  - `key_results.unified_account_block_merge_pass`
  - `key_results.unified_account_block_release_pass`
  - `evidence.unified_account_summary_json`
- `rc-candidate.json` 会同步包含：
  - `unified_account_gate_enabled`
  - `unified_account_pass`
  - `unified_account_block_merge_pass`
  - `unified_account_block_release_pass`
  - `unified_account_summary_json`

## 10.1 可选：Testnet Bootstrap（TNET-B）纳入快照/RC

当需要把 `testnet_bootstrap` 门禁结果写入 `release-snapshot/rc-candidate` 时，使用以下参数：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_release_candidate.ps1 `
  -RepoRoot . `
  -RcRef novovm-rc-2026-03-15-testnet-bootstrap `
  -IncludeTestnetBootstrapGate
```

说明：
- 该模式会把 `testnet_bootstrap_gate_enabled/pass` 写入 `acceptance-gate-summary.json`。
- `release-snapshot.json` 会额外包含：
  - `enabled_gates.testnet_bootstrap`
  - `key_results.testnet_bootstrap_pass`
  - `key_results.testnet_bootstrap_validators_pass`
  - `key_results.testnet_bootstrap_batches_pass`
  - `key_results.testnet_bootstrap_tps_pass`
  - `key_results.testnet_bootstrap_network_messages_pass`
  - `evidence.testnet_bootstrap_summary_json`
- `rc-candidate.json` 会同步包含：
  - `testnet_bootstrap_gate_enabled`
  - `testnet_bootstrap_pass`
  - `testnet_bootstrap_validators_pass`
  - `testnet_bootstrap_batches_pass`
  - `testnet_bootstrap_tps_pass`
  - `testnet_bootstrap_network_messages_pass`
  - `testnet_bootstrap_report_json`
  - `testnet_bootstrap_summary_json`
- 若不启用该参数，`full_snapshot_*` 默认语义保持不变（不强制进入默认发布面）。

## 11. EVM Overlap Router（A15）信号接线

- `run_migration_acceptance_gate.ps1` 已增加 `overlap_router_signal`（默认开启），并写入：
  - `overlap_router_signal_gate_enabled`
  - `overlap_router_signal_pass`
  - `overlap_router_signal_report_json`
- `release-snapshot.json` 已同步增加：
  - `enabled_gates.overlap_router_signal`
  - `key_results.overlap_router_signal_pass`
  - `evidence.overlap_router_signal_summary_json`
- `rc-candidate.json` 已同步增加：
  - `overlap_router_signal_gate_enabled`
  - `overlap_router_signal_pass`
  - `overlap_router_signal_report_json`

## 12. EVM Chain Profile（A09）信号接线

- `run_migration_acceptance_gate.ps1` 已增加 `evm_chain_profile_signal`（默认开启），并写入：
  - `evm_chain_profile_signal_gate_enabled`
  - `evm_chain_profile_signal_pass`
  - `evm_chain_profile_signal_report_json`
- `release-snapshot.json` 已同步增加：
  - `enabled_gates.evm_chain_profile_signal`
  - `key_results.evm_chain_profile_signal_pass`
  - `evidence.evm_chain_profile_signal_summary_json`
- `rc-candidate.json` 已同步增加：
  - `evm_chain_profile_signal_gate_enabled`
  - `evm_chain_profile_signal_pass`
  - `evm_chain_profile_signal_report_json`

## 13. EVM 四链 + 严格性能口径 RC 指针（2026-03-07）

- `rc_ref`: `rc-evm-next-step-4chain-strict-v2-20260307`
- `commit_hash`: `85486236ab14d6939fa06f9e165863fd704da20c`
- `snapshot_profile`: `full_snapshot_v2`
- `status`: `ReadyForMerge/SnapshotGreen`
- 四链 compare：
  - `evm_backend_compare_evm_pass=true`
  - `evm_backend_compare_polygon_pass=true`
  - `evm_backend_compare_bnb_pass=true`
  - `evm_backend_compare_avalanche_pass=true`
- 严格性能口径（`AllowedRegressionPct=-5`）：
  - `cpu_batch_stress delta=+2.17% pass=true`
  - `cpu_parity delta=-4.49% pass=true`
  - `preset_cooldown_sec=2`
- 产物入口：
  - `artifacts/migration/release-candidate-next-step-4chain-strict-v2/rc-candidate.json`
  - `artifacts/migration/release-candidate-next-step-4chain-strict-v2/snapshot/release-snapshot.json`
  - `artifacts/migration/release-candidate-next-step-4chain-strict-v2/snapshot/acceptance-gate-full/acceptance-gate-summary.json`
  - `artifacts/migration/release-candidate-next-step-4chain-strict-v2/snapshot/acceptance-gate-full/performance-gate/performance-gate-summary.json`

## 14. 严格口径复现命令（推荐）

先单跑严格性能门禁：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_performance_gate_seal_single.ps1 `
  -RepoRoot . `
  -OutputDir artifacts/migration/perf-gate-strict-manual `
  -AllowedRegressionPct -5 `
  -Runs 3
```

再跑严格 RC（默认 `AllowedRegressionPct=-5`）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_release_candidate.ps1 `
  -RepoRoot . `
  -RcRef rc-evm-next-step-4chain-strict-v2-20260307 `
  -OutputDir artifacts/migration/release-candidate-next-step-4chain-strict-v2 `
  -FullSnapshotProfileV2
```

说明：
- `run_performance_compare.ps1` 在 `seal_single` 下已固定 `threads=1, engine-workers=4`。
- preset 间默认冷却 `2s`（`PresetCooldownSec`），用于降低连续采样热耦合。

## 15. EVM+UA（plugin self-guard + rocksdb）严格 RC 指针（2026-03-08）

- `rc_ref`: `rc-evm-ua-selfguard-rocksdb-20260308-000948`
- `commit_hash`: `85486236ab14d6939fa06f9e165863fd704da20c`
- `snapshot_profile`: `full_snapshot_v2`
- `status`: `ReadyForMerge/SnapshotGreen`
- 关键结果：
  - `snapshot_overall_pass=true`
  - `unified_account_pass=true`
  - `evm_tx_type_signal_pass=true`
  - `evm_backend_compare_{evm,polygon,bnb,avalanche}_pass=true`
  - strict performance（`AllowedRegressionPct=-5`）：
    - `cpu_batch_stress delta=-0.68% pass=true`
    - `cpu_parity delta=-2.24% pass=true`
- 产物入口：
  - `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/rc-candidate.json`
  - `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/snapshot/acceptance-gate-full/acceptance-gate-summary.json`
  - `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/snapshot/acceptance-gate-full/performance-gate/performance-gate-summary.json`
- plugin-side standalone 证据：
  - `artifacts/migration/unifiedaccount/plugin-selfguard-standalone-smoke-20260308-001323/plugin-selfguard-standalone-smoke-summary.json`

补充：
- `run_evm_backend_compare_signal.ps1` 已增加 Windows 短路径状态目录策略（默认 `artifacts/migration/evm/backend-compare-state`），并支持 `NOVOVM_EVM_BACKEND_COMPARE_STATE_ROOT` 覆盖，避免 rocksdb 在深路径下创建 `OPTIONS-*.dbtmp` 失败。

## 16. 本地 AOEM 套件更新后的 acceptance 快速收口（2026-03-15）

- 先决条件：若出现 `AOEM DLL hash mismatch`，先同步 `aoem/manifest/aoem-manifest.json` 与当前 `aoem/bin/aoem_ffi.dll` 哈希。
- 脚本兼容：`run_performance_gate_seal_single.ps1` 在严格模式下需确保 `$IsWindows` 已初始化。
- 生产策略说明：在 `production-only` 节点策略下，`run_chain_query_rpc_gate.ps1` 已 decommission，旧 rpc probe 不作为本地快速收口主路径。
- 推荐本地收口命令：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_migration_acceptance_gate.ps1 `
  -FullSnapshotProfile `
  -AllowedRegressionPct -10
```

- 本地结果：`profile_name=full_snapshot_v1`，`overall_pass=true`，产物：
  - `artifacts/migration/acceptance-gate/acceptance-gate-summary.json`

## 17. 高性能直推最小提交集（2026-03-15）

目标：仅提交本轮“热路径性能收口”代码与对应台账，不混入其他功能线改动。

建议提交文件（最小集）：

- `crates/novovm-adapter-novovm/src/lib.rs`
  - 批验签 fallback 并行阈值与分块策略（小批串行、大批并行）。
- `crates/novovm-network/src/transport.rs`
  - sync-pull followup 默认 fast path；
  - fallback 发送成功后再 track；
  - followup fanout env 解析 `OnceLock` 缓存化。
- `crates/novovm-node/src/main.rs`
  - `verify_local_tx_signatures_batch` 去中间回拷贝；
  - admission/meta 汇总循环微优化（zip + 分支拆分 + 预分配）。
- `docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-2026-03-03.md`
  - 本轮性能改动与证据回填。

对应 diff 规模（参考）：

- `crates/novovm-adapter-novovm/src/lib.rs`: `+48/-2`
- `crates/novovm-network/src/transport.rs`: `+126/-43`
- `crates/novovm-node/src/main.rs`: `+389/-114`
- `docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-2026-03-03.md`: `+20/-0`

一键暂存（仅最小集）：

```powershell
git add `
  crates/novovm-adapter-novovm/src/lib.rs `
  crates/novovm-network/src/transport.rs `
  crates/novovm-node/src/main.rs `
  docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-2026-03-03.md
```

最小校验（非工程化口径）：

```powershell
cargo clippy -p novovm-adapter-novovm -p novovm-network -p novovm-node -- -D warnings
powershell -ExecutionPolicy Bypass -File scripts/migration/run_migration_acceptance_gate.ps1 -FullSnapshotProfile -AllowedRegressionPct -10
```

提交建议（示例）：

```powershell
git commit -m "perf(hotpath): tighten adapter/network/node fast paths and keep acceptance green"
```
