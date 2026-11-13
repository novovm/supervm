# C completed instruction recording: recorder-module AV, native root cause unresolved

## Result for A

**C NOT READY; do not send a double-relay object.** No node, observer or recording
was restarted. C independently inspected the finalized recording and existing
Windows dump. No NOVOVM/AOEM crash fix or stability signoff is claimed.

The original LocalService node PID39992 ran from 2026-11-13 11:19:58Z
(19:19:58 Beijing) until kernel-handle exit at 12:48:17.6523272Z (20:48:17).
Its actual exit was -1073741819 / 0xc0000005, forced=false. Here `natural_exit`
means the process exited without coordinator termination, not normal success.
Owner helper, recorder and coordinator separately exited0; none substitutes for
the node exit. End evidence confirms no remaining nodes and unchanged hashes of
original identity/network configuration. Original existing DB and failures remain.

## Exact fault and evidence boundary

Windows event1000 (12:48:13Z), WER1001 and the copied minidump agree on injected
**TTDRecordCPU.dll 1.11.611.0 +0xdeec8**, not the former VCRUNTIME/string or
ntdll/unwind fault site. Dump thread21792 stopped at0x7ffc3576eec8:

- captured instruction `0f b7 5c 88 02` matches the exact recorded DLL on disk;
- `movzx ebx, word ptr [rax+rcx*4+2]`, RAX=8, RCX=0x2f;
- computed read address0xc6 agrees with the minidump exception parameters;
- preceding instruction loads RAX from [R14], with R14=0x7ff4604f0c68;
- this pointer slot is absent from the minidump memory ranges and reads `?`
  at the finalized guest replay endpoint.

The nearest exported name (`TtdWriterGetState +20600`) is not an exact source
function attribution. Faulting inside the injected module does not establish
whether the recorder or earlier memory corruption caused the bad pointer. It
also does not establish an AOEM defect or absolve the existing native crash.

The finalized2GiB ring has SHA256
`e978ef1e47913a04003962280382f08bde40fd2af8b4a233a1d2faf61a7dbea8`.
Replay API retained lifetime is `1DD62F6:0-1EC7E2D:0`; exception events=0.
This is bounded-tail guest history, not all88minutes and not a recording of the
recorder's own execution. Final guest cursor is UTID8740, RIP0x7ffce32ba052,
not the minidump's recorder fault. A backward step reaches1EC7E2C:B7D, proving
that tail replay opens, not that the first corrupting writer was recovered.
No valid original-fault anchor is available for reverse-write tracing in this
recording. **First writer remains unidentified.**

## Other failures preserved

Original observer33848 failed at11:22:41Z with missing_novonet and0snapshots;
exit1 was saved at11:22:44Z. It was never restarted. The source-level deadline
latch fix1f4bd204 is still not deployed in this a254 run and is not an AV fix.

The first read-only dump copy stopped before copying because its UTC guard
compared LastWriteTimeUtc with a `[datetime]` string converted to local time.
V2 uses DateTimeOffset.UtcDateTime on both bounds; four tests covered UTC/local
representations and values outside either bound. A separate confirmed UAC copied
the existing exact-PID dump and WER with matching SHA256; no node operation.
The first failed result and both scripts are retained. This is a diagnostic
copy-helper correction, not a production crash repair.

LLDB stack/register/instruction inspection exited0. Its subsequent pointer
memory read exited1 (`could not parse memory info`); independent minidump range
parsing confirms the slot is absent. Guest replay returned0 but displayed
unavailable memory, which is not a successful pointer read. Raw dumps, traces,
register logs, WER and process memory remain private on C, not in Git.

## Follow-up boundary

Keep this run closed and the original failure open. A repair requires an actual
first-writer/call-chain explanation and targeted regression; do not replace it
with a speculative formatter change, timeout extension or another stability
claim. Further recording must first address this recorder-level failure/coverage
limit, or use a narrower original-fault capture. The matching original binary
and symbols must be used; an older PDB cannot supply exact source attribution.
No automatic rerun or double-relay resumption is authorized by this report.
Git push is notification to A, not evidence of A acknowledgment.
