# NOVOVM P3-A Gate Weekly Run-Window Template (2026-04-18)

Status: OPERATIONAL TEMPLATE (Authoritative for weekly decision reporting format)  
Scope: 7-day real run-window report format for P3-A enable/hold decisions  
Depends on:
- `NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md`
- `NOVOVM-P3-FEATURE-GATE-DECISION-THRESHOLDS-2026-04-18.md`

## Purpose

This template standardizes the first production-grade weekly decision report:

- consistent metric denominator
- consistent inclusion/exclusion rules
- explicit enable/hold outcome
- explicit rollback/fuse checks

## Fixed metric definitions

Population and denominator are fixed:

- population: `pay_asset != NOV` clearing attempts only
- window: rolling 7 days (UTC)
- included failures:
  - `fee.clearing.route_unavailable`
  - `fee.clearing.insufficient_liquidity`
  - `fee.clearing.slippage_exceeded`
- excluded from denominator:
  - `fee.quote.quote_expired`
  - `fee.clearing.clearing_disabled`

## Input files (minimum set)

For each day in the 7-day window, collect:

- `NOVOVM-CLEARING-METRICS-REPORT-<day>.md`
- `export-summary.json`
- `raw/clearing_metrics.json`
- `raw/policy_metrics.json`
- `raw/settlement_summary.json`
- `raw/clearing_summary.json`

## Report template

```markdown
# NOVOVM P3-A Gate Weekly Decision Report - <YYYY-MM-DD>

Status: DECISION INPUT (P3 remains disabled unless explicitly approved)
Window:
- Start (UTC):
- End (UTC):
- Included daily reports:

## 1. Aggregated clearing indicators (7d)
- attempts_non_nov_7d:
- route_unavailable_7d:
- insufficient_liquidity_7d:
- slippage_exceeded_7d:
- failure_combined_7d:
- failure_combined_rate_7d:

## 2. Policy and risk indicators (7d)
- blocked_ratio_7d:
- threshold_state distribution:
- constrained_strategy distribution:
- risk_buffer alerts:

## 3. P3-A enable conditions check
- condition A ((route_unavailable + insufficient_liquidity) / attempts >= 20%): pass/fail
- condition B (slippage_exceeded / attempts >= 10%): pass/fail
- condition C (blocked_ratio < 5%): pass/fail
- condition D (no active risk alert): pass/fail
- final enable eligibility: yes/no

## 4. Rollback/fuse checks
- combined failure >= 25% over continuous 24h: yes/no
- blocked >= 10% over continuous 24h: yes/no
- risk alert active: yes/no
- rollback required: yes/no

## 5. Decision
- Decision: HOLD / ENABLE-CANARY-10%
- Reason:
- Owner:
- Timestamp (UTC):

## 6. If ENABLE-CANARY-10%, rollout constraints
- Stage sequence: 10% -> 25% -> 50% -> 100%
- Minimum observation per stage: 24h (72h recommended)
- Promotion rule: no rollback condition hit in stage

## 7. Audit attachments
- Metrics snapshot bundle path:
- Raw evidence paths:
- Trace references:
```

## Fixed boundary

- This template does not enable P3 by itself.
- Any enablement must be recorded in a separate dated decision file.
- Keep feature flags unchanged unless decision section explicitly approves canary.
