# D_RELAY_OBSERVER_ARMED

D synchronized main c5005339 and followed CURRENT18:16. Started one original Watch-Relay.ps1 plus adjacent Read-RelayProgress.ps1, HostId D, fixed db667 source/node hash/WorkName, CommonRoleFix, TerminalCircuitCount4, MaximumWaitSeconds7200. No observer source or node changes.

- Actual observer PID50540, created2026-11-17T10:20:43.7852390Z (18:20:43 Beijing), hidden independent administrator PowerShell7. Actual command and creation time are in process.json.
- Actual READY10:20:44.9023410Z. Node PID56148, original LocalService/fixed EXE validated by observer.
- New EvidenceRoot: D:\WEB3_AI\SUPERVM\target\d-db667-observer-20261117\observation.
- Launcher PID32280 waits only for this observer's exit to retain exit code; no restart/retry. Local stdout/stderr and collector-exit.json (once produced) reside in the parent directory.
- Elevated precheck found no existing observer. Previous common95 observer40456 had exited0 at08:22:38Z; its evidence remains unchanged.
- Original bounds retained:7200seconds maximum,64 snapshots, one-second sampling, fresh-progress checks, early terminal exit. Maximum window ends approximately20:20:45 Beijing.

ready.json and current liveness.json are real observer outputs. process-status.json describes initial READY; publication-status.json gives the later liveness/terminal-file check. stdout/stderr are point-in-time copies, not final logs. Missing result/error/collector-exit are reported as absent and not fabricated. Local originals continue updating.

No node update/restart, ZIP copy, object send, C operation or old-test rerun. READY does not establish concurrent delivery, relay recovery, receiver custody or business/finality acceptance. Keep this same observer/window and return its original records after A's workload; do not rearm from this receipt.
