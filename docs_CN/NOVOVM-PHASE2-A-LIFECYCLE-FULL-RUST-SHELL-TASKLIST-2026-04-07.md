# NOVOVM Phase 2-A Lifecycle Full Rust Shell Task List

Date: 2026-04-07

## 1. Goal

- Deliver full Rust shell takeover for the `lifecycle` command domain.
- Move `scripts/novovm-node-lifecycle.ps1` from "strict compatibility shell with usable subset" to "pure compatibility shell".
- Seal `Phase 2-A` before opening `Phase 2-B rollout`.

## 2. In Scope

- `novovmctl lifecycle`
- `scripts/novovm-node-lifecycle.ps1`
- Mainline lifecycle actions:
  - `status`
  - `set-runtime`
  - `set-policy`
  - `start`
  - `stop`
  - `register`
  - `upgrade`
  - `rollback`

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
  - final deletion timing for `novovm-node-lifecycle.ps1`

## 4. Package Rules

- Migrate the whole command domain, not one action at a time.
- Do not reintroduce policy logic into `novovmctl`.
- Do not silently fall back to old PowerShell lifecycle behavior.
- Unsupported behavior must fail explicitly until the Rust path exists.
- When the package is complete, compress `novovm-node-lifecycle.ps1` to forwarding only.

## 5. Work Packages

## 5.1 CLI parity

- Map mainline lifecycle parameters from the existing shell surface into `novovmctl lifecycle`.
- Keep argument names stable where reasonable.
- Reject legacy-only parameters that are outside lifecycle mainline scope.

## 5.2 Process and state takeover

- Implement `start`
- Implement `stop`
- Preserve `status`
- Reuse existing `up`/node-launch helpers where possible instead of duplicating launch logic.

## 5.3 Governance and registration takeover

- Preserve `set-runtime`
- Preserve `set-policy`
- Implement `register`
- Keep governance/node-group/upgrade-window state changes in Rust shell flow.

## 5.4 Lifecycle action takeover

- Implement `upgrade`
- Implement `rollback`
- Keep the first implementation focused on mainline lifecycle behavior, not remote orchestration expansion.

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

- After Rust takeover lands, reduce `scripts/novovm-node-lifecycle.ps1` to:
  - parameter forwarding
  - `novovmctl` discovery
  - exit-code passthrough

## 6. Completion Criteria

- `novovmctl lifecycle` owns all in-scope lifecycle actions.
- `novovm-node-lifecycle.ps1` contains no remaining lifecycle business logic.
- No mainline lifecycle action silently falls back to old PowerShell logic.
- The command domain can be marked sealed and removed from active phase scope.

## 7. After Phase 2-A

- Open `Phase 2-B rollout` as the next package.
- Keep validation and physical retirement on separate tracks.

## 8. Validation Closure Note

- Minimum validation loop is complete for the mainline lifecycle path:
  - `register`
  - `start`
  - `status`
  - `stop`
- Compatibility shell validation is complete for `scripts/novovm-node-lifecycle.ps1` on the current mainline bridge.
- The former ingress blocker `NOVOVM_OPS_WIRE_DIR has no .opsw1 files` has been resolved by pre-spawn managed-ingress bootstrap seeding.
- Clean stop behavior is confirmed:
  - process termination succeeds
  - `pid_file` is removed
  - follow-up `status` is not misled by stale pid state
- This note closes the minimum validation track only; production-scale rollout, extended smoke, and physical shell retirement remain separate lines.
