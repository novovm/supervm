# C First-Chance attempt: stopped for callback configuration error

Baseline: `d6ac9935`. User authorized one original C run, at most 75 minutes,
with the debugger present before business execution; first AV capture followed
by controlled termination, no automatic restart and no new collector.

## Actual run

- Original a254 EXE SHA-256: `1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d`.
- Actual loaded AOEM966 SHA-256: `5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`.
- Original database opened with `existing`; original three identities retained.
- LocalService account independently confirmed. Node PID34156, CDB PID20796.
- Node created 2026-11-16 11:52:05.751 UTC; controlled end around 11:55:05 UTC.
- CDB initial stop and AV filter setup preceded business execution. No code or
  data breakpoints; normal heap selected with `-hd`; no ASan/PT/TTD/page heap.
- Existing progress reached 87,977 ticks; transport reported 776 data writes,
  78 accepted data and 686 history data observations. These are node counters,
  not independent peer-side receipt or four-machine consensus acceptance.

## Configuration error and stop

The quoted `sxe -c` callback used an absolute command-file path with unescaped
backslashes. CDB's displayed effective callback had lost path separators.
Therefore an AV would not reliably execute the intended capture command file.
The agent detected this in the live filter listing and ended the run rather
than continue with an invalid capture configuration. The error belongs to this
diagnostic setup and provides no evidence of a NOVOVM production defect.

No AV or First-Chance full dump was captured. Exit code -1 was from controlled
termination, not a natural crash or successful completion. This is a failed
capture preparation/run, not a stability qualification or memory-crash fix.

## Offline correction only

Use the relative callback `sxe -c "$$><capture.cmd" av` with `capture.cmd` in
CDB's working directory, avoiding nested Windows path escaping.

Under LocalService, the same installed CDB opened the existing PID38324 dump
read-only, retained an identically formed relative callback in `sx`, and opened
a harmless relative command file successfully; exit code 0. No new node,
synthetic exception or intentional crash was used. This verifies command
parsing and file resolution only; it does not qualify a real First-Chance dump.
A corrected run is prepared locally but has not been started or registered.

## Preservation and remaining boundary

Both owned processes exited and the live/offline tasks were removed. All three
identity hashes match before/after. WER configuration unchanged; no new WHEA
record IDs. Original database retained, with normal writes from this real run;
it is not claimed byte-for-byte unchanged. Historical dumps and production
code/binaries were not overwritten or modified. Dual relay remains paused.

No automatic retry: the user authorized one start. A further start requires a
new explicit instruction. Goal remains unresolved. PID13020/38324 postmortem
closure and the separate PID39152 XMM7/Hashbrown evidence boundary remain as
recorded; this failed attempt supplies no new causal evidence.

Private local evidence: `target/c-firstchance-once-20261116T114953Z`.
Only this sanitized result is published; configs, identities, raw logs, module
manifests and historical dumps remain local.
