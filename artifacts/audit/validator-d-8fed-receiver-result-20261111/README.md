# D original receiver capture returned

The existing collector PID21680 automatically completed at
2026-11-11 11:13:39 Beijing, exit 0. No collector rerun, resend, node restart,
update or other host operation was performed to obtain this receipt.

Object: a89c1dac10c0405ca4d153bb03b03279d6339cd4d465b7aa81352b1902dcafd4.
Session: d748367f3ba5e381bc8468467db43fb0.
Nonce: c1850dd49d79e5e33a241e12acbc759b9483a9abbbefdc76982ea2e56116d26d.

All seven existing receiver snapshots contain the matching object's
probe_pending_time_boundary and probe_received_integrity_verified events,
native_udp, source route 192.168.71.74:41051. These are repeated observations
of the same object, not seven unique deliveries. The full bounded public
transport projection is preserved without filtering or rewriting.

Original result: snapshots=7, terminal_event_observed=true, wait_expired=false,
node_mutated=false, business_receipt=false. stderr is empty; error.json and
launcher-error.json were not produced. No absent error file was manufactured.

original-receiver-results.zip contains exact original ready.json, result.json,
receiver-00.json through receiver-06.json, collector-exit.json, stdout/stderr
and armed-process.json. SHA256SUMS.json binds archive and all member bytes.
The archive is public diagnostic evidence only, not program binaries/config,
keys or database. A should match these originals against its actual sender
DONE; this return alone does not certify consensus, business or all relay paths.
