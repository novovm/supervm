# Same D observer interim readback

Sampled 2026-11-11 23:19 Beijing from the existing v2 observer PID13168,
node PID36464. Process creation identity matches its original launch record.
Raw liveness is active=true, snapshots0. No relay-*.json, result.json,
error.json or collector-exit.json exists at sampling. Missing terminal files
are explicitly recorded in presence.json, not replaced with synthetic results.

Read-anomaly first/latest metadata and available stdout/stderr are retained
with SHA256.json. Missing-novonet observations are rejected reads, not relay
success or a terminal error. No restart, new collector or object send occurred.
The original window still ends approximately23:22:30; this is a timestamped
interim receipt, not a guarantee of continued liveness or delivery acceptance.
