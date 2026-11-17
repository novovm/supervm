# D recovered object: received, integrity and time accepted

Returned only the existing PID40736 observer files, no new collection, resend or node operation. The same observer completed normally at2026-11-17T16:34:14.6259531Z (2026-11-18 00:34:14 Beijing), exit0, terminal_observed=true,2 snapshots, no timeout/capture-limit exhaustion. Final liveness is inactive. result.json and collector-exit.json exist; error/launcher-error/invocation-error do not exist. Missing error records are reported as absent, not fabricated.

- Exact Object2239b3c22fc30a7ba5b29f649d93271639ed439e93e6adb1cecd7cd275de9412, Session54868614a4c0ecda6864b04af6dad539 and Circuit55ff173d2812e03d6dc892f067620355 uniquely match D target/cursor2 on A->B->C->D. Route digest40bb81a55eb9f102834abbcea8acf756def545b8c069aac794dc296cb6e4dbb4.
- D recorded relay_probe_received_integrity_verified at2026-11-17T16:34:09.498Z (00:34:09.498 Beijing). Peer-time interval[1794933248653,1794933248770] lies inside validity[1794933247572,1794933257572), accepted rather than rejected_time. This is the receiver event time, not inferred latency from A's controller timestamp.
- Original transfer-tag derivation for session/streamNVBFT001/object yields49244b3297af25892290f3c7eaaee9c4f9f37100d68e6808d5c3f7add9916004. Exactly one matching receiver transfer:1/1 fragments assembled, hash_chunks1, verified_us1012744452, no duplicate data/repair/history hits, feedback emitted. Together with the actual target event this supports complete diagnostic target reception/integrity/time acceptance.
- Receiver transport_done_us/work_entered_us/work_finished_us/stored_receipt_created_us remain null: no invented receiver DONE timestamp, durable business receipt, ledger/asset settlement or finality. D target forwarding0/0 is not a C forwarding result. A/C must independently correlate sender DONE and actual opaque relay hop records; no assumption of in-flight concurrency or automatic/five-second restoration success.

All original relay-00.json and relay-01.json are returned byte-for-byte without Object/Session/Circuit filtering. returned-file-hashes.json covers raw snapshots, result, final liveness, collector exit and final stdout/stderr. Initial liveness is retained separately; initial READY/process/publication checks below remain historical. receiver-correlation.json is derived analysis only, not replacement evidence. No protected DB, key or private config upload. Do not rearm from this receipt.

## Original D_RELAY_OBSERVER_ARMED receipt

D synchronized main fc8badc1 and followed CURRENT2026-11-18 00:26. One original Watch-Relay.ps1 with adjacent Read-RelayProgress.ps1, HostId D, CommonCapacity8, TerminalCircuitCount1, MaximumWaitSeconds7200. Fixed source/node/WorkName/ExpectedOrigin/ExpectedPath are loaded directly from this candidate's recovery-observer-args.json. ExpectedPath stays a PowerShell array. Only the original stop condition is route-bound; all original new relay snapshots remain saved.

- Actual observer PID40736, hidden independent administrator PowerShell7; actual creation/command in process.json. READY2026-11-17T16:31:39.5727554Z (2026-11-18 00:31:39 Beijing).
- Actual node PID34148; original observer validates LocalService/fixed5b EXE. health-before.json records actual fresh original progress read, with no recovery/projection error/execution failures.
- Fresh EvidenceRoot: D:\WEB3_AI\SUPERVM\target\d-5b-recovery-observer-20261118\observation.
- Parent launcher PID45584 waits only for this observer exit to record actual code; no retry/restart. Stdout/stderr/exit retained in the parent directory. Current copies are point-in-time, not final logs.
- Precheck found no active original direct Watch-Relay or run-original observer wrapper; original db667 recovery PID45900 exit0 is retained. No old observer was stopped and no expired READY reused.
- Maximum window approximately2026-11-18 02:31:39 Beijing. Original64-snapshot, one-second sampling, fresh-progress and early terminal bounds unchanged.

Actual process command invokes local run-original.ps1, which performs a read-only original-reader health check and invokes the original Watch-Relay in that same process using the JSON values; it does not implement another collector/network owner. invocation-binding.json records invoker/binding hashes; actual ready.json contains expected route values. Latest copied liveness and publication-status.json record running and terminal-file presence; absent result/error/collector-exit are not fabricated. Initial process-status.json remains a point-in-time record. Local original files keep updating.

No node restart/update, ZIP copy, object send, B outage, C operation or old-test rerun. Readiness does not establish recovered-object delivery, true concurrent overlap, automatic/five-second restore, asset or consensus finality. Keep this same bounded window; do not rearm from this receipt.
