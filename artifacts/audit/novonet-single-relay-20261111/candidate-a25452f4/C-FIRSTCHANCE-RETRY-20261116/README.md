# C First-Chance retry: bounded run ended, no AV captured

User explicitly authorized one retry, at most 75 minutes, after the callback
path error in the previous attempt. No automatic restart is authorized.

Actual node PID41648, debugger PID31520, LocalService independently confirmed.
Node created at 2026-11-16 12:02:28.712868 UTC (20:02:28 Beijing).
Hard execution limit: 13:17:28 UTC (21:17:28 Beijing); native run limit 4485s
leaves a small margin inside the 4500s debugger-owner bound.

Original a254 EXE SHA-256:
`1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d`.
Actual loaded AOEM966 SHA-256:
`5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`.
Original database opened `existing`; original identity hashes verified before
launch. No production code or binary replacement; WER configuration unchanged.

CDB initial-stop log proves AV setup before business execution. Effective
callback in `sx` is exactly `$$><capture.cmd`, with the command file copied and
hash-checked in the debugger working directory. No code/data breakpoints;
normal heap selected with `-hd`; no ASan, PT, TTD or page heap. The first AV
callback saves `.dump /ma`, exception record/context, XMM registers, code bytes,
memory mapping, all thread stacks and module data, then quits without resuming
the exception. First-chance provenance must be verified from actual event
output; callback configuration alone does not prove successful capture.

## Actual final result

The node ended within its native run limit, with exit code 0. Observer completion
was 2026-11-16 13:17:16.078 UTC (21:17:16 Beijing), after 4487.66s, approximately
74m48s. This is inside the authorized 75-minute maximum; it is not a claim of
75 full minutes. Owner reason is `debugger_exited`, not the 75-minute kill bound.

CDB recorded no access-violation event. The AV callback was never executed,
there was no command-file open error, and no new dump was generated. Therefore
the actual First-Chance capture path has not been exercised or qualified by
this run. The initial loader breakpoint is not an application AV.

Final native stdout summary, later than the last periodic progress file:

| Observation | Result |
| --- | --- |
| Ticks | 2,156,977 |
| Transport reported data writes | 19,402 |
| Accepted data / repair | 1,930 / 2 |
| History data / repair observations | 17,485 / 1,551 |
| Protocol-time peers | One ready, 735 accepted samples; two not ready |
| Current height | 1 |
| Executed batches / durable decisions | 0 / 0 |

This confirms activity on the actual original node's query/serialization and
communication paths. Local transport/history counters are not independent
peer-side business receipts. No advancing-chain, transaction throughput or
four-machine acceptance is established. Debugger attachment also changes timing.

## Evidence acceptance and preservation

| Required evidence | This run |
| --- | --- |
| First-Chance AV ExceptionRecord | Not obtained; no AV occurred |
| Fault-thread context, including XMM6/XMM7 | Not obtained |
| New fault instruction and effective address | Not obtained |
| All thread stacks at the first AV | Not obtained |
| `.dump /ma` at first AV | Not obtained |
| Original EXE/AOEM hashes, loaded modules, symbol baseline | Preserved locally |

Both owned processes exited and the one-shot task was removed. Identity hashes
all match before/after. WER is unchanged. WHEA query completed successfully:
14 existing records, no new record IDs; absence of WHEA does not exclude hardware
faults. Original database is retained, including normal writes from this run;
it is not claimed byte-for-byte unchanged. PID13020, PID38324 and PID39152 dump
hashes match their recorded baselines. EXE/PDB symbol-baseline hashes also match.
No production code/binary modification, new collector, automatic retry or dual
relay recovery occurred.

The first corrupting instruction and production repair remain unknown. This run
adds a bounded non-reproduction result, not new causal evidence. Do not repeat
the same run automatically or reopen closed historical speculation. PID13020/
38324 retrospective closure and PID39152's separate XMM7/Hashbrown boundary remain
unchanged. The repair goal is not complete.

Private local run material:
`target/c-firstchance-prepared-20261116T115807Z`.
Public structured results: [result](result-public.json) and
[historical preservation](preservation-public.json). Raw stdout, configuration,
module manifests and debugger material remain private and outside product/release.
