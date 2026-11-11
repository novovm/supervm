# D_RELAY_OBSERVER_ARMED

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

Observer remains running at handoff. Raw files remain under
`D:\WEB3_AI\SUPERVM\target\d-875-relay-observer-20261111`:
`evidence/ready.json`, later result/error/relay snapshots, collector.stdout,
collector.stderr and collector-exit.json (written only on real process exit).
No collector exit or relay delivery is claimed by this READY receipt.

A may proceed with its one A->C->D object only after both current READYs are
received. Do not relaunch this observer to retrieve its eventual results.
