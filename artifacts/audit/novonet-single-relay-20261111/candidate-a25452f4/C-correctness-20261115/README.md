# C: A 24-hour receipt checked; portable correctness tests

2026-11-15. Main fast-forwarded to1aa5fb56, including A's f3ce1f90 receipt.
No node restart, BIOS/driver/configuration change, original DB or identity access,
product modification or double relay. Tools/data are isolated under ignored target.

## A evidence now available

A ran Nov12 16:13:18 through Nov13 16:13:19.975 Beijing, about86401.975 seconds,
then exited0 under its86400000ms cap. It is currently stopped (process_count0),
not an ongoing 24-hour run. Its retained Application query found no NOVOVM match;
WHEA query since Nov12 found0. This does not certify all hardware/software.

A's current on-disk EXE hash matches original a254 and earlier loaded-image
receipt. AOEM is1b764a36...acdce50, consistent with its Nov12 deployment evidence.
C PID27928 originally loaded exactly the SAME EXE/AOEM hashes and subsequently
crashed (C-source-investigation-20261112). Thus the later AOEM966 mismatch cannot
explain every C failure. C's later PID37796/34452 used5743f587...a01c0d52, not A's
runtime. Full hashes and comparison are in A-C-comparison-public.json.

A final progress: ticks233204557, executed_batches0, durable_decisions0,
execution_failures0, rejected_messages3671, expired_sends5047. The observed backend
is aoem_semantic_v2_cpu_callbacks, not evidence of Vulkan use. C PID34452 final
archived ticks2847944/send_polls.active_calls423210/reported_data_writes17192 use
different counter fields/exposure. Exact status invocation counts and matching
packet history remain unavailable. No matched workload, asset-load or same-heap
comparison is claimed. Current disk bytes do not prove every historical load.

A hardware/OS: Ultra9 275HX,24 logical processors,64GiB Micron6400,Windows26200,
Q7CN37WW. C:14900HX,32 logical processors,32GiB5600,Windows26300,N0CN22WW.
These are variables, not causes. Mixed DIMM vendors are not a diagnosis.

## Correctness approach

Official portable Prime95 30.19b20 Windows64 ZIP is SHA256-verified against
https://www.mersenne.org/download/ . Package contains user-mode EXE/DLLs and
text documentation, no driver package. No installer, service registration,
startup task, administrator elevation or networking assignment was used.
UsePrimenet=0 and StressTester=1; only documented -t torture mode is invoked.
The bundled stress.txt describes comparison against known correct results.
This differs from the previous WinSAT bandwidth measurement.

One CPU-focused16–64K run bounded120s, then one memory-focused1024–4096K run
bounded180s, requested4 cores, no hyperthreading, normal low test priority.
Memory run requests1024MiB of test memory; controller private-bytes cap3GiB.
No all-32GiB physical address coverage is claimed. WHEA/new self-test error
stops the sequence; deadline requests normal close, with10s exit fallback.
Actual natural/program-requested/forced exit must be distinguished in result.
No affinity change or APIC33 targeting was made; no mapping to Windows CPU33.
No new temperature driver was installed; package-temperature telemetry is not
available in this run, so no thermal qualification is asserted.

Raw third-party package, configuration (including generated ComputerGUID), and
wrapper remain local. Public output is limited to self-test logs, hashes,
coverage, exit and WHEA projections. No product dependency or default CI was added.
A source-archive download returned a2469-byte non-ZIP response and failed its
published hash; it was not built or executed. The separately downloaded executable
ZIP passed the published SHA256; source-archive failure is not a binary mismatch.

## Acceptance boundary

These are bounded screening runs, not proof of hardware health or of the original
NOVOVM cause. A pass only covers completed self-tests; interrupted cases remain
incomplete. New WHEA/self-test errors would be direct evidence requiring follow-up,
but would not alone identify CPU/RAM/firmware as the specific faulty component.
The existing14 WHEA reports and prior failure dumps remain preserved. Do not resume
double relay or submit a speculative product fix from either a pass or a failure.

## Execution notes

CPU phase reached its120-second bound. CloseMainWindow returned false for the
hidden GUI, so after10 seconds the controller terminated only that test PID.
Actual exit-1 is controller-forced, NOT a natural crash or a normal exit. Eight
completed self-tests (four16K, four18K) are retained; later in-flight work is not
counted as passed. CPU test was not rerun.

Only the not-yet-run memory phase was then launched once. Its bounded stop uses
Windows taskkill without /F first and records command/target exits separately;
if needed, the same exact-process forced fallback remains explicit. This is a
controller lifecycle adjustment, not a diagnostic engine or product change.
The four-core test was not pinned to APIC33; no claim is made that the CPU that
reported WHEA was individually exercised. No hardware defect is excluded.


## Actual terminal results (Beijing)

| Phase | Window | Completed known-result self-tests | Actual exit | Stop |
|---|---|---|---|---|
| CPU PID26828 |10:50:10–10:52:23|8: four16K, four18K|-1|120s bound, hidden normal-close unavailable, forced after10s|
| Memory PID22084 |10:52:51–10:55:55|12: four1600K, four1024K, four1680K|0|180s bound, taskkill WITHOUT /F sent normal close, no force needed|

Memory peak private bytes1092816896; not proof all those bytes were independently
verified. No logged self-test error and no new WHEA through postcheck. Retained
WHEA count14/latest110869 predates these tests. Query NoMatchingEventsFound is
explicitly distinguished from access failure. Final process count: Prime95=0,
NOVOVM original/BASE=0. No automatic retry or node recovery occurred.

The CPU forced stop makes its final in-flight calculation incomplete. The memory
exit0 follows controller-requested closure, not natural completion of all possible
torture cases. Logs and controller result are separate; neither a zero nor -1
controller-related exit substitutes for the completed self-test list. Short exposure,
limited memory footprint, unverified APIC coverage, ambient workload and absence
of package temperature telemetry prevent any hardware-clearance claim.

Next useful action is the already-scoped isolated NOVOVM BASE with its own keys
and full dumps, or a separately bounded broader correctness run if needed; no
need to repeat WinSAT or these exact short cases. Preserve the 14 corrected-error
reports and compare any new fault/event timestamps. Root cause remains unresolved.
