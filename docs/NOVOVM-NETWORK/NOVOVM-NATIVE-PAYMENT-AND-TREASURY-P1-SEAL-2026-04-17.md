# NOVOVM Native Payment and Treasury Settlement P1 Seal (2026-04-17)

## Purpose

This document seals completed scope for `P0 + P1-A + P1-B + P1-C`, freezes the current mainline wording, and prevents future enhancements from being misread as already released capabilities.

## Phase status

- `P0`: accepted
- `P1-A (Quote Engine)`: accepted
- `P1-B (Clearing Engine)`: accepted
- `P1-C (Treasury Settlement Full Path)`: accepted

## Implemented boundaries (code facts)

### Native transaction and primary entry
- `nov_*` is first-class mainchain entry.
- Native transaction triplet (`Transfer / Execute / Governance`) is on mainline.
- At least one native module has real execution closure with receipt and query visibility.

### Fee and settlement mainline
- `Execution Fee -> SettledFee(NOV)` is wired into runtime mainline.
- `pay_asset == NOV` follows direct NOV settlement.
- `pay_asset != NOV` has minimum real clearing path (not placeholder-only).

### Quote and clearing
- Quote includes source-priority, freshness checks, standardized failures `fee.quote.*`, and receipt metadata.
- Clearing includes route selection, liquidity checks, TTL/slippage/max-pay checks, standardized failures `fee.clearing.*`.

### Treasury settlement
- `quote -> settle -> journal` is wired.
- `redeem` writes settlement journal entries.
- Accounting snapshot is queryable (net-settlement and bucket consistency checks).

### Query surface
- `nov_getTreasurySettlementSummary`
- `nov_getTreasurySettlementPolicy`
- `nov_getTreasurySettlementJournal`

## Authoritative failure-code boundaries

- Quote phase: `fee.quote.*`
- Clearing phase: `fee.clearing.*`
- Do not mix quote and clearing failure prefixes.

## Receipt fields now treated as core

- `settled_fee_nov`
- `paid_asset`
- `paid_amount`
- `fee_quote_id`
- `fee_quote_contract`
- `fee_quote_required_pay_amount`
- `fee_quote_expires_at_unix_ms`
- `fee_clearing_route_ref`

## Not included in P1 (explicit non-claims)

- Multi-source AMM aggregated pricing
- Advanced route aggregation strategies
- Full advanced treasury risk policy parameterization
- Full treasury macro-policy automation

## Stable external wording

`The NOV native payment mainline has a complete minimum closure from quote and clearing to treasury settlement; multi-source aggregation and advanced risk policy remain post-P1 enhancements and do not affect current mainline validity.`

## Suggested next-phase naming

- `P2-A`: minimum multi-route clearing aggregation (sealed in a dedicated P2-A document)
- `P2-B1`: multi-source route/liquidity aggregation
- `P2-B2`: risk policy parameterization
- `P2-C`: advanced treasury policy/reserve strategy

## Follow-up phase seal

- `docs/NOVOVM-NETWORK/NOVOVM-CLEARING-ROUTER-P2A-SEAL-2026-04-17.md`

## Frozen references

- `docs/NOVOVM-NETWORK/NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
- `docs/NOVOVM-NETWORK/NOVOVM-NATIVE-TX-AND-EXECUTION-INTERFACE-DESIGN-2026-04-17.md`
