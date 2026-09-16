# NOVOVM Mainline Soak Evidence V2

## Scope

This contract governs `supervm-mainline-soak`, `supervm-mainline-nightly-gate`
and their JSON evidence. The observed source is the existing
`EthFullnodeNativeWorkerRuntimeSnapshotV1` exporter used by the EVM plugin's
ETH worker. A passing report describes the sampled worker and the declared
observation interval. It does not prove NativeTransaction execution, agreement
on NOV blocks, QC/finality, public topology readiness or mainnet throughput.

The report schemas are:

- `supervm-mainline-soak-report/v2`
- `supervm-mainline-nightly-soak-gate-report/v2`

Existing Rust API type names ending in `V1` are retained; consumers
must inspect the JSON schema and v2 evidence fields. A v1 report cannot be
upgraded to v2 evidence by changing its schema string.

The canonical mainline gate and its preflight/runtime consumers also add the
three soak-evidence, CLI and duty-report checks to their ordered lockset
(39 entries become 42). After updating the source, rerun
`cargo run -p novovm-node --bin supervm-mainline-gate` to regenerate the status
and delivery artifacts. An old 39-entry status is not evidence that the new
checks passed.

## Evidence policy and sampling

Every report records `evidence_policy` with these defaults:

| Field | Default | Meaning |
| --- | --- | --- |
| `mode` | `workload` | Require sampled body progress as well as valid evidence |
| `max_snapshot_age_ms` | `180000` | Maximum snapshot age at each read |
| `min_valid_samples` | `2` | Minimum distinct, fresh accepted samples |
| `min_valid_sample_ratio_bps` | `9000` | Minimum accepted-sample ratio, in basis points |

Schema and chain must match the expected snapshot contract. A sample is valid
only when its timestamp is fresh and advances from the previous accepted
snapshot, and its cumulative process counters do not regress. A duplicate
timestamp does not become another valid sample just because the file was read
again. Stale or future timestamps, wrong chain/schema, regressing timestamps
and regressing cumulative counters are hard evidence failures. Enough valid
samples and the configured valid-sample ratio are required in both modes.

The ratio denominator is all read attempts, including failed reads. The last
read must be valid, and the largest gap without a valid sample, including the
start and end of the observation window, must not exceed
`max_snapshot_age_ms`. After an initial fresh baseline, accepted updates must
have timestamps later than the run start; replaying a series of pre-run files
cannot prove current activity. A backwards observation wall clock also fails
the evidence check rather than extending snapshot freshness.

Counter meanings come from the producer, rather than their field names:

- `execution_budget_hit_count`, `execution_deferred_count` and
  `execution_time_slice_exceeded_count` are cumulative process counters. They
  must not observably regress during one observation run. After a known
  restart, begin a new run.
- `header_updates`, `body_updates` and `sync_requests` are per-drive-round
  values. They may legitimately fall between rounds. The sampler sums values
  from accepted distinct rounds after the baseline, exposing
  `sampled_header_updates`, `sampled_body_updates` and `sampled_sync_requests`
  under `counters`, with corresponding `sampled_*_per_hour` metrics.
- These sampled-round sums can omit rounds between reads. They are not exact
  process totals or a benchmark throughput claim. Duplicate snapshot reads
  must never add the same sampled round again.

Reports label this explicitly with
`counter_semantics = eth_worker_sampled_rounds_and_process_budget_deltas`.
The compatibility environment variable `MIN_BODY_UPDATES_PER_HOUR` now applies
to the sampled body-update rate.

`process_continuity_attested=false` is explicit in the report. The snapshot
does not carry a process identity, so a restart whose counters remain zero,
or recover beyond their previous values before the next read, cannot be
identified from these samples. Observable counter rollback is detected;
uninterrupted process lifetime is not attested.

`workload` requires `counters.sampled_body_updates > 0` in addition to the
evidence and configured metric thresholds. Setting a zero minimum rate does
not waive this progress requirement. `idle_health` allows zero body progress;
its passing result establishes snapshot exporter activity only, not useful
transaction work or chain advancement.

Elapsed observation time uses `Instant`. Wall-clock timestamps remain useful
for snapshot freshness and report provenance, but cannot substitute for elapsed
observation time or make a stale sample valid.

## Duration and report interpretation

The report exposes `mode`, `validation_scope`, `nominal_duration_seconds` and
`duration_requirement_met` alongside requested and observed durations.

| Scope | What a passing standalone report establishes |
| --- | --- |
| `short_smoke` | Workload evidence over the recorded shorter interval |
| `soak` | Workload evidence over at least the profile's nominal interval |
| `idle_health` | Exporter activity over the recorded interval |

The nominal `1h`, `6h` and `24h` durations are 3,600, 21,600 and 86,400 seconds. Reducing
`DURATION_SECONDS` does not change these meanings. A standalone smoke may pass
its evidence checks while `duration_requirement_met=false`; it cannot be
reported as a completed six-hour or twenty-four-hour soak.

Nightly `overall_pass=true` requires every selected profile to have a passing
evaluation, `mode=workload` and `duration_requirement_met=true`. An idle-health
report or a short smoke cannot satisfy the nightly gate. A generated report
with failure details is useful diagnostic evidence, but its presence does not
mean that the gate passed.

