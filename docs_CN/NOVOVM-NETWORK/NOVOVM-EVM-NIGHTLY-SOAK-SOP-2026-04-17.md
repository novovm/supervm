# NOVOVM EVM Nightly Soak 运行与处置 SOP（2026-04-17）

## 1. 目标与范围

本 SOP 仅用于 EVM 插件维护态（运行于 NOVOVM 宿主）：

- 守长稳，不做新功能开发
- 固定入口：`supervm-mainline-nightly-gate`
- 固定产物：`6h/24h soak + nightly gate report`
- 固定原则：主 gate 与 nightly gate 解耦，nightly 不拖慢日常 CI

V2 证据契约见
[`NOVOVM_MAINLINE_SOAK_EVIDENCE_V2.md`](../../docs/NOVOVM_MAINLINE_SOAK_EVIDENCE_V2.md)。
当前采样源是既有 ETH worker runtime snapshot；本 SOP 不签收 NOV 原生交易最终性、
区块一致性或主网 TPS。没有匹配的在线 self-hosted runner、没有实际执行的长跑，
记为 `NOT EXECUTED`，不得根据旧报告或冒烟结果填 Green。

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

- `supervm-mainline-soak-report/v2`
- `supervm-mainline-nightly-soak-gate-report/v2`

Rust 结构体仍沿用既有 `V1` 名称；读取 JSON 必须核对 `/v2` schema。

### 2.3 V2 证据门

- 默认 `mode=workload`：必须有 `counters.sampled_body_updates > 0`。
- `mode=idle_health`：只检查 exporter 在持续更新，不证明交易或区块推进。
- 快照必须 schema/chain 正确、不过期、不来自未来且 timestamp 前进；重复读取
  同 timestamp 不增加有效采样数。
- 默认 `max_snapshot_age_ms=180000`、`min_valid_samples=2`、
  `min_valid_sample_ratio_bps=9000`。
- 有效比例以全部读取尝试为分母；最后一次读取必须有效，首尾及中途无有效采样
  的间隔不得超过 `max_snapshot_age_ms`。基线后的 timestamp 必须晚于本轮开始，
  防止依次重放运行前快照；观察端系统时间回退也必须失败。
- budget hit/deferred/time-slice 三类进程累计计数不得回退；header/body/request 是
  每轮计数，可以下降。V2 对基线以后的有效采样轮次求和，字段为
  `sampled_header_updates`、`sampled_body_updates`、`sampled_sync_requests`。
  采样之间可能漏过轮次，因此不把这些值当作精确累计处理量。
- 过期、未来、错链、错 schema、timestamp 回退或累计计数回退均为硬失败；
  调低业务指标阈值不能绕过证据门。
- 实际观察时间使用单调时钟；等待经过六小时不会使静止的旧快照成为有效证据。

报告明确 `process_continuity_attested=false`：当前 snapshot 没有进程身份，
例如预算计数一直为零时，重启前后 `0 -> 0` 无法据此识别。这里只能检测可见的
累计计数回退，不能证明进程从未重启；已知重启后应开始新的观察轮次。

### 2.4 时长与运行条件

`validation_scope` 为 `short_smoke`、`soak` 或 `idle_health`。缩短 standalone
时长可以得到通过的 `short_smoke`，但报告仍保留 6h/24h 的
`nominal_duration_seconds` 和 `duration_requirement_met=false`。
Nightly 总体通过要求每个 profile 均为通过的 workload 且完成标称时长；短冒烟或
idle-health 不能通过 nightly 签收。

默认 6h、24h 串行运行，纯观察即 30 小时；workflow job 配置 36 小时预算。
在线 runner、持续快照源、供电和完整产物收集仍须独立确认。GitHub 的
`GITHUB_TOKEN` 最长 24 小时，延长 job timeout 不会延长它；长跑后的鉴权操作和
产物上传需在实际运行环境验证，参见
[官方说明](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idtimeout-minutes)。
串行的 30 小时 job 也不能保证每日完成一轮。

同 workflow 中另有 native pipeline 的两秒 production-profile 冒烟：步骤和产物
明确命名为 `production smoke` / `native-pipeline-production-smoke.json`，不属于
30 分钟或本节 6h/24h 长跑证据。

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
  mode:
  validation_scope:
  nominal_duration_seconds:
  duration_requirement_met:
  sample_count:
  sampling_evidence_summary:
  observed_elapsed_seconds:
  violation_count:
  throttle_hit_rate_bps_estimated:
  sampled_body_updates:
  sampled_body_updates_per_hour:
  pending_queue_depth_peak:
  pending_queue_recovery_per_hour:
  target_oscillation_bps:
  time_slice_target_utilization_peak_bps:
  top_execution_target_reason:
  top_execution_target_reason_share_bps:

