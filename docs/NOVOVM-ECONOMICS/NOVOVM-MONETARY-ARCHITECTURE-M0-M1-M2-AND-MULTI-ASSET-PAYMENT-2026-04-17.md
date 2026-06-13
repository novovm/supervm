# NOVOVM Monetary Architecture Resolution (M0/M1/M2 and Multi-Asset Payment)
_2026-04-17_

## 1. Purpose and scope

This document freezes the following boundaries to prevent implementation drift:

1. NOV protocol positioning versus EVM/ETH
2. Terminology and implementation boundaries for Gas versus NOVOVM fees
3. M0/M1/M2 monetary layering and the mainline model: external-asset payment, internal NOV settlement

This document is an implementation constraint, not a concept draft.

## 2. Inputs (already cross-checked)

- External macro manuscript:
  - Local source PDF used during review (desktop path in local environment)
- Repository extracts:
  - `artifacts/audit/macro-econ-fulltext-2026-04-17.txt`
  - `artifacts/audit/macro-econ-key-extract-2026-04-17.txt`
- Current implementation points:
  - `crates/novovm-consensus/src/token_runtime.rs` (`NOV` native symbol)
  - `crates/novovm-consensus/src/protocol.rs` (HotStuff/BFT mainline)
  - `crates/novovm-protocol/src/tx_wire.rs` (native tx wire still transfer-centered)
  - `crates/novovm-node/src/tx_ingress.rs` (ingress mapping still transfer-heavy)

## 3. Question 1: NOV protocol versus EVM/ETH

### 3.1 Frozen conclusion

- NOVOVM's advantage is not a TPS slogan. It is execution-first semantics, verifiable settlement, and host-based multi-chain capability.
- External wording may use: `proof-driven execution network`.
- **Current implementation still includes canonical chain, HotStuff, and block lifecycle.**
  Conclusion: this is an execution-proof-oriented on-chain system, not a fully blockless system.

### 3.2 External wording (fixed)

- Allowed: `We are not block-packaging-centric; we are execution-and-verifiable-result-centric.`
- Not allowed: `We are already fully beyond blockchain/block mode.`

## 4. Question 2: GAS and fee model

### 4.1 Frozen conclusion

- Compatibility surfaces may keep `gas_*` fields for EVM tooling.
- NOV native wording must use `Execution Fee` and not use Gas as the main narrative.

### 4.2 Fee model constraints

- Native fee accounting must be resource-itemized: `compute + storage + bandwidth + proof + routing`.
- Internal settlement currency is unique: `NOV`.

## 5. Question 3: M0/M1/M2 and multi-asset payment

### 5.1 Layer freeze (critical)

- `M0`: base money layer, only `NOV`.
- `M1`: circulating money layer, only NOV-system circulating money; no mirrored external assets.
- `M2`: credit expansion layer, where new credit assets (`n*`) are issued.

### 5.2 Explicit prohibitions

- Do not place `pETH / pUSDT / pSOL`-style mirrored assets in M1.
- Do not bypass NOV settlement by treating external assets as internal settlement currency.

### 5.3 M2 generation mainline (frozen)

1. External assets (ETH/USDT/DAI...) are locked on the EVM plugin side.
2. Lock results are booked into NOVOVM treasury reserves.
3. Through clearing/exchange rules, value is converted into NOV collateral basis.
4. `M2` credit assets (`n*`) are minted only when collateral and risk constraints are satisfied.
5. M2 assets can circulate (including RWA-like assets), but their risk belongs to M2 and must not be reclassified as M1.

## 6. Multi-asset payment model (implementation wording)

### 6.1 Core rule

- Users may pay with external assets (ETH/USDT/DAI...).
- The system auto-clears/auto-converts, then settles internally in NOV.

### 6.2 Standard flow

`external-asset payment -> clearing pool/AMM -> NOV settlement -> execution bookkeeping -> treasury reserve/distribution`

### 6.3 Boundary constraints

- Must include quote TTL, slippage protection, and fallback on insufficient liquidity.
- Fee collection must not bypass treasury settlement flow.

## 7. Current code gap (P0 executable)

1. Native `tx_wire` is still transfer-heavy and must be upgraded to express native execute/governance.
2. Native `nov_*` entry exists but NOV-native execution and fee terminology still need stronger "native-mainchain-first" treatment.
3. Multi-asset routing, auto-clearing, and treasury settlement are not yet a single unified module (payment router + quote + clearing + treasury settlement).

## 8. Term freeze

- Brand: `NOVOVM`
- Technical short name: `NVM`
- Base currency: `NOV`
- Native fee term: `Execution Fee`
- EVM `gas` remains compatibility-only and does not define NOV native economic terminology

## 9. Execution priority (next cut only)

1. Implement NOV-native payment/clearing route skeleton (crate-level interface draft).
2. Upgrade native tx wire (express Execute/Governance, no longer transfer-only).
3. Add M2 risk boundaries (collateral ratio, liquidation threshold, treasury backstop, pause switch).

---

This document is used to align monetary boundaries before coding. Future code changes must follow this document unless replaced by a newer formal resolution.
