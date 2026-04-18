# NOVOVM EVM Nightly Soak 运行与处置 SOP（2026-04-17）

## 1. 目标与范围

本 SOP 仅用于 EVM 插件维护态（运行于 NOVOVM 宿主）：

- 守长稳，不做新功能开发
- 固定入口：`supervm-mainline-nightly-gate`
- 固定产物：`6h/24h soak + nightly gate report`
- 固定原则：主 gate 与 nightly gate 解耦，nightly 不拖慢日常 CI

## 2. 固定入口与产物

### 2.1 Nightly workflow

- Workflow: `.github/workflows/mainline-nightly-soak.yml`
- 默认 profile: `6h,24h`
- 执行入口：
  - `cargo run -p novovm-node --bin supervm-mainline-nightly-gate`

### 2.2 报告产物

- `artifacts/mainline/mainline-soak-6h.json`
- `artifacts/mainline/mainline-soak-24h.json`
- `artifacts/mainline/mainline-nightly-soak-gate-report.json`

Schema 固定：

- `supervm-mainline-soak-report/v1`
- `supervm-mainline-nightly-soak-gate-report/v1`

## 3. 值班操作（每日）

1. 打开最新 nightly 运行，确认 job 是否完成。
2. 下载 nightly artifact，核对三份报告是否齐全。
3. 按本 SOP 第 4 节模板填一份“当日解读”。
4. 按第 5 节判级：
   - Green：仅归档；
   - Yellow：进入参数调优；
   - Red：触发应急动作。
5. 若非 Green，按第 6 节根因矩阵执行定位和处置。

## 4. 报告解读模板（直接复制）

```text
[EVM NIGHTLY SOAK DAILY]
date_utc:
workflow_run:
nightly_gate_overall_pass:
profiles: 6h/24h

6h_summary:
  pass:
  sample_count:
  observed_elapsed_seconds:
  violation_count:
  throttle_hit_rate_bps_estimated:
  body_updates_per_hour:
  pending_queue_depth_peak:
  pending_queue_recovery_per_hour:
  target_oscillation_bps:
  time_slice_target_utilization_peak_bps:
  top_execution_target_reason:
  top_execution_target_reason_share_bps:

24h_summary:
  pass:
  sample_count:
  observed_elapsed_seconds:
  violation_count:
  throttle_hit_rate_bps_estimated:
  body_updates_per_hour:
  pending_queue_depth_peak:
  pending_queue_recovery_per_hour:
  target_oscillation_bps:
  time_slice_target_utilization_peak_bps:
  top_execution_target_reason:
  top_execution_target_reason_share_bps:

classification:
  level: GREEN|YELLOW|RED
  primary_issue:
  impacted_path: network|sync|broadcast|mempool|execution_budget

actions:
  immediate:
  config_changes:
  owner:
  eta:
```

## 5. Green/Yellow/Red 判定

### 5.1 Green

- `nightly_gate_overall_pass=true`
- `6h/24h` 都无 violation
- 无持续高压信号（连续 3 天同一高压 reason）

动作：归档，无参数变更。

### 5.2 Yellow

- `overall_pass=true`，但出现以下任一趋势：
  - `top_execution_target_reason_share_bps` 持续抬高
  - `pending_queue_depth_peak` 连续上升
  - `body_updates_per_hour` 连续下降
  - `target_oscillation_bps` 持续高位

动作：小步调参，保留硬上限不变。

### 5.3 Red

- `overall_pass=false`
- 或任一 profile `pass=false`
- 或出现核心路径失活（如 `body_updates_per_hour` 接近 0 且持续）

动作：进入第 6 节应急流程。

## 6. 根因矩阵与处置动作

### 6.1 `execution_budget_issue` / `execution_budget_pressure_high`

常见信号：

- `throttle_hit_rate_bps_estimated` 高
- `execution_budget_hit_count` 快速增长
- `execution_time_slice_exceeded_count` 增长

动作顺序：

1. 保持硬边界不变：
   - `HOST_EXEC_BUDGET_PER_TICK`
   - `HOST_EXEC_TIME_SLICE_MS`
