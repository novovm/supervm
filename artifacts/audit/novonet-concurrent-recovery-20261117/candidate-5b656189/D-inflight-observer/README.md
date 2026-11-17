# D original paired-workload return: delivery evidence, NO concurrency pass

Same PID57340 observer exited0 at2026-11-17T16:52:13.4171545Z (2026-11-18 00:52:13 Beijing), terminal_observed=true,3 snapshots, no timeout/capture-limit exhaustion. Final liveness inactive, result/collector-exit present, error/launcher-error/invocation-error absent. No new collector, resend or node restart/update.

- All3 raw relay-00..02 snapshots are returned without Object/Session/Circuit filtering. returned-file-hashes.json records original byte lengths/SHA256 for snapshots/result/final liveness/exit/stdout/stderr. Initial liveness retained separately; initial READY and process/publication checks remain historical.
- B object0b6ce6deac7baf19087132c31cdf9cf42e31fec97919e2b49ede280c1e867e5f/session2006a82a71e890675d2f1e116ab64e43 uniquely matches D's relay_probe_received_integrity_verified event. Peer-time interval[1794934321046,1794934321593] is within validity[1794934317678,1794934327678). Target/cursor2 circuit158b1b1f563306b76338e16ce49d130f, route digestb6021e57dd4d0aecdb89cb3f1daa6f659e64604efb1777db49138308089cfff5. Exact original session/streamNVBFT001/object transfer-tag correlation and receiver transfer are in receiver-and-forwarding.json.
- A-origin A->B->D->C: D relay/cursor1 circuitbfef7d3eff8d06e5e3b34e3105dfd509, route digestb070d42bd0bdb3ac7f7b6fa9313145fb5756482d480be588a9dea6ac5d26470b. Actual final forwarding9packets/9380bytes and9packets/9044bytes, failed writes0/0. Relay native_session=null is expected opaque-hop semantics, not missing target identity. A must correlate its objectebb48a05c4e57c7db319d336decd9014aae78821c762f2956b12c9cc6befc589/sessionbe68e3543249f73571d901ce97dc0781 using its own Circuit/Route/hop evidence, not fabricate a relay Object mapping.
- The final snapshot also retains the prior recovered-object event2239...; it is historical, not a new object in this workload. Current B correlation uses exact requested Object/Session, while no raw snapshots are filtered.

A's published TIMING-CORRELATION.json reports conservative overlap0 and no concurrent in-flight proof. D reception/forwarding does not override that timing verdict. No new business/durable receipt/finality, automatic recovery or concurrency acceptance claimed. Raw receiver business work/receipt timestamps remain as observed, not filled. No private keys/full config/database uploaded. This window is now closed; do not rearm from the return.

## Original D_RELAY_OBSERVER_ARMED

D synchronized main df8b28f8 and followed CURRENT2026-11-18 00:45. One original Watch-Relay.ps1 and adjacent Read-RelayProgress.ps1, HostId D, fixed5b Source/NodeSha256/WorkName, CommonCapacity8, TerminalCircuitCount2, MaximumWaitSeconds7200. ExpectedOrigin/ExpectedPath are NOT passed: actual ready.json shows empty origin and path. Both A->B->D->C and B->A->C->D are observable, with all original snapshots retained.

- Actual observer PID57340, created2026-11-17T16:48:56.5784740Z (2026-11-18 00:48:56 Beijing), hidden independent administrator PowerShell7. READY16:48:58.2604610Z.
- Node PID34148, validated by original observer against fixed5b EXE/LocalService. Original-reader precheck16:48:57Z: height1, failures0, recovery false, projection error null.
- Fresh EvidenceRoot: D:\WEB3_AI\SUPERVM\target\d-5b-inflight-observer-20261118\observation.
- Launcher PID40964 records only this observer's eventual exit, without retry/restart. Actual command/creation time in process.json. Local run-original.ps1 calls the original Watch-Relay in the same child process; no second collector/network owner. Invocation path/hash and explicit argument facts recorded in invocation.json.
- Elevated precheck found no direct Watch-Relay or run-original observer alive. Previous recovered-object observer40736 already exited0 at16:34:14Z; original evidence unchanged. No duplicate/termination.
- Maximum window approximately2026-11-18 02:48:58 Beijing; original early-terminal/64-snapshot/freshness/sampling bounds unchanged.

Real ready/current liveness, actual PID/creation/command, node PID and terminal-file presence are adjacent. stdout/stderr are point-in-time copies; local originals continue updating. No fabricated absent result/error/collector-exit. Initial process-status/publication-status are point-in-time facts, not future health guarantees.

No node restart/update, ZIP copy, object send, C operation, B outage or old/recovery-test rerun. Readiness is not measured in-flight overlap, dual-end delivery or finality. Keep this same window; after A's one paired workload return all original snapshots/terminal logs without filtering. Do not rearm from this receipt.