24h_summary:
  pass:
  mode:
  validation_scope:
  nominal_duration_seconds:
  duration_requirement_met:
  sample_count:
  sampling_evidence_summary:
  observed_elapsed_seconds:
  violation_count:
  throttle_hit_rate_bps_estimated:
  sampled_body_updates:
  sampled_body_updates_per_hour:
  pending_queue_depth_peak:
  pending_queue_recovery_per_hour:
  target_oscillation_bps:
  time_slice_target_utilization_peak_bps:
  top_execution_target_reason:
  top_execution_target_reason_share_bps:

classification:
  level: GREEN|YELLOW|RED|NOT_EXECUTED
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
- 两份报告均为 `/v2`、`mode=workload`、`validation_scope=soak`，且
  `duration_requirement_met=true`
- 无持续高压信号（连续 3 天同一高压 reason）

自动 duty-report consumer 同样要求 v2 schema、profile 匹配、workload/soak、
标称及实际时长足够、至少两次采样和正的 sampled body 处理量。旧版报告、
short-smoke、idle-health 即使带有 `pass=true`，也不能形成 Green 日报。

动作：归档，无参数变更。

### 5.2 Yellow

- `overall_pass=true`，但出现以下任一趋势：
  - `top_execution_target_reason_share_bps` 持续抬高
  - `pending_queue_depth_peak` 连续上升
  - `sampled_body_updates_per_hour` 在相同采样策略下连续下降
  - `target_oscillation_bps` 持续高位

动作：小步调参，保留硬上限不变。

### 5.3 Red

- `overall_pass=false`
- 或任一 profile `pass=false`
- 或证据失效（快照过期、错链、重复导致有效采样不足、累计计数回退等）
- 或出现核心路径失活（如 `sampled_body_updates_per_hour` 接近 0 且持续）

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
- `sampled_body_updates_per_hour` 在相同采样策略下下降

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

V2 证据参数沿用上述覆盖顺序（profile 优先）：

| 后缀 | 默认值 |
| --- | --- |
| `MODE` | `workload`；可选 `idle_health` |
| `MAX_SNAPSHOT_AGE_MS` | `180000` |
| `MIN_VALID_SAMPLES` | `2` |
| `MIN_VALID_SAMPLE_RATIO_BPS` | `9000` |

兼容阈值 `MIN_BODY_UPDATES_PER_HOUR` 对应 V2 的采样轮次 body-update 速率，
不代表精确全量吞吐。采样间隔应与 exporter 实际刷新频率匹配；重复 timestamp
会降低有效采样比例。

启动前还会校验配置：profile 仅支持 `1h/6h/24h`；duration 为 1 至 604800 秒；
interval 为 1 至 duration 秒；计划采样数 `ceil(duration/interval)+1` 不得超过
100000。`min_valid_samples` 至少为 2 且不能超过计划采样数；
`max_snapshot_age_ms` 至少覆盖一个采样间隔；有效比例为 1 至 10000 bps。
负数或非有限速率阈值，以及超过 10000 的 bps 阈值均拒绝，不进入运行。

## 8. 应急与回滚

触发 Red 时：

1. 先冻结新增变更（只做预算回调，不上新功能）。
2. 回滚到上一个 Green 配置快照。
3. 可先运行 standalone `short_smoke` 定位，再跑完整 6h；短时结果不能关闭事件。
4. 若仍 Red，升级为主线稳定性事件。

回滚完成条件：

- 至少一轮完整 6h workload `pass=true` 且 `duration_requirement_met=true`
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

仅做快速冒烟，消费已经持续更新的快照（命令不会启动节点或生成负载）：

```powershell
$env:NOVOVM_MAINLINE_SOAK_PROFILE="6h"
$env:NOVOVM_MAINLINE_SOAK_MODE="workload"
$env:NOVOVM_MAINLINE_SOAK_DURATION_SECONDS="120"
$env:NOVOVM_MAINLINE_SOAK_INTERVAL_SECONDS="5"
$env:NOVOVM_MAINLINE_SOAK_REPORT_PATH="artifacts/mainline/mainline-smoke-120s.json"
cargo run -p novovm-node --bin supervm-mainline-soak
```

通过后只能记录 `short_smoke`。若只有 exporter 活性检查需求，显式改为
`MODE=idle_health` 并保留该报告分类。开始正式长跑时用新 PowerShell 会话，或清除
冒烟环境变量；直接缩短 nightly 的 profile 时长会使 V2 nightly gate 拒绝签收。

---

执行边界：

- EVM 线处于维护态：只做守门、校准、回归，不做无门禁的新逻辑扩张。
- 任何参数调整都必须通过后续 nightly 报告验证，不允许“改了就算通过”。
