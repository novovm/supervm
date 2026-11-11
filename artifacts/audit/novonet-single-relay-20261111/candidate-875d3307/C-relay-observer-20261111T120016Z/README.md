# C_RELAY_OBSERVER_ARMED

Actual original Watch-Relay.ps1 ready.json is preserved byte-for-byte beside this file.

- main@4f44244e; fixed source 875d3307a1fc2ca34b45464ac3bf51a03619adc2.
- Ready UTC 2026-11-11T12:00:16.9208005Z; approximately 20:00:16–20:30:16 Beijing on 2026-11-11.
- Actual helper PID27480 (independent hidden admin PowerShell7); node PID31296.
- Read-only original watcher, original SHA256 2048bba7a26080fb774f3b97f9045e236c39345a9638ac822d77e2bb67a5e0b6.
- Maximum wait1800s, at most64 snapshots; original terminal/limit conditions remain unchanged.
- ready.json SHA256 `f3f366a52af5d0ef37518c7fe67968e039f5d26a2a1e6399d2923ee39324c201`; exact sizes/hashes in SHA256.json.
- Collector exit/result not yet present at this READY snapshot. Parent stays alive to capture actual exit;
  stdout/stderr remain in the local evidence directory from helper-started.json.
- No node restart/update, key change, clock change, socket/probe or object send. C did not operate D.
- A alone sends one A→C→D object after both independent current READYs. This is not relay acceptance.

Do not start another observer to retrieve results. After A sends, collect the existing result/error,
relay snapshots and exit/stdout/stderr from the same local directory. Preserve empty/failed outcomes.
