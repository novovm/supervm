# Same D observer completed: all8 original snapshots returned

2026-11-17: PID50540 already exited0 at10:31:14.9162722Z (18:31:14 Beijing), before this return request. result.json reports terminal_observed=true,8 snapshots, wait_expired=false and capture_limit_reached=false. Final liveness is inactive. No error/launcher-error record. This was the original TerminalCircuitCount4 early exit, not a two-hour timeout or manual termination. It cannot capture any later recovery object in this same window. No observer was restarted or node changed to compensate.

All relay-00.json through relay-07.json are copied byte-for-byte, without Object/Session filtering; snapshot-sha256.json records each length/SHA256. result, final liveness, collector-exit and final stdout/stderr are returned. Original initial liveness is preserved as initial-liveness.json; old READY and initial process checks remain historical. current-state.json records actual file presence. final-snapshot-index.json is a derived circuit/event index only, not replacement evidence.

- B object dacc8cf0e64e261b37bef2b6f246e466debf73d0a276ae48ca01035e94551b34: D records relay_probe_received_integrity_verified, session cad7f1818f501775790b91b8ff088c03, event wall_ms1794911354650. Authenticated peer-time interval[1794911354437,1794911354880] is inside probe validity[1794911354405,1794911364405). Target/cursor2 circuit af188fd2a5b1d7b09c09ad3954a37202, route digest0f8d47d3321a36c3dd4508508e24eca55d5bd3574f0e597eebfd866c825fbaa2. This is receiver diagnostic integrity/time acceptance, not a durable business receipt.
- A-origin A->B->D->C: D relay/cursor1 circuit7a04952558722e4461dc0d4be9ef9e36, route digest9a5a051e0bdf0b894b2323a81e8cd497da0cc97b2211b7f42b91c4b518182bba. Final observed forwarding9packets/9380bytes and9packets/9044bytes, failed writes0/0. Opaque relay native_session is null: bind A object528a2ae9174fa5d4dc89b1d1023e28e07f5422e6b0468234e0012c8783a5706a using A/C circuit/route/hop evidence, not an invented local object mapping.
- Final snapshot has four closed circuits, including the above two plus e405f669ded4a84d9adf81f25fabccf4 and d48f99d87a6bf2aefd13c7abed018b60. Four closed circuits do not by themselves establish that the intended recovery scenario completed. No new recovered-object integrity event is claimed here; A must correlate the full workload and any subsequent timing.

No fresh read from protected node/config was made during this return; only existing observer files were read/copied. No new window, resend, restart/update, package operation, private key or full private configuration upload. The previous live-window statement below is historical and superseded by these terminal records.

## Original D_RELAY_OBSERVER_ARMED

D synchronized main c5005339 and followed CURRENT18:16. Started one original Watch-Relay.ps1 plus adjacent Read-RelayProgress.ps1, HostId D, fixed db667 source/node hash/WorkName, CommonRoleFix, TerminalCircuitCount4, MaximumWaitSeconds7200. No observer source or node changes.

- Actual observer PID50540, created2026-11-17T10:20:43.7852390Z (18:20:43 Beijing), hidden independent administrator PowerShell7. Actual command and creation time are in process.json.
- Actual READY10:20:44.9023410Z. Node PID56148, original LocalService/fixed EXE validated by observer.
- New EvidenceRoot: D:\WEB3_AI\SUPERVM\target\d-db667-observer-20261117\observation.
- Launcher PID32280 waits only for this observer's exit to retain exit code; no restart/retry. Local stdout/stderr and collector-exit.json (once produced) reside in the parent directory.
- Elevated precheck found no existing observer. Previous common95 observer40456 had exited0 at08:22:38Z; its evidence remains unchanged.
- Original bounds retained:7200seconds maximum,64 snapshots, one-second sampling, fresh-progress checks, early terminal exit. Maximum window ends approximately20:20:45 Beijing.

ready.json and current liveness.json are real observer outputs. process-status.json describes initial READY; publication-status.json gives the later liveness/terminal-file check. stdout/stderr are point-in-time copies, not final logs. Missing result/error/collector-exit are reported as absent and not fabricated. Local originals continue updating.

No node update/restart, ZIP copy, object send, C operation or old-test rerun. READY does not establish concurrent delivery, relay recovery, receiver custody or business/finality acceptance. Keep this same observer/window and return its original records after A's workload; do not rearm from this receipt.
