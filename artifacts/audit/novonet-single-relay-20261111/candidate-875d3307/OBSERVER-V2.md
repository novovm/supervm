# Relay observation recovery, not node deployment

D READY11:59:46.9577810UTC was followed by exit1 at12:02:36.0380319UTC,
zero snapshots, missing `novonet` at Watch-Relay.ps1:28. All six original
receipt hashes in validator-d-875-relay-armed-20261111/SHA256.json were checked.
A sent no new object. Historical READY is not current readiness.

In fixed875 novovm-node.rs the first report-interval check creates optional
novonet_observation; product.poll runs; a second independent interval check
writes Status, adding novonet only if the first check captured it. A deadline
crossing between checks permits an otherwise valid report without that field.
The report uses fs::write, so readers must also reject incomplete JSON while
writing. Neither potential path proves which bytes D read: they were not saved.

The read-only watcher now waits for an actual complete fresh projection within
15 seconds (the existing freshness boundary), preserves bounded public anomaly
metadata, refuses wrong chain/failed execution/disabled diagnostics/incomplete
transport, and publishes versioned liveness plus terminal inactive status.
It does not combine old and new snapshots or accept absent fields. Observation
gaps may still prevent evidence acceptance. The source writer issue is not
claimed repaired; no running executable changed.

Local PowerShell7 test: scripts/tests/relay-observation-reader.tests.ps1,
13 checks: valid projection, persistent missing field, malformed/empty JSON,
wrong chain, failed execution, disabled owner, missing transport, stale report,
missing-to-complete transition, retained first anomaly, watcher syntax and
oversized report. Tests use the actual shared reader, not a copied reader.
This is helper validation only, not relay or four-host network acceptance.

A's existing local one-shot sender uses the same reader and requires v2 READY,
matching observer/node PID liveness sampled within180 seconds, and absence of
known terminal receipts. Remote copied liveness is a bounded snapshot, not a
guarantee of future collector survival. Final acceptance still requires actual
C forwarding and D integrity/time acceptance with matching A/D object/session
and original DONE. No repeated direct delivery, new socket or business action.

Only C/D observation helpers need one controlled replacement; no new program
ZIP, Full CI, node restart, key change, cache reset or database operation.