The duty-report consumer independently requires v2 schemas, the expected
profile, workload/soak classification, completed nominal and actual observation
durations, at least two samples and positive sampled body progress. Legacy,
short-smoke and idle-health reports cannot produce a Green duty result even
if a supplied `pass` flag is true.

## Configuration

For standalone and global nightly defaults, append these keys to
`NOVOVM_MAINLINE_SOAK_`:

```text
MODE                         workload | idle_health
MAX_SNAPSHOT_AGE_MS           180000
MIN_VALID_SAMPLES             2
MIN_VALID_SAMPLE_RATIO_BPS    9000
```

Nightly profile overrides use
`NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_` and
`NOVOVM_MAINLINE_NIGHTLY_SOAK_24H_` with the same keys. Profile overrides take
precedence over the global values. The existing profile, path, interval,
duration and metric-threshold configuration remains available. No setting
turns invalid source evidence into a valid sample.

Invalid configuration fails before observation starts:

- Profiles are restricted to `1h`, `6h` and `24h`; shorter smoke runs use a
  duration override rather than an invented profile name.
- Duration is 1 through 604,800 seconds. The interval is at least one second
  and no greater than the requested duration.
- The planned sample bound, `ceil(duration / interval) + 1`, must not exceed
  100,000. `min_valid_samples` must be at least two and fit within that bound.
- `max_snapshot_age_ms` must cover at least one sampling interval.
- `min_valid_sample_ratio_bps` is 1 through 10,000. Rate thresholds must be
  finite and nonnegative; basis-point metric thresholds cannot exceed 10,000.

A standalone smoke against an already running exporter can use:

```powershell
$env:NOVOVM_MAINLINE_SOAK_PROFILE = "6h"
$env:NOVOVM_MAINLINE_SOAK_MODE = "workload"
$env:NOVOVM_MAINLINE_SOAK_DURATION_SECONDS = "120"
$env:NOVOVM_MAINLINE_SOAK_INTERVAL_SECONDS = "5"
$env:NOVOVM_MAINLINE_SOAK_REPORT_PATH = "artifacts/mainline/mainline-smoke-120s.json"
cargo run -p novovm-node --bin supervm-mainline-soak
```

Use a fresh PowerShell session for the subsequent full run, or clear the smoke
overrides. The exporter must refresh at least as often as sampling requires to
meet the valid-sample ratio. For idle exporter checks, set `MODE=idle_health`
and retain that scope in all reporting. These commands consume an exporter;
they do not start a node or generate traffic.

## Workflow runtime and evidence collection

`.github/workflows/mainline-nightly-soak.yml` still requires a registered,
online self-hosted runner and a live worker snapshot source. Its default
profiles execute sequentially for 30 hours, plus setup and the separate
regression gates. The job budget is explicitly 2,160 minutes (36 hours).
This exceeds the default 360-minute job timeout but stays within the documented
self-hosted execution limit. See [GitHub workflow timeout documentation](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idtimeout-minutes)
and [Actions limits](https://docs.github.com/en/actions/reference/limits).

Increasing the job timeout does not extend `GITHUB_TOKEN` beyond its documented
24-hour lifetime. Long-job authenticated operations and final artifact delivery
must be verified with the eventual runner and collection arrangement before
operational signoff. Daily scheduling with a serialized 30-hour job also does
not imply one completed run per day. This change does not register a runner,
install credentials, trigger a workflow or certify a real long run.

The native pipeline's separate production-profile check retains its two-second
budget. Its step explicitly says smoke and writes
`native-pipeline-production-smoke.json` and the matching `-summary.json`.
The underlying `30min` profile name does not turn that check into a
thirty-minute soak. Full-duration native pipeline examples elsewhere retain
their independent scope.

## Required regression evidence

The gate must reject a frozen old file even when the sampler waits long enough
and all optional metric thresholds would otherwise pass. Regression coverage
must include duplicate timestamps, future/stale/wrong-chain/wrong-schema
snapshots, timestamp or cumulative-counter rollback, insufficient valid
samples, low valid-sample ratio and workload with zero sampled body progress.
It also covers replayed pre-run snapshots, observation-clock rollback,
coverage gaps and a final read that is not fresh.

Positive fixtures must contain genuinely advancing fresh snapshots. Falling
per-round body/header/request values are allowed and must be summed according
to their producer semantics. Idle-health and short-smoke success must never
promote to nightly success. Unit and short integration fixtures establish
these checks, not a real six-hour or twenty-four-hour operational result.

## Local verification (2026-09-16)

The original stopped-exporter audit fixture (`updated_at_unix_ms=1`, zero
progress) was rerun unchanged for two seconds. It now writes a v2 failure
report and exits with code 1: all three reads are stale and none count as a
valid sample. This closes the original false-PASS reproduction without
altering the fixture.

Local Windows verification passed: 15 sampler unit tests, seven duty-report
tests, seven CLI black-box tests, the nightly acceptance unit test, the
strengthened mainline lockset test, node all-target Clippy with warnings denied,
format checking and the complete canonical mainline gate including preflight.
The positive CLI tests use a synthetic exporter; the long-duration unit case
uses an injected observation clock. Neither is a real 6h/24h operational run.
