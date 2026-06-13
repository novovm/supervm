# NOVOVM Economics

This directory contains NOVOVM's upper-layer economic boundaries, monetary rules, settlement rules, and asset-lifecycle policy.

Boundary:

- This directory owns `NOV / M0 / M1 / M2 / Treasury / AMM / Protocol Clearing Price` rules.
- `docs/NOVOVM-NETWORK` owns network, node, ingress, execution, and runtime-system documents.
- Economic boundary clauses must not be placed under `docs/NOVOVM-NETWORK`, so monetary and settlement policy stay separate from node/runtime documentation.
- DAPP, website, and wallet work are outside this directory's current scope.

Current authoritative documents:

- `NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
- `NOVOVM-DUAL-TRACK-SETTLEMENT-AND-MARKET-SYSTEM-P2A-2026-04-17.md`
