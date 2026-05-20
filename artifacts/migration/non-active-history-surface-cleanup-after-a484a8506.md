# Non-Active History Surface Cleanup After a484a8506

## Scope

This is a governance / documentation / guard patch. It does not change EVM
execution semantics, RLPx handshake behavior, UA RocksDB handling, `eth_baseFee`,
`balHash`, or eth/71 capability behavior.

The goal is to prevent historical material from being read as current NOVOVM EVM
acceptance or current Ethereum capability support.

## Search Terms

Manual grep covered these high-risk wording groups:

- strategy-specific external observation acronym and specific DEX family name
- `strategy`, `selector`, `router`, `autopilot`, and priority-scoring wording
- public RLPx readiness overclaims
- eth/71 / BAL support overclaims
- `SuperVM` / `SUPERVM` external naming drift
- gateway / host entrypoint ambiguity

## Findings

- A disabled legacy daemon script still described the old name as the policy
  owner instead of NOVOVM.
- The core / plugin / external layer map used a dual-brand host label that could
  be read as current external naming.
- Historical strategy-research material under `docs_CN` did not have a local
  archive marker stating that it is not active NOVOVM EVM acceptance.
- Migrated WEB30 source snapshots used historical source wording without a local
  note separating source-era naming from current NOVOVM naming.

## Actions Taken

- Reworded the disabled legacy daemon script to NOVOVM single-mainline policy and
  marked `SUPERVM` as an internal historical code name only.
- Reworded English and Chinese core / plugin / external layer maps so the host is
  NOVOVM, while `SUPERVM` is limited to repo/path/internal historical wording.
- Added an archive marker for historical strategy-research material under
  `docs_CN`.
- Added historical-source and migration notes to WEB30 reference material.
- Added a machine guard:
  `scripts/migration/assert_non_active_history_surface_boundary.ps1`.

## Historical / Archived Material Handling

Historical files were not bulk-rewritten. The patch adds explicit local markers
where older material is most likely to be mistaken for active acceptance:

- archived strategy-research directory
- migrated `SVM2026` source snapshots
- WEB30 migration index

These markers preserve audit traceability while preventing current acceptance or
product naming drift.

## Active Surface Guard

The new guard verifies that:

- historical strategy-research material is marked archived / non-active
- historical strategy material is not active NOVOVM EVM acceptance
- migrated source snapshots distinguish source-era wording from NOVOVM naming
- disabled legacy daemon wording uses NOVOVM policy ownership
- layer maps do not use `NOVOVM / SUPERVM (Host)` as current host wording
- RLPx reports do not claim public readiness where only local evidence exists
- eth/71 / BAL design material remains design-only and no-advertise

It complements:

- `scripts/migration/assert_geth_upstream_compat_boundary.ps1`
- `scripts/migration/assert_no_strategy_surface_in_supervm.ps1`

## Not Claimed

- No EVM execution semantic change.
- No geth RPC compatibility change.
- No RLPx handshake semantic change.
- No UA RocksDB migration or reset behavior change.
- No full eth/71 / BAL implementation.
- No public RLPx session readiness claim.
- No new NOVOVM plugin architecture.

## Diff Audit

In scope:

- `scripts/novovm-prod-daemon.ps1`
- `scripts/migration/assert_non_active_history_surface_boundary.ps1`
- `docs/NOVOVM-NETWORK/NOVOVM-CORE-PLUGIN-EXTERNAL-LAYER-MAP-2026-04-17.md`
- `docs/README.zh-CN.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-CORE-PLUGIN-EXTERNAL-LAYER-MAP-2026-04-17.md`
- `docs_CN/WEB30-PROTOCOL/SVM2026-REFERENCE/README.md`
- `docs_CN/WEB30-PROTOCOL/WEB30-PROTOCOL-MIGRATION-INDEX-2026-03-05.md`
- historical strategy archive README under `docs_CN`
- `artifacts/migration/non-active-history-surface-cleanup-after-a484a8506.md`

Out of scope and intentionally not touched:

- `crates/gateways/evm-gateway/src/main.rs`
- `crates/novovm-adapter-novovm/src/lib.rs`
- `crates/plugins/evm/core/src/lib.rs`

## Merge Note

This patch marks non-active historical strategy and readiness material so it
cannot be mistaken for current NOVOVM EVM acceptance or current Ethereum
capability support. It also adds a boundary guard that keeps historical wording,
RLPx readiness wording, eth/71 / BAL design wording, and NOVOVM naming aligned
with the sealed migration patches.
