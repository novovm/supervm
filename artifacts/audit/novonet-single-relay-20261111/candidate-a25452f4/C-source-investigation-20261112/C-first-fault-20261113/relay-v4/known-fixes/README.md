# Two verified fixes; native AV remains unresolved

## Snapshot publisher

The same tick checked elapsed time once before collecting NOVONET observation,
then again after product.poll before writing progress. Crossing the interval in
between could write an otherwise complete JSON status with no `novonet` field.
The unchanged original reader first recorded that schema failure at16:52:35,
then exited1 at16:59:18 after its original15-second bound was exhausted.

The node now latches the due decision once and uses it for both observation and
emission. It never synthesizes a missing network status, returns stale cached
observation, weakens reader checks or extends its timeout. A tick crossing the
deadline waits until the next original tick instead of writing a partial report.

The deterministic regression extracts the actual scheduling/assignment source:
original a254 compiles but the crossing case fails (3pass/1fail); fixed source
passes4of4, including no observation before due and no fabricated disabled
network. It is a control-flow regression with minimal owner stubs, not a full
node or network test. Full node `cargo check` with iroh-transport and strict
Clippy both exited0. Rustfmt check exited0. Explicit SUPERVM target-dir avoids
the machine's inherited MEV build-directory setting. No MEV source was edited.

## Diagnostic cleanup timestamp

PowerShell converted ISO `created_utc` into a DateTime. The old guard compared
an ISO-formatted string to that object, falsely rejected the same creation time
and silently skipped debugger cleanup. V4 now normalizes both sides to UTC
ticks, preserves PID/path checks, rejects mismatches explicitly, and confirms
the matching debugger exits after termination. Five timestamp cases cover
actual JSON DateTime, string, offset-equivalent instant and reused-PID time
mismatch; AST parsing passed. The exact original failed launcher is retained.

The separate stop helper was never executed: UAC cancelled. Before another
request was necessary, CDB captured the17:08 first-chance AV and exited; node
and debugger were absent at read-only collection. No node restart or new
observer was launched. This helper is historical prepared source, not an
instruction to stop a reused PID.

These fixes are source/tooling changes only. No candidate executable was built
or installed. They do **not** establish the cause/fix of the original native
memory fault. The277,824,196-byte full dump remains private on C for analysis.
All old failures and original database/identity remain; C is NOT relay READY.
