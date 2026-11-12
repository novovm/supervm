# C crash follow-up: FFI ownership correction, cause still open

Status: **C node NOT READY; no corrected runtime deployed; no double-relay send.**
This follow-up distinguishes an independently confirmed AOEM FFI allocation
contract defect from the still-unexplained C node crash. It does not change the
existing C node, its database, identity, keys, or observer.
A read-only check on 2026-11-13 found no local `novovm_node.exe` process or C
UDP listeners on 41052/42052; no recovery was attempted.

## Original failure and bounded reproduction

The existing a254 PID27928 dump records an access violation during a status
snapshot/string append. The saved formatter points to a plausible empty String,
but the failing writer consumed a different stack location and attempted a
two-byte copy to address `0x18`. The exact earlier corrupting instruction and
the relationship between the 875 and a254 crashes remain unknown. Original
dumps and private register logs remain only on C.

C also ran one isolated owner-lifecycle test against the **original packaged**
AOEM DLL SHA256
`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`.
It used the original profile snapshot implementation, the existing SUPERVM
ML-DSA semantic verifier owner, 32 private context lifecycles and shared-owner
calls. It completed 1,024 valid and 1,024 modified-message negative verifier
calls, 80,000 profile event attempts, 501 reader snapshots, 512 concurrent
crypto snapshots and 805,376 checked hash fields. Exit 0, test body 2,439 ms;
the full node, persistent database and network were not used. This did not
reproduce or clear the C crash.

## Confirmed source-level defect and correction

The AOEM FFI export `aoem_free(ptr, len)` reconstructs a `Vec` with capacity
equal to `len`. Five producer paths instead returned a raw pointer from a
`Vec` without preserving its actual capacity: delegated output, graph/state
JSON response, single execution, batch execution and primitive output. If
capacity differs from length, the allocator receives an incorrect layout on
free. This is a source-level undefined-behavior defect regardless of whether
it explains C's particular crash.

AOEM private main commit `fd5cfb31` converts those buffers to exact-sized
boxed slices before transferring ownership. It changes no exported signature,
service policy or production timeout. Focused verification: `cargo check -p
aoem-ffi --no-default-features` passed; the state JSON round-trip, primitive
rejection and new varying-size graph-error response/free tests each passed.
The workspace-wide formatter check reports existing unrelated formatting
differences; `git diff --check` passed. This source commit has **not** replaced
the original DLL loaded by C and is **not** a deployment qualification.

SUPERVM dynamically calls and frees AOEM-owned outputs through
`crates/aoem-bindings/src/lib.rs`. Some of these exports are available to the
host, but the currently observed status-snapshot crash stack contains no AOEM
frame. The exact route by which this contract defect could affect that crash
has not been established. Do not report `fd5cfb31` as the crash root cause.

## Next acceptance boundary

1. Produce a fixed AOEM Windows candidate and record its source, binary hash
   and ABI contract. Do not silently substitute it into the running C node.
2. Exercise the original SUPERVM owner against that candidate with allocation
   checking and compare to the pinned old DLL. Retain any new failure.
3. Only after candidate qualification, perform one controlled C `existing`
   recovery and observe the real status/transport path long enough to cover
   the former failure window. Preserve old dumps and failed runs.
4. Require fresh C node health and observer READY before the scheduled
   A→B→C→D object test. Sender DONE alone is not C forwarding or D receipt.

C owns these checks. A receives the public handoff and coordinates a later
network send; this report does not ask A to diagnose the C crash.
