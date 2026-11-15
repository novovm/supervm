# A/C binary, load and WHEA correlation — 2026-11-15

C independently completed read-only correlation and one bounded Windows memory
bandwidth comparison. Original DB/identity/failure evidence unchanged; no node
started, no BIOS/driver/VBS changes, no double relay. No root-cause fix claimed.

## Binary and load comparison

- A's available receipt A-update-public.json is dated Nov12 08:13:20Z, PID5136.
  EXE SHA256 1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d
  matches C's a254 failure-run EXE and C's currently rehashed isolated copy.
- That A receipt's loaded AOEM hash is
  1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50.
  C's later failure run and rehashed AOEM966 file use
  5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52.
  These are different. This does NOT assert A still loads the old DLL today.
- A's Nov12 authenticated status is a point observation (round470, height0,
  current_height1, elapsed32ms), not 24-hour workload evidence. A's current
  24-hour no-symptom observation is user-reported; user is arranging a Git receipt.
- C PID34452 final archived counters: ticks2847944, retained transport records76,
  expired2643, evicted0, send_polls.active_calls423210, reported_data_writes17192,
  accepted_data1708, history_data15497. These count different operations, not
  actual object allocation/free or exact status function calls.
- A's corresponding first/last counters, configuration, actual loaded modules,
  uptime and query activity are missing. Equal-load/equal-runtime control is NOT
  established. Do not wait for A to continue C's independent analysis.

## Retained WHEA history

Read all retained System WHEA-Logger events: 14 event19 records from May8 to Nov13,
all report processor-core Internal parity error and APIC33, MCA bank0. Status
has two values: 10 records 0x8000004000050005, 4 records 0x8000004000040005.
No speculative bit-level root-cause interpretation is made. Public fields omit
raw hardware payload and machine identity. Current System log retention begins
Apr19; this is retained history, not proof of complete lifetime coverage.

12 of the14 records precede the November investigation. Therefore the reported
error class cannot be explained entirely by this TTD experiment. Corrected errors
are evidence of a platform-level report, not proof that CPU silicon, RAM brand,
firmware or NOVOVM caused the RBP corruption. No direct link to mixed DIMMs found.

## Time relation (Beijing)

| Event | Time | Relation to WHEA |
|---|---|---|
| Original875 PID31296 dump | Nov11 21:27:58 | No event within +/-5min |
| Original875 PID3228 dump | Nov12 04:13:40 | No event within +/-5min |
| Original a254/AOEM966 PID37796 owner exit | Nov13 10:15:56 | First Nov13 WHEA occurs9h49m22.847s later |
| WHEA110868 | Nov13 20:05:18.847 | Inside PID39992 TTD run |
| WHEA110869 | Nov13 20:23:01.167 | Inside PID39992 TTD run |
| TTD PID39992 owner exit | Nov13 20:48:17.652 | WHEA precede exit by42m58.805s and25m16.486s |
| Original a254/AOEM966 PID34452 owner exit | Nov14 17:13:47.443 | Last WHEA precedes by20h50m46.277s; none during16:07:24–17:13:47 run |

TTD Application Error1000 time is20:48:13.181, several seconds before owner exit;
keep fault/notification/exit timestamps distinct. Historical dump times are not
assumed identical to instruction fault times. The +/-5min window is a stated
comparison window, not a diagnostic threshold. No contemporaneous WHEA does not
exclude silent corruption or missing hardware reporting. APIC33 is not Windows
CPU index33 and is not identified with earlier PT CPU9.

Sources: C-original-dmp-review-20261113/dump-inventory-public.json;
C-source-investigation-20261112/C-AOEM966-controlled-20261113/backtrace-exit-37796/README.md;
C-source-investigation-20261112/C-first-fault-20261113/relay-v4/ttd-node-ended/event-reference-public.json;
C-wer-natural-20261114/node-exit.json; C-sustained-rbp-20261115/README.md.
Machine-readable time differences: timeline-public.json.

## Actual isolated hardware-load step

After Windows elevation, stock WinSAT ran on C, UTC02:29:21–02:29:44 Nov15:
`winsat mem -rn -mint 10 -maxt 10 -buffersize 16MB -xml <new-local-file>`;
single-thread run adds `-up`. One run each, normal priority; no formal assessment,
no disk/GPU test, no node, no extra driver or recorder. Timeout guard60s per child
was not hit. Both actual child exit codes0; XML CompletionStatus0. XML reports
1 versus24 test threads (32 logical processors), not assumed32 worker threads.
No new WHEA in-run or in the subsequent postcheck; see bounded-load-public.json.
Raw XML/stdout/stderr and wrapper remain isolated under target, not release code.
Raw XML contains machine inventory and is not uploaded; public metric projection
and hashes allow local reconciliation.

This is a bandwidth/load observation, NOT a RAM correctness test, long-duration
CPU stability pass, original failure reproduction, or cross-machine A/C test.
Microsoft defines this test as memory-copy bandwidth:
https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/winsat-mem

## Remaining hardware control

Next meaningful control requires an established CPU/memory correctness diagnostic
with exported results, bounded duration and before/after WHEA. Do not replace it
with another home-grown register collector or claim WinSAT proves sound hardware.
Lenovo's official Windows diagnostic entry was located:
https://support.lenovo.com/us/en/solutions/ht506581
No tool was installed or launched: driver/installation behavior must first be
verified against the user's no-driver-change boundary. Reboot-based memory tests,
DIMM removal, BIOS defaults, undervolting changes and VBS changes were not executed.
Record ambient load, test duration/coverage, errors and new WHEA; any non-reproduction
remains bounded. A matched executable/runtime/load receipt is still outstanding.
