# NOVOVM Phase 2-B Rollout Full Rust Shell Task List

Date: 2026-04-07

## 1. Goal

- Deliver full Rust shell takeover for the `rollout` command domain.
- Move `scripts/novovm-node-rollout.ps1` from "strict compatibility shell with status subset" to "pure compatibility shell".
- Seal `Phase 2-B` before opening `Phase 2-C daemon`.

## 2. In Scope

- `novovmctl rollout`
- `scripts/novovm-node-rollout.ps1`
- Mainline rollout actions:
  - `status`
  - `upgrade`
  - `rollback`
  - `set-policy`
- Mainline rollout orchestration path:
  - rollout plan hydration
  - enabled-group and controller filtering
  - transport selection summary for `local / ssh / winrm`
  - remote lifecycle dispatch chaining needed by rollout mainline

## 3. Out of Scope

- Validation line:
  - build validation
  - integration smoke
  - dry-run / non-dry-run comparison
  - compatibility-shell forwarding checks
- Historical/auxiliary shell line:
  - `gateway`
  - `reconcile`
  - `build orchestration`
- Physical retirement line:
  - final deletion timing for `novovm-node-rollout.ps1`

## 4. Package Rules

- Migrate the whole command domain, not one orchestration action at a time.
- Do not reintroduce policy logic into `novovmctl`; rollout policy decisions must continue to come from `novovm-rollout-policy`.
- Do not silently fall back to old PowerShell rollout orchestration.
- Unsupported behavior must fail explicitly until the Rust path exists.
- When the package is complete, compress `novovm-node-rollout.ps1` to forwarding only.

## 5. Work Packages

## 5.1 CLI parity

- Map mainline rollout parameters from the existing shell surface into `novovmctl rollout`.
- Keep argument names stable where reasonable.
- Reject legacy-only parameters that are outside rollout mainline scope.

## 5.2 Plan and state takeover

- Preserve `status`
- Implement plan hydration and normalized plan-state rendering in Rust shell flow
- Keep enabled-group, controller, and transport summaries in Rust output/audit

## 5.3 Rollout action takeover

- Implement `upgrade`
- Implement `rollback`
- Implement `set-policy`
- Keep the first implementation focused on mainline rollout behavior, not gateway/reconcile/build expansion

## 5.4 Remote orchestration takeover

- Move the mainline rollout dispatch chain into Rust shell flow
- Reuse lifecycle actions through `novovmctl lifecycle` where possible instead of duplicating lifecycle logic
- Keep transport orchestration limited to current mainline `local / ssh / winrm` behavior

## 5.5 Output and audit

- Reuse `novovmctl` terminal summary format.
- Reuse unified JSON envelope:
  - `ok`
  - `command`
  - `timestamp_unix_ms`
  - `host`
  - `data|error`
- Reuse JSONL audit append path.

## 5.6 Compatibility shell compression

- After Rust takeover lands, reduce `scripts/novovm-node-rollout.ps1` to:
  - parameter forwarding
  - `novovmctl` discovery
  - exit-code passthrough

## 6. Completion Criteria

- `novovmctl rollout` owns all in-scope rollout actions.
- `novovm-node-rollout.ps1` contains no remaining rollout business logic.
- No mainline rollout action silently falls back to old PowerShell orchestration.
- The command domain can be marked sealed and removed from active phase scope.

## 7. After Phase 2-B

- Open `Phase 2-C daemon` as the next package.
- Keep validation and physical retirement on separate tracks.

## Phase 2-B rollout validation closure note (2026-04-07)

- `novovmctl rollout` now compiles and the compat shell `scripts/novovm-node-rollout.ps1` forwards into Rust without falling back to legacy PowerShell rollout orchestration.
- `status` and `set-policy` passed on the full validation plan fixture.
- `upgrade --dry-run` and `rollback --dry-run` passed on the canary-only rollout validation fixture after batch seeding release/state prerequisites.
- JSON terminal envelope and JSONL audit remained unified across `rollout -> lifecycle`.
- `dry-run` left the lifecycle state hash unchanged for both `upgrade` and `rollback` validation runs.
- The remaining full multi-group gate/state interaction was classified as fixture complexity, not a Rust shell linkage defect.
- Phase 2-B is therefore considered closed for minimum mainline Rust-shell validation.
