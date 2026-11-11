# D relay observer: READY then FAILED; do not send

**Current status: FAILED, not armed.** Before push, the observer exited1 at
2026-11-11 20:02:36 Beijing, with zero snapshots. Watch-Relay.ps1 line28 reported
`The property 'novonet' cannot be found on this object. Verify that the property exists.`
The failed read's original progress bytes were not retained by the script;
the underlying reason for the absent property is not established. No retry,
node restart or object send was performed. A must not send based on this READY.
Original ready.json is historical evidence of initial readiness only.

Following main4f44244e CURRENT, D launched the original Watch-Relay.ps1 once
in an independent hidden administrator PowerShell7 process. Original ready.json
is returned immediately, not after completion; SHA256.json records raw hashes.

- Actual observer PID26224; exit-capturing wrapper PID3296.
- Original875 LocalService node PID36464.
- Source875d3307a1fc2ca34b45464ac3bf51a03619adc2.
- Ready at 2026-11-11 19:59:46.9577810 Beijing (11:59:46.9577810UTC).
- Maximum wait1800 seconds, until approximately20:29:47 Beijing. The original
  script may finish earlier after terminal evidence or its64-snapshot bound.
- Script SHA2567f878be67d9e7775b3376aa00e7e202673d59beb81dff1b712949c3ef3fe9110.
- No node restart, package update, key change, clock change or object send.
  C was not operated on; this receipt does not assert C readiness.

Observer has exited; raw files remain under
`D:\WEB3_AI\SUPERVM\target\d-875-relay-observer-20261111`:
`evidence/ready.json`, later result/error/relay snapshots, collector.stdout,
collector.stderr and collector-exit.json (actual exit1). Original failure,
stdout/stderr and exit evidence are included alongside READY.

A must wait for a separately resolved, currently active observation window.
Do not relaunch this observer to retrieve its results.
