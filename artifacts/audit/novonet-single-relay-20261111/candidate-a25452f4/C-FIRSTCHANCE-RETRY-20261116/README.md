# C First-Chance retry: authorized single run in progress

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

As of approximately 27m53s, both processes remain live and original periodic
progress reports continue. No AV/dump has been captured. Existing counters
show real communication activity; one protocol-time peer has accepted samples,
two remain not ready. This is not four-peer/consensus acceptance or a fix claim.

At first AV or the bound, end this run and retain original database, identities
and historical evidence. Dual relay is not resumed. PID13020/38324 retrospective
closure and PID39152's separate XMM7/Hashbrown boundary remain unchanged.

Private local run material:
`target/c-firstchance-prepared-20261116T115807Z`.
Do not start another node for this handoff. Final result is pending.
