# NOVOVM P3 Feature Gate Decision Thresholds (2026-04-18)

Status: AUTHORITATIVE  
Scope: Decision policy for enabling or rolling back P3 routing features  
Depends on: `NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md`

## Purpose

This document upgrades P3 activation from recommendation to executable decision policy.  
P3 remains disabled by default. Any enablement must satisfy measurable thresholds from sealed P2-D trace and metrics.

## Fixed metric definitions (anti-ambiguity)

Population and denominator are fixed:

- population: non-NOV clearing attempts only (`pay_asset != NOV`)
- window: rolling 7 days (UTC-based time normalization)
- included failure codes:
  - `fee.clearing.route_unavailable`
  - `fee.clearing.insufficient_liquidity`
  - `fee.clearing.slippage_exceeded`
- excluded from denominator and failure-rate evaluation:
  - `fee.quote.quote_expired`
  - `fee.clearing.clearing_disabled`

Notation:

- `attempts_non_nov_7d`: count(population)
- `failure_combined_7d`: count(included failures)
- `failure_combined_rate_7d = failure_combined_7d / attempts_non_nov_7d`

## Metric query reference (pseudo SQL)

```sql
WITH base AS (
  SELECT *
  FROM execution_traces
  WHERE pay_asset <> 'NOV'
    AND created_at_utc >= now_utc - interval '7 day'
),
denom AS (
  SELECT *
  FROM base
  WHERE COALESCE(final_failure_code, '') NOT IN (
    'fee.quote.quote_expired',
    'fee.clearing.clearing_disabled'
  )
),
agg AS (
  SELECT
    COUNT(*) AS attempts_non_nov_7d,
    SUM(CASE WHEN final_failure_code = 'fee.clearing.route_unavailable' THEN 1 ELSE 0 END) AS route_unavailable_7d,
    SUM(CASE WHEN final_failure_code = 'fee.clearing.insufficient_liquidity' THEN 1 ELSE 0 END) AS insufficient_liquidity_7d,
    SUM(CASE WHEN final_failure_code = 'fee.clearing.slippage_exceeded' THEN 1 ELSE 0 END) AS slippage_exceeded_7d
  FROM denom
)
SELECT *,
       (route_unavailable_7d + insufficient_liquidity_7d + slippage_exceeded_7d)
         * 1.0 / NULLIF(attempts_non_nov_7d, 0) AS failure_combined_rate_7d
FROM agg;
```

## Default feature flags

- `enable_multi_hop = false`
- `enable_split = false`

## P3-A (multi-source enhancement) decision policy

### Enable conditions (all required unless stated OR)

- (`route_unavailable + insufficient_liquidity`) / `attempts_non_nov_7d` >= `20%`  
  OR `slippage_exceeded / attempts_non_nov_7d` >= `10%`
- `threshold_state=blocked` ratio < `5%` (rolling 7d)
- no active risk-buffer alert in risk summary

### Rollback/fuse conditions (any one triggers rollback)

- combined failure rate >= `25%` for continuous `24h`
- `threshold_state=blocked` ratio >= `10%` for continuous `24h`
- risk-buffer alert active

### Rollout policy (mandatory canary)

- traffic stages: `10% -> 25% -> 50% -> 100%`
- each stage observation window: `24h` minimum (`72h` recommended)
- promotion requires no rollback condition hit in current stage

## P3-B (multi-hop) decision policy

### Enable conditions

- P3-A stable for `14 consecutive days`
- offline replay median improvement >= `8%`
- full CI/mainline gate green

### Rollback/fuse conditions

- all P3-A rollback conditions apply
- path-specific failure growth beyond baseline tolerance

## P3-C (split routing) decision policy

### Enable conditions

- P3-B stable for `30 consecutive days`
- P95 slippage remains >= `target + 2%`
- offline split replay improves output without raising failure rate

### Rollback/fuse conditions

- all P3-A rollback conditions apply
- split-specific failure growth beyond baseline tolerance

## Audit and trace requirements

Any enable/rollback decision must be explainable from:

- execution trace (`candidate_routes`, `selected_route`, failure code, policy context)
- clearing metrics summary
- policy metrics summary
- settlement/risk summaries

## Explicit non-claims

This document is decision policy only:

- it does not enable P3 by itself
- it does not redefine sealed P2-C/P2-D contracts
- it does not introduce auto-tuning or strategy-solver behavior

