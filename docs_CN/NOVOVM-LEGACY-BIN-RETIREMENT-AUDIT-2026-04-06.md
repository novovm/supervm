# NOVOVM Legacy Bin Retirement Audit 2026-04-06

## Rule

- Normal main path must go through `novovm-rollout-policy`
- Legacy bin may exist only as a compatibility thin wrapper
- A legacy bin can be physically deleted only after external usage is confirmed absent

## Color policy

- Green: can delete now
- Yellow: keep as thin wrapper for compatibility
- Red: temporarily hold, not yet eligible for cleanup

## Green

- None yet

## Yellow

### Overlay
- `crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-auto-profile.rs`
- `crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-relay-discovery-merge.rs`
- `crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-relay-health-refresh.rs`

### Failover
- `crates/novovm-rollout-policy/src/bin/failover/novovm-failover-policy-matrix-build.rs`

### Risk
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-action-eval.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-action-matrix-build.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-blocked-map-build.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-blocked-select.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-level-set.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-matrix-select.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-policy-profile-select.rs`

### Rollout
- `crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-dashboard-consumer.rs`
- `crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-dashboard-export.rs`
- `crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-delivery.rs`
- `crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-route.rs`

## Red

- None yet

## Current conclusion

All current legacy bins are compatibility surfaces only. They are not part of the normal strategy-brain path anymore. Keep them as thin wrappers until external usage is audited, then delete by batch rather than one by one.

## Batch 1 delete candidates

These legacy bins are now first-round physical deletion candidates after default-path cleanup, but should still be deleted only by audited batch once external compatibility usage is confirmed absent:

- `crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-auto-profile.rs`
- `crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-relay-discovery-merge.rs`
- `crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-relay-health-refresh.rs`
- `crates/novovm-rollout-policy/src/bin/failover/novovm-failover-policy-matrix-build.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-action-eval.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-action-matrix-build.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-blocked-map-build.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-blocked-select.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-level-set.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-matrix-select.rs`
- `crates/novovm-rollout-policy/src/bin/risk/novovm-risk-policy-profile-select.rs`
- `crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-dashboard-consumer.rs`
- `crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-dashboard-export.rs`
- `crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-delivery.rs`
- `crates/novovm-rollout-policy/src/bin/rollout/novovm-rollout-decision-route.rs`

## 2026-04-07 Batch 1 physical retirement completed

The following compatibility-only wrapper bins have now been physically removed from `crates/novovm-rollout-policy/src/bin/*` and from `Cargo.toml` bin registration. Compatibility remains through the unified `novovm-rollout-policy` entrypoint and its flat legacy tool-name dispatch.

- `novovm-overlay-auto-profile`
- `novovm-overlay-relay-discovery-merge`
- `novovm-overlay-relay-health-refresh`
- `novovm-rollout-decision-dashboard-export`
- `novovm-rollout-decision-dashboard-consumer`
- `novovm-rollout-decision-delivery`
- `novovm-rollout-decision-route`
- `novovm-risk-action-eval`
- `novovm-risk-matrix-select`
- `novovm-risk-blocked-select`
- `novovm-risk-blocked-map-build`
- `novovm-risk-level-set`
- `novovm-risk-policy-profile-select`
- `novovm-risk-action-matrix-build`
- `novovm-failover-policy-matrix-build`
