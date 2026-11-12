# D target receipt: matching relay object integrity verified

Readback only from the same observer PID18808, original node PID36464.
No new capture, object send, node restart or change on C was performed.

At 2026-11-12 09:47:23.907 Beijing the original diagnostic owner recorded
`relay_probe_received_integrity_verified` for exactly:

- Object: a8536fd1dbd85b4ffa4d3ff8ff7ac4220c2e00d0fa95b62347fdf2ad1db09fb3
- Session: 1689752becc7449ff129b61be6f6cf95
- Circuit: 84140e79686cf652db40ca81f5b3a9d9
- Route digest: 3a2a824925a227cb51f840213f5afaf85e064304d0e7c25297c058a6abd65aae

D circuit role is target; native session matches the integrity event.
Authenticated and time-reference-ready events precede receipt. The recorded
peer-time interval1794448043316..1794448043352 lies within the probe validity
interval1794448043090..1794448053090. Both circuit failed-write counters are0.
This supports the target's diagnostic receipt/integrity/time acceptance, not
business execution or durable ledger acceptance. C's actual bidirectional
forwarding and A's sender DONE must still be joined with their original evidence.
D target forwarded counters are0 and are not presented as C forwarding proof.

Observer ended at09:47:30 with exit0, terminal_observed=true, snapshots2,
wait_expired=false, capture_limit_reached=false, final active=false. No error.json.
original-public.zip contains all10 raw original public outputs: two relay
snapshots, ready, final liveness/result, first/latest read-anomaly, actual
collector-exit and stdout/stderr. Every ZIP member was checked against the source
SHA256; members.json and archive.json provide hashes. stderr is empty.
match-summary.json is explicitly derived from relay-01.json, not raw evidence.

Read anomalies remain preserved; rejected missing-novonet reads were not counted
as accepted observations. No claim of gap-free collection or mainnet acceptance.
