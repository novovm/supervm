# D_RELAY_OBSERVER_ARMED (v2)

Following main c92d2172, the original Watch-Relay.ps1 and adjacent
Read-RelayProgress.ps1 launched once as a hidden administrator PowerShell7
observer. The earlier UAC request was cancelled before any helper started;
the renewed UAC was approved. Reader regression: 13 checks passed locally.

- Ready: 2026-11-11 20:21:13.9848722 Beijing.
- Observer PID14224, wrapper PID28148, unchanged node PID36464.
- Maximum window1800 seconds, approximately until20:51:14 Beijing;
  original terminal/snapshot bounds can finish earlier.
- Original old observer PID26224 had already exited1; no process was stopped.
  Old evidence retained. See old-observer-check.json.
- Script/reader SHA256, creation time and command: launch.json.
- Raw ready.json and sampled liveness.json are byte-preserved. The latter
  must be checked for freshness, version2 and active=true before A sends.
- No package change, node restart, key change, object send or operation on C.

Local evidence root:
`D:\WEB3_AI\SUPERVM\target\d-875-relay-observer-v2-20261111\evidence`.
The parent directory captures collector stdout/stderr and real exit code.
handoff.json records terminal-file presence at sampling, not a future guarantee.
READY is not relay delivery acceptance; A requires both current C/D receipts.
