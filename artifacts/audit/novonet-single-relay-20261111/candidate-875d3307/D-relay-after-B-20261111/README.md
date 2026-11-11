# D_RELAY_OBSERVER_ARMED: new window after B recovery

User revised coordination: D starts when available, publishes real READY, then
A starts its timely sender. This is not the expired 21:18 A helper window.

- Original v2 Watch-Relay.ps1 plus adjacent Read-RelayProgress.ps1, main94d98656.
- Ready 2026-11-11 22:52:30.3554997 Beijing; maximum window until23:22:30,
  subject to original terminal/snapshot/error bounds.
- Actual observer PID13168, wrapper PID5784, unchanged node PID36464.
- Previous observer PID14224 exited0 at20:51:15, wait_expired=true, snapshots0.
  Its PID is absent; no old process was stopped and old outputs remain intact.
- One hidden independent administrator PowerShell7 observer launched. No node
  restart, package/key change, initialization or object send. No C operation.

Raw ready/liveness, startup command and creation time, script/helper hashes,
old termination verification and current terminal-record presence are included.
SHA256.json covers copied public files. liveness is a time-specific sample,
not a promise of future availability. A needs current C and D windows before
its one A->C->D send; READY does not establish relay delivery acceptance.

Local outputs: `D:\WEB3_AI\SUPERVM\target\d-relay-after-B-20261111`.
Original observer remains independent of the terminal tool, with real exit
captured by the wrapper. Do not rerun to retrieve later evidence.
