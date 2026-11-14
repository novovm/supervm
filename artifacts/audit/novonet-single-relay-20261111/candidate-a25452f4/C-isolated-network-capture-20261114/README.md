# C independent isolated network / status capture — 2026-11-14

## Result and authority

**The original crash remains unresolved. No first corrupting instruction was found.**
C independently built and ran one bounded diagnostic experiment. A acknowledgment
is no longer a prerequisite; Git publishes evidence for coordination only.
No production fix, deployment, original-database access, original-identity use,
TTD recording, or double-relay restart occurred.

This is a new, instrumented a254-source execution with fresh test data. It is
neither the original a254 EXE nor a replay of PID37796's missing packet history.
It does not supersede the historical evidence limits recorded at `20e010df`.

## Actual execution

- Source: `a25452f4c49be37cd984e650f2079642e6c273b3`, exported under a new `target`
  directory. The main checkout's production crates were not edited.
- Four independent Windows processes, current user, loopback UDP only, fresh
  sealed test purpose keys and four new test databases. Test chain 51420261114.
- Existing `ResidentNode -> RpcLifecycle::poll -> native network` execution,
  original status generation/serialization, AOEM966 DLL. No external endpoints,
  original funds, real-user messages, HTTP listener or NOVONET relay were used.
- New binary SHA256:
  `271405511e72b78e4026f31506139818e4eba5317e69b3e5ac40df609d2caf73`.
- AOEM DLL SHA256:
  `5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`.
- Node run: 02:52:57–02:55:59 UTC (10:52:57–10:55:59 Beijing), 182.156 seconds
  including cleanup. Owner's intended duration was 180 seconds; supervisor's
  hard ceiling was 1,500 seconds, not a promise to run for 25 minutes.
- Target exit event: `Exit process 0:6114, code 0`; debugger exit 0; other three
  process exits 0; orchestration exit 0. No time/space limit termination.
- After completion the four recorded debugger/peer PIDs were absent. No node
  was subsequently restarted and no second workload attempt was made.

See [raw-derived public result](result-public.json) and
[build/EXE/PDB manifest](build-public.json). Failed compile attempts (101, 101)
and final successful build (0) remain recorded; these were harness compile
errors, not node crashes.

## What was captured

Before keys/databases were created, a gate-only process successfully wrote a
33,047,048-byte `/ma` dump. Header, stream bounds, Memory64 range bounds and
completion output were checked. This verifies normal gate dump creation, not
that every future failing process can successfully write a dump.

The new PDB GUID/age matches the new EXE; hash marker symbols resolved. Selected
standard-library function instruction patterns and unwind codes match original
a254 after relocation/displacement normalization; see
[function map](function-map-public.json). This is not whole-binary equivalence.

Private bounded traces preserve packet bytes in observed receive order and
limited object lifetime events. Published summaries expose only counts/hashes.
Across four processes, 520 unique object identifiers were observed on both a
send-construction and completed-receive path. Trace headers/sequences were intact;
observed live objects all had matching final Drop events. This is a new local
network execution, not four-host network acceptance or proof of business receipts.

The single actual status query produced 23,296 complete formatter/writer groups:
21,173 tail-jump and 2,123 padded-call groups, 139,776 boundary records total.
Receiver and text/length comparisons matched in these captured groups. No
exception/receiver guard fired and no failure dump was produced.

Recorded boundaries: formatter entry and invocation, `pad_integral` entry,
tail-jump or call preparation, writer entry, and copy call. Records associate
Windows thread, query/call sequence, RSP and relevant arguments. Trace thread
labels are process-local labels, **not** Windows thread IDs. The owner query
marker is the cross-log association; there is no asserted cross-process clock
or deterministic scheduling replay.

## New limiting evidence — do not repeat this run unchanged

The status query took **161.099 seconds** under software breakpoints. Only one
query completed; this did not exercise repeated periodic queries over hours.
The target's trace contains:

| Events | Before query | During query | After query |
| --- | ---: | ---: | ---: |
| Received UDP packets | 813 | 0 | 459 |
| Observed outgoing Arc creations | 55 | 0 | 55 |
| Final outgoing object Drops | 55 | 0 | 55 |
| Completed received objects | 35 | 0 | 42 |

The original owner executes polling and status sequentially. Other processes
continued running, but the breakpoint pauses prevented this target from polling
during its query. The captured event path itself used one logical thread per
process; this says nothing about all uninstrumented AOEM/background threads.

Consequently the run establishes capture wiring and actual local network input,
**not a faithful reproduction of the original timing or concurrent background
state**. More normal boundary records are not new root-cause evidence. Do not
extend this same breakpoint run or re-run ordinary snapshot examples as the next
step. Missing conditions remain: original packet/timing history, original
LocalService environment, NOVONET discovery/object-relay activity, multi-hour
residency and relevant unobserved background-thread interactions.

Before another workload run, the capture design must preserve repeated owner
queries without stopping at every formatted byte, and specify the actual missing
NOVONET workload to exercise with fresh identities. An isolated diagnostic-only
event buffer/targeted stop is a design option, not a delivered collector or
authorized change to production paths. We have not established deterministic
replay or captured a failure at those boundaries.

## Collection coverage and limits

- Object generation is assigned when an outgoing Arc is first observed here;
  it is not allocator instrumentation. Strong count is an instantaneous sample,
  not a complete clone/borrow/refcount history. Receive completion does not trace
  all subsequent frees. No complete memory lifetime claim is made.
- Receiver guards use the same invocation's formatter entry, not historical
  DMP registers. Guard mismatch would still need investigation: a guard trigger
  alone is not proof of the original crash or the first corrupting instruction.
- Boundary capture is scoped to the marked profile hash formatting path and
  owner thread, not every String, stack, callback or FFI path in the process.
- Trace file cap: 128 MiB per process. Boundary cap: 512 MiB. Space guards and
  retained process handles bound cleanup to this run; no deployed node is matched
  by executable name. No PageHeap or TTD configuration was made.
- Network crate retains `forbid(unsafe_code)`. Hooks and the new harness alter
  execution timing/layout; production sources and deployed binaries are unchanged.

## Private evidence and reproducibility

Local evidence root: `target/c-joint-network-capture-20261114/`. Private raw packet
traces, debugger output, gate dump, test keys/databases, generated EXE/PDB, and
build logs remain local. They are not Git artifacts. Public source hashes and
summary hashes permit local cross-checking without distributing their contents.

- [prepare.py](prepare.py) applies exact diagnostic hooks to a fresh historical
  source export; [patch hashes](source-patches-public.json) record changed paths.
- [diagnostic_capture.rs](diagnostic_capture.rs) defines bounded private records;
  [isolated harness](c-joint-network-capture.rs) reuses existing node ownership.
- [capture.py](capture.py) performs one guarded run; its existing start marker
  rejects a repeat invocation. **Do not run it on a deployment directory.**
- [summarize.py](summarize.py) analyzes the completed capture without launching
  nodes; [build_manifest.py](build_manifest.py) verifies new PE/PDB metadata.

C owns further diagnosis. A receipt is optional coordination and does not block
work. The original crash, first parameter corruption and repair remain open;
double-relay remains paused.
