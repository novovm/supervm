# CURRENT: C v2 observer window ended; active=false

The same PID6592 observer naturally reached its original bound at
2026-11-11T12:50:36.5703516Z (Beijing20:50:36). Actual wrapper exit0.
result.json exists: wait_expired=true, snapshots=0, terminal_observed=false.
error.json and launcher-error.json are absent. No relay-*.json exists.
The actual terminal liveness/result/exit and final logs are adjacent; all existing
raw observer outputs, read-anomaly first/latest and launch records are also in
final-original.zip, with SHA256/member hashes in final-result-public.json.
Historical liveness snapshots remain preserved; the previous READY is no longer active.

A's f4ad41fc attempt failed before relay dial and sent no object. B recovery does not
renew this completed C window. C did not re-arm, restart/update any node, change
keys/clock, send or retry. No relay-delivery acceptance is claimed from zero snapshots.

---

# C_RELAY_OBSERVER_ARMED — observer_version=2

Actual byte-preserved ready.json and current liveness.json are adjacent. Version2 helper PID6592,
node PID31296; original Watch-Relay.ps1 plus adjacent Read-RelayProgress.ps1, hashes in SHA256.json.
Ready2026-11-11T12:20:35.2926544Z (Beijing20:20:35); maximum1800s, approximately through20:50:35.

At this immediate snapshot liveness.active=true, matching observer/node PID and version2;
observer exit/result/error and launcher error were absent. Logs are snapshots available so far,
not final stdout/stderr. Parent remains independent/hidden and records actual exit when it occurs.
Original terminal/64-snapshot limits remain unchanged. No node/package/key/clock change or object send.
C did not operate D. A must use both hosts' fresh active receipts, not historical READY alone.

Old C v1 PID27480 had already exited1 at12:05:19UTC with0 snapshots and missing novonet.
Its PID was absent; terminal receipt matched. No process was killed. The original stop/absence check
is in old-observer-stop.json. Old raw READY/error/exit/logs are preserved in old-v1-terminal-original.zip;
this does not attribute the missing field to a specific writer window because original report bytes were not retained.

Do not launch another observer. Collect later results from the same local evidence directory.
If the current helper exits, its terminal receipt supersedes this readiness immediately.
