# NOVOVM Unified Policy Core Seal Audit 2026-04-07

## Conclusion

The current mainline policy system is sealed around a unified Rust core.

Normal main path:

- `novovm-rollout-policy`

Compatibility path:

- explicit compatibility script shell
- explicit `binary_file` override only
- unified entrypoint flat legacy tool-name dispatch only

Emergency fallback path:

- PowerShell startup shell
- PowerShell ops shell
- minimal conservative fallback only

There is no longer any valid mainline assumption that a deleted per-tool wrapper executable will be auto-discovered as the default runtime path.

## Layer 1: normal main path

The following domains are treated as unified Rust-core domains:

- `overlay`
- `failover`
- `risk`
- `rollout`

Normal policy execution must go through:

- `novovm-rollout-policy overlay ...`
- `novovm-rollout-policy failover ...`
- `novovm-rollout-policy risk ...`
- `novovm-rollout-policy rollout ...`

Normal main path also includes flat legacy-name dispatch through the same unified entrypoint, for example:

- `novovm-rollout-policy overlay-relay-discovery-merge ...`
- `novovm-rollout-policy risk-action-eval ...`
- `novovm-rollout-policy rollout-decision-route ...`

These are compatibility names on top of the same unified Rust core, not separate executables.

## Layer 2: explicit compatibility path

Compatibility remains allowed only in these forms:

- root `scripts/*.ps1` thin shell
- explicit `binary_file` override in runtime policy config
- unified `novovm-rollout-policy` flat legacy-name dispatch

Compatibility is not allowed to reintroduce a second real implementation.

## Layer 3: emergency fallback path

PowerShell remains valid only for:

- startup
- runtime env injection
- audit emission
- operator shell
- minimal conservative fallback when unified Rust runtime is unavailable

PowerShell is not a valid place for a full second strategy brain.

## Root-script status

### Mainline shells kept

- `scripts/novovm-up.ps1`
- `scripts/novovm-prod-daemon.ps1`
- `scripts/novovm-node-rollout.ps1`
- `scripts/novovm-node-rollout-control.ps1`
- `scripts/novovm-node-lifecycle.ps1`
- `scripts/novovm-ua-prod-store.ps1`
- `scripts/novovm-l1l4-settlement-cycle.ps1`
- `scripts/novovm-l1l4-reconcile.ps1`
- `scripts/novovm-l1l4-reconcile-daemon.ps1`
- `scripts/novovm-l1l4-real-broadcast.ps1`
- `scripts/novovm-l1l4-payout-execute.ps1`
- `scripts/novovm-l1l4-external-confirm.ps1`
- `scripts/novovm-l1l4-auto-payout.ps1`
- `scripts/novovm-generate-role-matrix.ps1`

### Thin-wrapper roots already sealed

- `scripts/novovm-rollout-decision-dashboard-export.ps1`
- `scripts/novovm-rollout-decision-dashboard-consumer.ps1`
- `scripts/novovm-overlay-relay-health-refresh.ps1`
- `scripts/novovm-overlay-relay-discovery-merge.ps1`

## Legacy executable status

Batch 1 compatibility-only wrapper executables have been physically retired from `crates/novovm-rollout-policy/src/bin/*`.

Compatibility now remains only through:

- unified `novovm-rollout-policy`
- explicit shell wrappers
- explicit configuration override

## Frozen history asset pool

The following directory is outside the current mainline policy-core cleanup path:

- `scripts/migration/*`

It is a frozen history/migration asset pool and should only be modified if explicitly reactivated into the production path.

## Seal rules

1. Normal main path must resolve to unified Rust core.
2. Deleted per-tool wrapper executables must not be auto-discovered as default runtime paths.
3. PowerShell may preserve startup and emergency behavior only.
4. New policy logic must enter the shared Rust core first.
5. Compatibility surfaces must not contain independent strategy logic.

## Operational meaning

If an operator sees behavior divergence, the first assumption must be:

- unified Rust core logic

and not:

- hidden script logic
- hidden dedicated wrapper executable logic

This is the intended sealed operating model.
