# NOVOVM Economics

This directory contains NOVOVM's upper-layer economic boundaries, monetary rules, settlement rules, and asset-lifecycle policy.

Boundary:

- This directory owns `NOV / M0 / M1 / M2 / Treasury / AMM / Protocol Clearing Price` rules.
- `docs/NOVOVM-NETWORK` owns network, node, ingress, execution, and runtime-system documents.
- Economic boundary clauses must not be placed under `docs/NOVOVM-NETWORK`, so monetary and settlement policy stay separate from node/runtime documentation.
- DAPP, website, and wallet work are outside this directory's current scope.

Current authoritative documents:

- [NOV first-release economic contract, 2026-10-24](NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md)
  is authoritative for native NOV issuance and current commercial boundaries:
  100,000,000 NOV fully issued at genesis, 9 decimals (10^17 atomic units),
  purpose-bound locks/releases, no later native NOV mint or burn, separate
  treasury/founder/service payables, funded promotion and reviewed staged
  registration airdrops. Allocation amounts, release tables and unapproved
  percentages remain undecided. This is confirmed policy, not completed wiring
  or production acceptance.
- `NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
- `NOVOVM-DUAL-TRACK-SETTLEMENT-AND-MARKET-SYSTEM-P2A-2026-04-17.md`

The April documents retain their monetary-layer and pool-isolation boundaries;
the October 24 contract overrides conflicting native NOV issuance/burn or
commercial instructions. Historical mint/burn implementations and regression
evidence are reusable only after adapting to that contract, not release policy.

Proposals (not frozen genesis parameters or implemented settlement):

- [NOVONET service billing design v1, revised 2026-10-24](NOVONET-SERVICE-BILLING-V1-2026-10-14.md).
  NOV payments and settlement, non-transferable resource allowances, funded
  provider budgets, and explicit gross/net fee mapping. No live billing yet.
- [NOVONET L3/L4 contribution rewards, 2026-10-13](NOVONET-CONTRIBUTION-REWARDS-DRAFT-2026-10-13.md).
  The user confirmed default-enabled bounded L4 participation and genesis-defined
  reward policy; allocation weights, release schedule and pricing remain proposals.
  Former burn splits, 30-day month assumptions and mint-based release are not
  current first-release instructions. Existing test/data evidence remains intact.
