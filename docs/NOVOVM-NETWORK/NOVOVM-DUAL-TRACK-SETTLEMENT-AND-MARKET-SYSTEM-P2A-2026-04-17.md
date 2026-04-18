# NOVOVM Dual-Track Settlement and Market Pricing System (P2-A Freeze)
_2026-04-17_

## 1. Purpose

This document converges boundaries across monetary, reserve, clearing, and market-pricing systems as the `P2-A` implementation freeze.

The target is not feature expansion. The target is enforceable institutional boundaries that prevent accounting, pricing, and responsibility drift.

## 2. Relationship to completed phases

- `P0`: accepted (native transaction triplet and native execution entry established)
- `P1-A`: accepted (quote mainline established)
- `P1-B`: accepted (minimum real clearing mainline established)
- `P1-C`: accepted (settlement accounting mainline, journal, snapshot, and query established)

This document defines `P2-A`: upgrade `pay_asset != NOV` from minimum clearing to a minimal route-competition mainline with both rule-track and market-track paths.

## 3. Three-pool structure (frozen)

### 3.1 Reserve Settlement Pool (RSP)

Sources: execution-fee inflow, clearing income, and external settlement inflow.

Roles:

1. Supports rule-based settlement exchange.
2. Provides system risk buffer.
3. Serves as one of the base reserves for credit-expansion constraints.

### 3.2 Mirror Custody Pool (MCP)

Sources: locked external assets (for example ETH/USDT locked on plugin-side contracts).

Roles:

1. Supports 1:1 redemption rollback for mirrored assets.
2. Maintains correspondence between custody assets and mirrored liabilities.

Hard constraint: MCP assets must not be used for market making and must not become free system reserves.

### 3.3 Market Liquidity Pool (MLP)

Sources: market-making liquidity from system deployment and/or external LPs.

Roles:

1. Market price discovery.
2. Depth and slippage buffering.
3. Arbitrage convergence channel.

## 4. Dual-track price system (frozen)

### 4.1 Rule-based settlement price (Track A)

Determined by transparent rules and not equal to market spot.

Reference form:

`settlement_price = reference_price * reserve_haircut * risk_haircut * fee_factor`

Characteristics: limited capacity, auditable parameters, governance-adjustable.

### 4.2 Market trading price (Track B)

Determined by AMM/order-book markets and allowed to float.

Characteristics: no fixed redemption promise, formed by liquidity and trades.

### 4.3 Frozen conclusion

- Settlement price is not market price.
- AMM price is not rule-based settlement price.
- The two converge in a range through arbitrage, with distinct responsibilities.

## 5. Asset layering (frozen)

### 5.1 NOV

Base settlement currency (core of M0/M1).

### 5.2 Mirrored assets

Use `m*` naming (for example `mETH`, `mUSDT`) to represent 1:1 custody mapping and redemption rights.

### 5.3 Credit-expansion assets

Reserve `n*` naming (for example `nUSD`, `nRWA`) for M2.

Frozen constraint: mirrored assets and credit-expansion assets must not share naming semantics or risk controls.

## 6. Alignment with current code mainline

### 6.1 Already established

- `fee.quote.*` and `fee.clearing.*` failure-code boundaries are separated.
- `quote -> clearing -> settlement` minimum closure exists.
- `settled_fee_nov / paid_asset / paid_amount / route_ref` are visible.
- Settlement journal and accounting snapshot are queryable.

### 6.2 Remaining gap for P2-A

- Route sources are still minimum and narrow.
- Market-track and rule-track have not yet been normalized into one multi-route selection mainline.
- Multi-source selection and fallback policies are not yet frozen.

## 7. P2-A implementation freeze (minimum executable)

### 7.1 Scope

Implement a minimal multi-route aggregator only. Do not build a complex global aggregator at this stage.

### 7.2 Minimum model

1. Route sources must include at least:
   - `reserve_direct`
   - `amm_pool`
2. Router flow is fixed:
   - `quote_routes`
   - `select_best_route`
   - `execute_selected_route`
3. Selection policy is initially fixed to `max_expected_out`.

### 7.3 Failure codes (keep prefix freeze)

- `fee.clearing.route_unavailable`
- `fee.clearing.insufficient_liquidity`
- `fee.clearing.quote_expired`
- `fee.clearing.slippage_exceeded`

Do not introduce mixed-prefix failure codes.

### 7.4 Required receipt/query route fields

- `route_id`
- `route_source`
- `expected_nov_out`
- `route_fee_ppm`

Keep existing settlement fields:

- `settled_fee_nov`
- `paid_asset`
- `paid_amount`

## 8. Risk-control boundaries (P2-A)

1. Hard quote TTL checks.
2. Hard slippage checks.
3. Hard `max_pay_amount` checks.
4. Insufficient liquidity must fail explicitly; no silent degraded settlement.
5. MCP and MLP accounting isolation; no cross-pool asset reuse.

## 9. Acceptance gates (P2-A)

1. `pay_asset != NOV` can select between at least two routes.
2. Route unavailable and insufficient liquidity return stable standardized failure codes.
3. Successful receipts include route metadata.
4. Clearing results continue into settlement mainline and do not bypass internal NOV settlement.
5. `cargo check / clippy / test / supervm-mainline-gate` all green.

## 10. Replacement and conflict rule

If implementation conflicts with this document:

1. Update this document first with explicit deviation rationale.
2. Then change implementation.
3. Code-only deviation without doc update is not a valid resolution.

---

This document is the institutional freeze for `P2-A`: establish rule-track settlement + market-track trading + three-pool isolation before complex aggregation and advanced policy expansion.
