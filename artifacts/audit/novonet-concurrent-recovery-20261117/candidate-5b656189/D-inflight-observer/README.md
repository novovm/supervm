# D_RELAY_OBSERVER_ARMED: two-route in-flight workload only

D synchronized main df8b28f8 and followed CURRENT2026-11-18 00:45. One original Watch-Relay.ps1 and adjacent Read-RelayProgress.ps1, HostId D, fixed5b Source/NodeSha256/WorkName, CommonCapacity8, TerminalCircuitCount2, MaximumWaitSeconds7200. ExpectedOrigin/ExpectedPath are NOT passed: actual ready.json shows empty origin and path. Both A->B->D->C and B->A->C->D are observable, with all original snapshots retained.

- Actual observer PID57340, created2026-11-17T16:48:56.5784740Z (2026-11-18 00:48:56 Beijing), hidden independent administrator PowerShell7. READY16:48:58.2604610Z.
- Node PID34148, validated by original observer against fixed5b EXE/LocalService. Original-reader precheck16:48:57Z: height1, failures0, recovery false, projection error null.
- Fresh EvidenceRoot: D:\WEB3_AI\SUPERVM\target\d-5b-inflight-observer-20261118\observation.
- Launcher PID40964 records only this observer's eventual exit, without retry/restart. Actual command/creation time in process.json. Local run-original.ps1 calls the original Watch-Relay in the same child process; no second collector/network owner. Invocation path/hash and explicit argument facts recorded in invocation.json.
- Elevated precheck found no direct Watch-Relay or run-original observer alive. Previous recovered-object observer40736 already exited0 at16:34:14Z; original evidence unchanged. No duplicate/termination.
- Maximum window approximately2026-11-18 02:48:58 Beijing; original early-terminal/64-snapshot/freshness/sampling bounds unchanged.

Real ready/current liveness, actual PID/creation/command, node PID and terminal-file presence are adjacent. stdout/stderr are point-in-time copies; local originals continue updating. No fabricated absent result/error/collector-exit. Initial process-status/publication-status are point-in-time facts, not future health guarantees.

No node restart/update, ZIP copy, object send, C operation, B outage or old/recovery-test rerun. Readiness is not measured in-flight overlap, dual-end delivery or finality. Keep this same window; after A's one paired workload return all original snapshots/terminal logs without filtering. Do not rearm from this receipt.
