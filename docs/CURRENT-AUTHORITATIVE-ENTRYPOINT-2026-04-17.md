# NOVOVM Current Authoritative Documentation Entry Point (2026-04-17)

## Purpose

This file defines the current public documentation surface for NOVOVM and lists only the documents that describe current product capabilities, interfaces, runtime operations, and decision policy.

## Current public documentation surface (priority order)

1. Repository root README (product positioning and mainline entry)
   - `README.md`
2. NOV native monetary and execution baseline
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-CURRENT-SYSTEM-ARCHITECTURE-2026-04-19.md` (CURRENT OVERVIEW)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-v1-2026-04-20.md` (AUTHORITATIVE, v1-min)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-KEYALGO-ACCOUNTMODE-EXECUTIONPOLICY-LAYERING-2026-04-20.md` (ACCOUNT CAPABILITY LAYERING RULE)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTA-KEYALGO-SEAL-2026-04-20.md` (CUT A / KEYALGO SEAL)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTA-KEYALGO-IMPLEMENTATION-CHECKLIST-2026-04-20.md` (CUT A / KEYALGO GATE)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-SEAL-2026-04-20.md` (CUT C / EXECUTIONPOLICY SEAL)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-IMPLEMENTATION-CHECKLIST-2026-04-20.md` (CUT C / EXECUTIONPOLICY GATE)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md` (CUT B / ACCOUNTMODE TRIGGER CHECKLIST)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-v1-MIN-TO-v1-EVOLUTION-ROADMAP-2026-04-20.md` (AUTHORITATIVE ROADMAP)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE2-IMPLEMENTATION-CHECKLIST-2026-04-20.md` (PHASE 2 GATE)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT1-SEAL-2026-04-20.md` (PHASE 3 / CUT 1 SEAL)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT2-SEAL-2026-04-20.md` (PHASE 3 / CUT 2 SEAL)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md` (PHASE 3 GATE)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md` (PHASE 4 TRIGGER CHECKLIST)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md` (PHASE 4 CONSTRAINT DRAFT)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md` (PHASE 4 FAILURE MODES)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md` (PHASE 4 TRIGGERED MVP SLICE TEMPLATE)
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md` (UNIFIED-ACCOUNT STRUCTURAL CHANGE GOVERNANCE GATE)
   - `docs/NOVOVM-NETWORK/NOVOVM-CORE-PLUGIN-EXTERNAL-LAYER-MAP-2026-04-17.md`
   - `docs/NOVOVM-ECONOMICS/NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-NATIVE-TX-AND-EXECUTION-INTERFACE-DESIGN-2026-04-17.md`
   - `docs/NOVOVM_NATIVE_TRANSACTION_AUTHENTICATION_V1.md` (ACTIVE NATIVE INGRESS AUTH CONTRACT)
   - `docs/NOVOVM_GENERIC_NATIVE_TRANSACTION_AOEM_STATE_OWNERSHIP_V1.md` (HOST ADAPTER + DOMAIN-NEUTRAL AOEM SEMANTIC GRAPH V3 OWNERSHIP CANDIDATE)
   - `docs/NOVOVM-NETWORK/NOVOVM-NATIVE-PAYMENT-AND-TREASURY-P1-SEAL-2026-04-17.md`
   - `docs/NOVOVM-ECONOMICS/NOVOVM-DUAL-TRACK-SETTLEMENT-AND-MARKET-SYSTEM-P2A-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-CLEARING-ROUTER-P2A-SEAL-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-STAGE2-SEAL-2026-04-18.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-CONSTRAINED-STRATEGY-SEAL-2026-04-18.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-SEAL-2026-04-18.md` (FINAL)
   - `docs/NOVOVM-NETWORK/NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md` (FINAL)
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-ECONOMIC-USER-SURFACE-SEAL-2026-04-18.md` (FINAL)
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-USER-SURFACE-SEAL-2026-04-18.md` (FINAL)
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-MLDSA87-EXTERNAL-VOTE-SEAL-2026-04-18.md` (FINAL)
3. P3 feature gate decision policy (decision only, not enabled)
   - `docs/NOVOVM-NETWORK/NOVOVM-P3-FEATURE-GATE-DECISION-THRESHOLDS-2026-04-18.md` (AUTHORITATIVE)
   - `docs/NOVOVM-NETWORK/NOVOVM-P3-GATE-DRYRUN-RESULT-2026-04-18.md` (RECORDED RESULT)
   - `docs/NOVOVM-NETWORK/NOVOVM-P3A-GATE-WEEKLY-RUN-WINDOW-TEMPLATE-2026-04-18.md` (OPERATIONAL TEMPLATE)
4. P2-D run-phase reporting template and exporter
   - `docs/NOVOVM-NETWORK/NOVOVM-CLEARING-METRICS-RUN-PHASE-TEMPLATE-2026-04-18.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-FULL-MODE-MINIMAL-BOOTSTRAP-TEMPLATE-2026-04-18.md`
   - `scripts/novovm-p2d-daily-report.ps1`
5. Mainline status and delivery contract artifacts
   - `artifacts/mainline-status.json` (generated only by a successful mainline run)
   - `artifacts/mainline-delivery-contract.json` (generated only by a successful mainline run)
   - `artifacts/mainline/mainline-nightly-soak-gate-report.json` (generated only by a successful nightly soak)
6. Product overlay operational runtime (not a claim of public topology completion)
   - `docs/NOVOVM_PRODUCT_MAINLINE_OVERLAY_LIFECYCLE_V1.md`
   - `docs/novovm-product-topology-preflight-v1.md`
   - `docs/novovm-product-relay-daemon-v1.md`
   - `docs/novovm-product-relay-client-v1.md`
   - `docs/novovm-product-node-overlay-v1.md`
   - `docs/novovm-product-nat-runtime-v1.md`
   - `docs/novovm-product-peer-runtime-v1.md`
   - `docs/novovm-product-evidence-v1.md`
   - `scripts/novovm-package-product-linux.ps1`

## Current unified-account wording

The unified account / asset line should currently be read with the following frozen conclusion:

`The unified-account mainline has completed the minimal production closure of the subject layer, asset-view layer, key-capability layer, and execution-policy layer; AccountMode / Cut B remains a non-core optional label layer with default No-Go; Phase 4 remains trigger-governed and is currently No-Go.`

External wording should no longer frame this line as "waiting for the account architecture to settle" or as "still under structural advancement."

The correct current wording is:

`This line has entered a stable baseline for long-term operation and does not require further structural push by default.`

## Public-surface rule

- Only the documents listed in this file are part of the current public documentation surface.
- Repository files not explicitly listed here are engineering reference material, not primary external product documentation.

## Conflict resolution rules

When documentation conflicts occur, resolve in this order:

1. Code and executable gate results (CI/mainline/nightly)
2. `artifacts/mainline-status.json` and `artifacts/mainline-delivery-contract.json`
3. The entry points listed in this file
4. Other documents (engineering reference only, not primary external product material)

## Maintenance requirements

- If you add a new public interface, runtime entry, or gate entry, update this file in the same change.
- Runtime artifacts are evidence only after the corresponding command or CI job succeeds; missing generated artifacts prove no result and must not be replaced with placeholders.
- The product overlay now has an opt-in `novovm-node` native-pipeline lifecycle and AOEM ingress bridge defined by `docs/NOVOVM_PRODUCT_MAINLINE_OVERLAY_LIFECYCLE_V1.md`. Local real-WSS lifecycle coverage is not a claim of a public VPS, NAT, cellular, VPN, or CGNAT result; those claims still require signed topology evidence.
- Current public documents should describe established capabilities, current boundaries, and current reading order rather than development history.
- If the project adds a new sealed capability or decision policy, publish that seal first and then update this entry point.
- Unified account should be read from the current `Account Protocol v1-min` document rather than inferred from legacy or migration material.
- Unified-account wording should explicitly treat the current line as a stable baseline rather than as a still-open structural design track.
- Any unified-account structural-change PR must satisfy `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`; missing trigger payload defaults to `Reject (No-Go)`.
- Any PR that modifies Trigger Checklist / MVP Slice Template / PR Gate itself must include governance proposal + vote + execute evidence; otherwise default `Reject (No-Go)`.

## Term freeze (avoid role inversion)

- `NOVOVM/SUPERVM`: host system
- `AOEM`: unified execution engine
- `EVM`: plugin capability (guest), not the host system

Recommended external phrasing:

- "The EVM plugin mainline is in maintenance mode."
- Avoid "EVM host mainline", which can be misread as "EVM is the host."
