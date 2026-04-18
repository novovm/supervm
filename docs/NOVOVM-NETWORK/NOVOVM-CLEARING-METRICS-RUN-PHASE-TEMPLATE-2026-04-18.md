# NOVOVM Clearing Metrics Run-Phase Report Template (2026-04-18)

Status: OPERATIONAL TEMPLATE (Authoritative for run-phase reporting format)  
Scope: Day-1 / Week-1 reporting format and one-click export flow for sealed P2-D observability

## Purpose

This template standardizes run-phase reporting after P2-D seal:

- one-click data export from `nov_*` observability queries
- stable daily report structure
- stable weekly rollup structure

This template does not enable P3 features. P3 remains `Decision Only / Not Enabled`.

## One-click export

Use the script below from repo root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/novovm-p2d-daily-report.ps1
```

Optional parameters:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/novovm-p2d-daily-report.ps1 `
  -RpcUrl "http://127.0.0.1:8899" `
  -DayLabel "2026-04-18" `
  -OutputDir "artifacts/mainline/p2d-run-phase/2026-04-18" `
  -JournalLimit 100
```

Generated artifacts:

- `NOVOVM-CLEARING-METRICS-REPORT-<day>.md`
- `export-summary.json`
- `raw/clearing_metrics.json`
- `raw/policy_metrics.json`
- `raw/settlement_summary.json`
- `raw/clearing_summary.json`
- `raw/settlement_policy.json`
- `raw/execution_trace.json`
- `raw/settlement_journal.json`

## Daily report template

```markdown
# NOVOVM Clearing Metrics Report - <YYYY-MM-DD>

- Generated at (UTC):
- RPC endpoint:
- Raw snapshot directory:

## 1. Clearing Overview

| Metric | Value |
| --- | ---: |
| total_clearing_attempts | |
| successful_clearings | |
| failed_clearings | |
| success_rate | |

## 2. P3 Decision Inputs (Current Snapshot)

- denominator (attempts_non_nov excluding quote_expired and clearing_disabled):
- route_unavailable:
- insufficient_liquidity:
- slippage_exceeded:
- included_failure_total:
- blocked_state_hits:

Failure counts (all):
- ...

## 3. Policy Snapshot

- policy_contract_id:
- policy_source:
- threshold_state:
- constrained_strategy:

threshold_state_hits:
- ...

constrained_strategy_hits:
- ...

## 4. Settlement Buckets Snapshot

- reserve bucket NOV:
- fee bucket NOV:
- risk_buffer bucket NOV:
- journal total entries:

## 5. Execution Trace Snapshot

- trace_found:
- trace_tx_hash:
- trace_final_status:
- trace_final_failure_code:

## 6. Operator Notes

- Decision statement: P3 remains Decision Only / Not Enabled unless threshold policy is satisfied.
- Observation:
- Action:
```

## Week-1 rollup template

```markdown
# NOVOVM Clearing Metrics Weekly Rollup - Week 1

## 1. Window
- Start (UTC):
- End (UTC):
- Included day reports:

## 2. Aggregated Clearing Indicators
- attempts_non_nov_7d:
- route_unavailable_7d:
- insufficient_liquidity_7d:
- slippage_exceeded_7d:
- failure_combined_rate_7d:

## 3. Policy and Risk Indicators
- blocked_ratio_7d:
- threshold_state distribution:
- constrained_strategy distribution:
- risk alerts:

## 4. Decision
- P3-A decision: hold / enable-canary
- Reason:
- Next checkpoint:
```

## Fixed boundary

- Do not change sealed P2-C/P2-D semantics in this report.
- Do not claim P3 enablement from a single-day snapshot.
- Keep report terminology stable (`route_unavailable`, `insufficient_liquidity`, `slippage_exceeded`, `blocked`).
