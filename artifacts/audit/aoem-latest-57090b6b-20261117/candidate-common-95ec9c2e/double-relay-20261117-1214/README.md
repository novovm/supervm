# Actual A→B→C→D object: A/B/D verified; C original snapshots pending

## A review13:19: D complete; C original-file return only

A pulled C/D receipts through6556e0c6. Independently verified all8 D original-file SHA256s and all3 matching raw snapshots, target PID23340/cursor2, same circuit/digest/native session and exact object/probe nonce. The original D receiver event `relay_probe_received_integrity_verified` is unique at12:14:44.453; authenticated peer-time interval `[1794888883915,1794888884042]` is wholly inside the original unchanged lifetime `[1794888883122,1794888893122)`. A recomputed original transfer_tag_v1 from session/NVBFT001/object and found exactly the matching1/1-fragment complete, hash-verified transfer with no duplicate data/repair. See `A-audit-D-double-relay.json` and the untouched D original observer directory.

This accepts A sender DONE plus D bounded receiver custody/integrity/time outcome, not a fabricated D transport_done_us timestamp (that field remains null), durable Business Receipt or consensus. Cross-host wall timestamps are not a one-way latency measurement.

C returned the same observer's terminal result (3snapshots, no error, early terminal exit), with relay-role counters9/9packets,9380/9044bytes, failed writes0/0. However it uploaded only a derived summary, not the3 original snapshots, and marked Object ID/session association unproven. An opaque intermediate relay normally does not expose the end-to-end object/session; `native_session=null` is not a transport failure. Its exact role/cursor1/circuit/path/digest/forwarding must be checked from the existing original snapshots. A has requested **only those already-saved files and hashes**, including non-Object-ID matches; no new capture, tool, object send, observer or node restart. Complete per-hop acceptance remains pending that C original-file audit.

## Original12:14 sender/B result (retained)

2026-11-17 Beijing12:14. A pulled C/D actual READY and liveness through9501a49d, checked same fixed95 program, observers active with no terminal files, then refreshed B's actual original owner. A was elevated; no update, new process owner, direct-object repeat, firewall block or new Full was used.

The existing one-shot sender published one original diagnostic reservation, waited for actual authenticated ordered B/C/D circuit/native-session readiness, and published **one** fixed object. Controller exit0. Original logs and projections are in this directory; `correlated-result.json` is the explicitly derived cross-record summary, not another network receipt.

- Object `e36af9089c38c9ec19835255934f04aa308ebc84c0bf42051bfa39a6026fcd5d`.
- Native session `b960c007c82275a45ee5c45f75705aab`.
- Circuit `457c502773ba39993a584d68cbbf1515`.
- Route digest `82ead291e70fd94915b90e0dc1591617826b32a64c1860e52b25ba7936521c14`.
- A diagnostic dial12:14:39.842; original object admitted12:14:43.857; sender Object DONE12:14:43.971, state Delivered. Controller observed/exited12:14:47.787. These are original A wall timestamps, not independently synchronized inter-host timings.
- B PID2621117 actual relay role/cursor0 matches the same circuit/digest/ordered path, authenticated downstream ready, forward counters9 packets/9380bytes and9 packets/9044bytes, failed writes0/0. Relay's native_session null is expected for opaque forwarding; correlation uses authenticated circuit/path, not an invented B end-to-end session.
- C actual forwarding and D integrity/time/final custody remain **pending their existing observer output**. Sender DONE is not a substitute for these records. Both hosts have been asked to return the same existing collectors' result/error, matching snapshots, exit/stdout/stderr, even if no match.

Do not restart observers/nodes, resend this object or repeat direct tests. Keep current owners running and return existing evidence. This sample uses original authenticated Iroh hop-stream relay carrying NativeObjectLink/native-object semantics; it is **not native UDP multihop**, an asset Business Receipt or consensus/finality acceptance.