2. 仅调目标值：
   - `HOST_EXEC_TARGET_PER_TICK`
   - `HOST_EXEC_TARGET_TIME_SLICE_MS`
3. 观察下一次 6h 报告是否回落。

### 6.2 `mempool_pressure_issue`

常见信号：

- `pending_queue_depth_peak` 高
- `pending_queue_recovery_per_hour` 低
- `evicted/expired/rejected` 分布异常

动作顺序：

1. 调整存储预算窗口（优先温和）：
   - `PENDING_TX_TTL_MS`
   - `PENDING_TX_NO_SUCCESS_ATTEMPT_LIMIT`
   - `PENDING_TX_TOMBSTONE_RETENTION_MAX`
2. 检查广播预算是否过紧：
   - `TX_BROADCAST_MAX_PER_TICK`
   - `TX_BROADCAST_MAX_PROPAGATIONS`

### 6.3 `broadcast_issue`

常见信号：

- `broadcast_no_available_peer`
- `broadcast_repeated_failure`
- `broadcast_phase_stall`

动作顺序：

1. 检查可用 peer 和会话状态（先看 runtime summary）。
2. 再调整网络预算：
   - `SYNC_TARGET_FANOUT`
   - `RLPX_REQUEST_TIMEOUT_MS`
   - `SYNC_REQUEST_INTERVAL_MS`
3. 必要时下调广播频率，防止放大故障。

### 6.4 `chain_gap_issue`

常见信号：

- `highest-current` 长时间不收敛
- `body_updates_per_hour` 下降

动作顺序：

1. 先确认 peer 生命周期是否异常（cooldown/permanent reject 激增）。
2. 调整同步批次预算：
   - `SYNC_PULL_HEADERS_BATCH`
   - `SYNC_PULL_BODIES_BATCH`
3. 复查 6h 报告是否恢复。

## 7. 参数覆盖规则（运行时）

预算参数支持两级环境变量：

- 链级：`NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_<chain_id>_<KEY>`
- 全局：`NOVOVM_NETWORK_ETH_RUNTIME_<KEY>`

示例（chain_id=1）：

- `NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_HOST_EXEC_BUDGET_PER_TICK`
- `NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_HOST_EXEC_TARGET_PER_TICK`
- `NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_TX_BROADCAST_MAX_PER_TICK`
- `NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_PENDING_TX_TTL_MS`

Nightly soak 阈值覆盖：

- 全局前缀：`NOVOVM_MAINLINE_SOAK_...`
- profile 前缀：`NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_...` / `..._24H_...`

## 8. 应急与回滚

触发 Red 时：

1. 先冻结新增变更（只做预算回调，不上新功能）。
2. 回滚到上一个 Green 配置快照。
3. 复跑一次 6h（可先缩短验证，再跑完整 6h）。
4. 若仍 Red，升级为主线稳定性事件。

回滚完成条件：

- 至少一轮 6h `pass=true`
- `primary_issue` 不再持续出现

## 9. 复盘模板（Red/持续 Yellow 必填）

```text
[EVM NIGHTLY INCIDENT POSTMORTEM]
time_window:
incident_level:
primary_reason:
secondary_signals:
blast_radius:
what_changed:
why_not_caught_in_day_gate:
mitigation:
rollback:
verification:
followup_actions:
owner:
```

## 10. 执行命令参考

本地单次 soak：

```powershell
cargo run -p novovm-node --bin supervm-mainline-soak
```

本地 nightly gate：

```powershell
cargo run -p novovm-node --bin supervm-mainline-nightly-gate
```

从 nightly 产物自动生成值班日报：

```powershell
cargo run -p novovm-node --bin supervm-mainline-duty-report
```

仅做快速冒烟（不要替代 nightly 正式运行）：

```powershell
$env:NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_DURATION_SECONDS="120"
$env:NOVOVM_MAINLINE_NIGHTLY_SOAK_24H_DURATION_SECONDS="120"
cargo run -p novovm-node --bin supervm-mainline-nightly-gate
```

---

执行边界：

- EVM 线处于维护态：只做守门、校准、回归，不做无门禁的新逻辑扩张。
- 任何参数调整都必须通过后续 nightly 报告验证，不允许“改了就算通过”。
