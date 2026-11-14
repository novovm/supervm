# Isolated Windows x64 RBP call capture

Standalone qualification tool, **not a NOVOVM fix, injection tool, or production wrapper**.
Builds with the installed MSVC x64 toolchain. Never opens the node database, identity,
existing dumps, services, registry, network endpoints or a running process.

`CallCapture` stores actual RBP/RSP around one indirect call, without debugger stops,
per-event clocks, formatting, or allocation. The caller reserves one 192-byte record
from a preallocated per-thread array. Thread creation time, process creation time,
sequence, parent sequence, depth and stack limits identify the invocation. Full
capacity refuses the target call and increments `rejected`; it does not silently
overwrite history or execute an unrecorded call. An outer loop is responsible for
stopping on mismatch. The qualification harness deliberately permits exactly one
known bad fixture so it can finish checking the recorder.

The assembly wrapper has Windows unwind metadata for `push rbp` and its 64-byte
allocation. It reserves shadow space, preserves its caller's RBP, and captures the
callee's returned RBP before restoring the caller. **That restoration is containment
for an isolated test, not an approved way to hide a node's ABI violation.**

Only four integer/pointer arguments and an integer/pointer RAX return are supported.
Not suitable for arbitrary signatures, varargs, floating-point arguments, or Rust
ABI calls. It checks RBP, not all ABI state. An extra frame and per-call record
writes alter execution; no low-interference qualification is implied.

## Three separately runnable modes

Run in PowerShell 7 with **new** evidence directories. No elevation required.

```powershell
& scripts/diagnostics/rbp-call-capture/Run-Qualification.ps1 -EvidenceRoot D:\WEB3_AI\SUPERVM\target\rbp-lifecycle-new -Mode Lifecycle
& scripts/diagnostics/rbp-call-capture/Run-Qualification.ps1 -EvidenceRoot D:\WEB3_AI\SUPERVM\target\rbp-single-new -Mode Single
& scripts/diagnostics/rbp-call-capture/Run-Qualification.ps1 -EvidenceRoot D:\WEB3_AI\SUPERVM\target\rbp-concurrent-new -Mode Concurrent
python scripts/diagnostics/rbp-call-capture/Analyze.py D:\WEB3_AI\SUPERVM\target\rbp-lifecycle-new D:\WEB3_AI\SUPERVM\target\rbp-single-new D:\WEB3_AI\SUPERVM\target\rbp-concurrent-new D:\WEB3_AI\SUPERVM\target\rbp-analysis-new.json
```

Each process is bounded to 60 seconds; timeout terminates only the newly launched
qualification process. Outputs are not replaced. Build sources, executable, PDB,
SHA256 manifest, stdout/stderr and process exit are local. Typical allocated record
arrays: 1.5 MiB in single-thread mode, 3 MiB total in concurrent mode. These are
explicit recorder capacities, not measured total process memory or peak RSS.

- **Lifecycle**: legitimate temporary RBP use, deliberate ABI violation, marker SEH
  exception, subsequent call, exhausted capacity, four threads and nested invocations.
  The faulty fixture's `xor rbp,0x4000` is known synthetic test code, not an observed
  NOVOVM instruction. Independent Python analysis checks the instruction in its
  actual PE and rejects mixed threads, reused sequence, altered RSP, or an unwind
  mislabeled as a normal return.
- **Single**: direct versus captured calls to the real loaded `ntdll!RtlReAllocateHeap`
  on a new private heap. 60 batches per mode, 4096 calls per batch, AB/BA ordering.
- **Concurrent**: four persistent threads share a new private heap; 120 batches per
  mode, 16384 calls per batch. Record reset and disk export are outside timed regions.
  No deterministic replay of production scheduling is claimed.

`state=1` is in flight, `2` is a normal return, `3` is the caught test exception.
The SEH filter records the exception context separately and marks `context_gap=1`.
It does not synthesize a post-call RBP for an unwound activation. Arbitrary
`NtContinue`, longjmp, fiber switches or context restoration are **not covered**.
Only the intentional marker exception is handled; other exceptions propagate.

## Evidence limits

Call before/after values localize a returning violation to **a call interval**.
Only the controlled straight-line fixture supplies a directly verified first
modifying instruction. This wrapper cannot automatically find the first write
inside an arbitrary callee. Destruction of RSP or its own saved pointer may prevent
post-call recording. The original heap-cookie fast-fail can occur before return;
this limitation is especially relevant to PID34452. No original EXE is patched.

The original failure has not been reproduced. Do not deploy this around the
production heap or resume double relay based on these tests. The recorded batch
average percentiles are not per-call latency percentiles. The process CPU counter
has coarse resolution, particularly in the single-thread experiment.

Microsoft references: [x64 calling convention](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention),
[frame-based exception handling](https://learn.microsoft.com/en-us/windows/win32/debug/frame-based-exception-handling).
