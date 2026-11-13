# C original node: bounded instruction recording start (historical)

Final outcome: [recording ended with injected recorder AV; native root cause remains open](../ttd-node-ended/README.md). Do not treat this historical start as current readiness.

Actual start: 2026-11-13 11:19:58Z (19:19:58 Beijing). Node PID39992,
LocalService SID S-1-5-19, a254 executable and AOEM966 hashes match the pinned
preflight. The suspended child was identified through kernel image path,
creation time and owner token; the administrator TTD recorder confirmed attach
before LocalService resumed its original main thread. Existing-only config,
original DB/identity and prior progress/log evidence are preserved.

This is one node diagnostic, not a new candidate deployment or relay readiness.
No object is sent and A must not send. Raw instruction traces contain process
memory and stay local, as do node stdout/stderr, original config and progress.

## Bounds and actual observer failure

Node process lifetime is 7200 seconds; owner hard bound7320seconds; task130min.
The 2048MiB ring file is preallocated and retains only a bounded tail. Its file
size is not proof of how much history has already been overwritten. Max private
memory6GiB, free-disk reserve12GiB; cleanup verifies exact owned process/handles.
Stop requests/target exits are kept distinct from recorder/coordinator exits.

Original Watch PID33848 started11:21:20Z. It produced an original ready file,
then failed11:22:41Z with missing_novonet, zero relay snapshots. Its ready file
is preserved as **diagnostic observer evidence only**, not permission for A to
send. The original node recording continues independently, with no restarted
observer. The publisher deadline-latch source fix from1f4bd204 is not deployed
in this a254 run; this observation failure is separate from the native AV.

## Replay preparation

A one-line extension of the official Microsoft TraceInformation sample prints
ExceptionEvent.Position (the original sample prints no coordinates). Its source
retains MIT attribution. Build used/W4/WX, exit0. A control trace's exception at
47:0 was replayed through the unchanged official TraceDebugger; the RIP matched
the exception PC. Control hashes/results are included. This does not yet read
an unfinished node trace or identify the node's first corrupting write.

## Still open

No node exit/fault outcome at this snapshot; no crash fix is claimed. At completion,
keep the actual kernel-handle node exit, recorder/coordinator exits, original
observer result, final trace hash and retained lifetime. If an AV is present,
replay its guest registers/instructions and reverse watch the relevant pointer
writes. If it is absent or outside the retained range, report unresolved; do not
turn a negative bounded run into a stability signoff or automatically restart.
Double-relay remains blocked. Git notification is not A acknowledgment.

## Finalized-trace reader

Inspect-FinishedRecording.py refuses a live or unfinalized ring: it requires the
specific node kernel-handle exit, recorder normal exit, no remaining node and
unchanged original identity/configuration. It validates pinned offline readers,
trace PID, retained lifetime and exception count; selected fault PCs must agree
with replayed guest RIP. Exception operands alone are not treated as proof.
Any write origin still requires further reverse-watch analysis; the reader never
marks a root cause fixed, starts a process under test, or retries a node.
The source uses fixed local paths for this one captured process, not a generic
run command. Microsoft sample terms are in LICENSE-Microsoft.txt.
