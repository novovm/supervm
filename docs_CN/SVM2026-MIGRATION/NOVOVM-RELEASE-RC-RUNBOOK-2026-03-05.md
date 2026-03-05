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
