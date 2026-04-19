# NOVOVM Current Authoritative Documentation Entry Point (2026-04-17)

## Purpose

This file defines the current authoritative scope and separates it from archival and migration-era documents, so historical files are not treated as current production policy.

## Current authoritative entry points (priority order)

1. Repository root README (product positioning and mainline entry)
   - `README.md`
2. NOV native monetary and execution baseline
   - `docs/NOVOVM-NETWORK/NOVOVM-CORE-PLUGIN-EXTERNAL-LAYER-MAP-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-NATIVE-TX-AND-EXECUTION-INTERFACE-DESIGN-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-NATIVE-PAYMENT-AND-TREASURY-P1-SEAL-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-DUAL-TRACK-SETTLEMENT-AND-MARKET-SYSTEM-P2A-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-CLEARING-ROUTER-P2A-SEAL-2026-04-17.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-STAGE2-SEAL-2026-04-18.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-CONSTRAINED-STRATEGY-SEAL-2026-04-18.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-SEAL-2026-04-18.md` (FINAL)
   - `docs/NOVOVM-NETWORK/NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md` (FINAL)
3. P3 feature gate decision policy (decision only, not enabled)
   - `docs/NOVOVM-NETWORK/NOVOVM-P3-FEATURE-GATE-DECISION-THRESHOLDS-2026-04-18.md` (AUTHORITATIVE)
   - `docs/NOVOVM-NETWORK/NOVOVM-P3-GATE-DRYRUN-RESULT-2026-04-18.md` (RECORDED RESULT)
   - `docs/NOVOVM-NETWORK/NOVOVM-P3A-GATE-WEEKLY-RUN-WINDOW-TEMPLATE-2026-04-18.md` (OPERATIONAL TEMPLATE)
4. P2-D run-phase reporting template and exporter
   - `docs/NOVOVM-NETWORK/NOVOVM-CLEARING-METRICS-RUN-PHASE-TEMPLATE-2026-04-18.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-FULL-MODE-MINIMAL-BOOTSTRAP-TEMPLATE-2026-04-18.md`
   - `scripts/novovm-p2d-daily-report.ps1`
5. Mainline status and delivery contract artifacts
   - `artifacts/mainline-status.json`
   - `artifacts/mainline-delivery-contract.json`
   - `artifacts/mainline/mainline-nightly-soak-gate-report.json`

## Historical/archival documents (not current by default)

The following directories are historical context unless a file explicitly marks itself as current/active:

- `docs_CN/Old Design/`
- `docs_CN/MEV/`
- `docs_CN/SVM2026-MIGRATION/`
- `docs_CN/AOEM-FFI/archive/`
- date-stamped phase audit files under `artifacts/audit/`

## Conflict resolution rules

When documentation conflicts occur, resolve in this order:

1. Code and executable gate results (CI/mainline/nightly)
2. `artifacts/mainline-status.json` and `artifacts/mainline-delivery-contract.json`
3. The entry points listed in this file
4. Other documents (informational only)

## Maintenance requirements

- If you add a new runtime or gate entry, update this file in the same change.
- Historical files must not claim current status without explicit date and scope.
- If the project moves to `P2-B1/P2-B2/P2-C/P2-D/P3`, publish a phase seal with completed/not-completed boundaries before updating this entry point.

## Term freeze (avoid role inversion)

- `NOVOVM/SUPERVM`: host system
- `AOEM`: unified execution engine
- `EVM`: plugin capability (guest), not the host system

Recommended external phrasing:

- "The EVM plugin mainline is in maintenance mode."
- Avoid "EVM host mainline", which can be misread as "EVM is the host."
