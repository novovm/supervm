# NOVOVM Rust migration final closure summary (2026-04-07)

## Final conclusion

NOVOVM has completed two mainline Rust migration stages and the mainline is now considered closed for this program.

Phase 1 closed:
- unified Rust policy core closed
- `novovm-rollout-policy` is the sole normal-path policy engine for `overlay + failover + risk + rollout`

Phase 2 closed:
- mainline Rust shell migration closed
- `novovmctl` is the cross-platform operational shell
- `up` and `rollout-control` are mainline Rust-shell paths
- `lifecycle`, `rollout`, and `daemon` are closed under Phase 2-A / 2-B / 2-C minimum mainline validation

The mainline architecture is now fixed as:

```text
novovmctl              -> Rust operational / entry shell
novovm-rollout-policy  -> Rust policy core
novovm-node            -> node runtime executable
```

PowerShell is no longer a mainline decision layer. Mainline `.ps1` files are compatibility shells only.

## Current architecture and status

| Layer | Main executable | Current status | Notes |
| --- | --- | --- | --- |
| Operational shell | `novovmctl` | Closed | Cross-platform mainline shell |
| Policy core | `novovm-rollout-policy` | Closed | Normal-path policy engine |
| Node runtime | `novovm-node` | Active | Runtime executable, not shell logic |
| Compatibility entry shells | `scripts/*.ps1` mainline wrappers | Reduced | Compatibility forwarding only |

## Mainline status by command domain

| Domain | Current status | Validation level |
| --- | --- | --- |
| `up` | Mainline Rust shell takeover closed | Mainline path established |
| `rollout-control` | Mainline Rust shell takeover closed | Mainline path established |
| `lifecycle` | Phase 2-A closed | Minimum mainline validation loop closed |
| `rollout` | Phase 2-B closed | Minimum mainline validation loop closed |
| `daemon` | Phase 2-C closed | Minimum mainline validation loop closed |

## What is inside the completed scope

Completed scope includes:
- unified Rust policy core
- unified Rust output and JSON envelope
- unified JSONL audit path
- compatibility shell pure forwarding
- minimum dry-run safety validation for mainline shell domains
- Phase 2-A / 2-B / 2-C mainline shell closure

## What is outside the completed scope

The following items are explicitly outside the completed scope of Phase 1 and Phase 2:
- Phase 3 candidate capability work
- stage-out deeper validation line
- real non-dry-run remote rollout verification across `ssh / winrm`
- deeper gateway / reconcile / build orchestration work
- historical and migration helper scripts
- physical retirement timing of all compatibility shells

These are follow-on workstreams, not blockers for the completed Phase 1 / Phase 2 closure.

## Compatibility shell status

Mainline compatibility shells remain in place only for compatibility and forwarding:
- `scripts/novovm-up.ps1`
- `scripts/novovm-node-rollout-control.ps1`
- `scripts/novovm-node-lifecycle.ps1`
- `scripts/novovm-node-rollout.ps1`
- `scripts/novovm-prod-daemon.ps1`

Their role is limited to:
- parameter bridge
- binary discovery
- Rust CLI forwarding
- exit-code passthrough

They are not part of the mainline decision core anymore.

## Recommended next-step options

The current program should not reopen Phase 1 or Phase 2.

If work continues, it should go into one of two tracks only:
- validation track: deeper real-environment verification
- capability track: explicit Phase 3 candidates such as gateway / reconcile / build orchestration

## Reference status line

Formal reference line for project status:

> Phase 1 unified Rust policy core is closed. Phase 2 mainline Rust shell migration is closed. The NOVOVM mainline now runs on a fixed three-layer Rust architecture of `novovmctl -> novovm-rollout-policy -> novovm-node`, with PowerShell reduced to compatibility shells only.
