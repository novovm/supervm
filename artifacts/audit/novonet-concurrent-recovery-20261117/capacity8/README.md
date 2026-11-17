# Main-node shared Carrier capacity candidate (2026-11-17)

## CURRENT20:25: local code passed; security/dependency decision blocks qualification

One exact-source Full44079683535 for e666a6fb has a failed root Audit job (libz-sys). The generated lock-only patch to1.1.33 passes local root/runtime Audit; runtime Deny also passes, but root Deny now exposes chrono unmaintained with no patched version. No ignore, rollout or second manual Full. [Actual outcomes and migration-scope decision](ci-first/README.md). Both original e666 packagers assembled outputs only, not qualified archives. The local-only next-step narrative below predates this newly observed security gate and does not authorize rerunning an already-known-red Full or deploying e666.

User approved **candidate validation with a maximum of eight shared Carrier connections**, reusing `ServiceConcurrencyV1`. This is not approval to install an unqualified package. Generic SDK compatibility default four, signed relay sessions four, shared relay 128KiB/minute, original authentication, timeouts, NativeObjectLink, AOEM/ledger and consensus remain unchanged.

## Actual unfinished recovery attempt, 19:05 Beijing

Fresh actual C/D route-bound observers from main@1a558102 passed the original readiness checks. The existing guarded coordinator resumed only the unfinished recovery object; it neither resent the completed pair nor stopped B again. A setup failed before object submission: `carrier stream IO failed: connection lost: closed by peer: scope finished (code 0)`.

A's original `relay_diagnostic_dial` at wall1794913519992 was followed by failure1794913520105. B's actual same-route circuit `803b7c8e254f77c24b6a716d4afde98d` authenticated at1989196025us, closed at1989196055us, had no downstream-ready timestamp and forwarded zero packets/bytes. Original B failure ordinal47, `inbound_session`: **`carrier connection limit: no permits available`**. The earlier ordinal1 cancellation is historical, not this failure. [Original error/carrier records, selected route projection and original hashes](failed-1905/).

The original driver retained three installed bound peer connections. Its shared default-four admission counted the new inbound relay as the fourth; `ingress.scope.connect(&next.route)` could not acquire a fifth downstream slot. This is a valid resource refusal, not evidence of a permit leak or AOEM/database/clock/authentication defect. Inbound handshake readiness is not downstream/object readiness. No new object was sent and B was not restarted in this attempt.

## Minimal implementation under validation

- Original main-node driver owns one existing `ServiceConcurrencyV1` and supplies it to the original Carrier scope. Existing maintenance refreshes host resource observations; no new timer, worker, socket, router or network service.
- Explicit local `IrohConfigV1` connection ceiling is part of retained endpoint-owner policy matching. Native main node ceiling eight; actual admission may shrink under resource pressure. Generic SDK without a controller remains four, including when an explicit ceiling is higher.
- Both inbound and outbound original connections use the same existing admission gate. Driver pending ingress follows that gate's effective target rather than a second hardcoded-four limit. Shrink does not revoke active leases; no ninth connection is admitted.
- Existing public observation adds ceiling/effective target/admission-refusal counters. No secrets, new authority or delivery claim.
- New native-reactor regression retains three actual bound peer routes at every node, then runs crossed two-relay endpoint/forwarder roles, checking both target integrity and sender DONE, unchanged business sessions, original signed four-session/128KiB budget and no failed relay writes. These are component crypto fixtures, not four-host ML-DSA acceptance.

## Gates and next action

Windows/Linux local gates have completed: original native-owner groups13/13 each, corrected Carrier groups72/72 each, resource-owner groups34/34 each, final corrected new-case1/1 each and strict Clippy `--lib --tests -D warnings` exit0 each. Format and whitespace checks pass. [Actual results, source/harness hashes and raw logs](local/result.json). No Full, fixed-package, deployment or real-host pass is inferred. Next freeze this source and run one exact-source Full, concurrently build the original Windows/Linux whole packages; only qualified new packages and subsequent deployment confirmation can advance real-host testing. Do not rearm/retry unchanged db667 to bypass this capacity gate. Existing observers may finish their bounded windows; preserve output, no new window now.

Both platforms' native-owner group passed13/13. The first separate Carrier group was71 passed/1 failed on each platform: the new CPU-pressure fixture assumed a target four at512MiB, but the original independent memory bound correctly restricted it to three. This was a test assumption failure, not a production resource-policy defect. Originals are retained in `local/*-first*`; only that `cfg(test)` sample changed to1GiB to isolate the CPU-pressure case. No host observation, pressure policy or production allowance was changed for this correction. The earlier failed groups are not relabeled green.

Corrected Carrier groups subsequently passed72/72 and original resource-owner groups34/34 on both platforms. First strict Clippy correctly rejected `needless_range_loop` in the new test's original-session comparison; both original outputs remain in `local/`. The comparison now enumerates the original session slice, with no lint suppression or runtime change. Final new-case rerun1/1 and strict Clippy passed on both platforms. Earlier13-case results remain separate, not silently rerun or overwritten as this final check.

Future deployment plan: qualified fixed whole-package B→A→C→D, existing original DB/custody/config, one host health gate before the next, preserve useful db667 rollback. No initialization/cache/key change and no config workaround on the old binary. Pin actual Node/Client/AOEM/manifest hashes and effective Carrier target before a new route-bound real-host window. Capacity admission/Carrier/bound authentication/target integrity/sender DONE remain separate. This eight-slot approval was for candidate validation; deployment confirmation is still required.

The completed crossed pair remains independently accepted as two real deliveries, **not** concurrent in-flight acceptance. Original automatic B restoration failed; delayed existing recovery is recorded separately. Recovered-object delivery and real in-flight concurrency remain unaccepted. No accepted direct/single/double baseline is repeated.
