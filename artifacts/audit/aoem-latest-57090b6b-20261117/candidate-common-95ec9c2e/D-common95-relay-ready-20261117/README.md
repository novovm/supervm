# D receiver result: same common95 observer completed

Returned existing PID51148 evidence only; no new collector, resend, node restart or node update. Original READY is retained below. The observer exited0 at2026-11-17T04:14:51.9907909Z (12:14:51 Beijing), terminal_observed=true,3 snapshots, no timeout/capture-limit exhaustion. Final liveness is inactive; error/read-anomaly/launcher-error records are absent, not replaced with fabricated empty files.

- Object: `e36af9089c38c9ec19835255934f04aa308ebc84c0bf42051bfa39a6026fcd5d`.
- Native session: `b960c007c82275a45ee5c45f75705aab`.
- Circuit: `457c502773ba39993a584d68cbbf1515`; route digest `82ead291e70fd94915b90e0dc1591617826b32a64c1860e52b25ba7936521c14`, matching A's published correlation.
- D node23340 is target/cursor2. `relay_probe_received_integrity_verified` for that exact object/session was recorded at04:14:44.453Z (12:14:44.453 Beijing). D's forwarding counters0/0 are consistent with final-target role, not evidence of C forwarding.
- Time reference interval `[1794888883915,1794888884042]` falls within probe validity `[1794888883122,1794888893122)`. The receiver accepted the time window, not `relay_probe_rejected_time`.
- Original `transfer_tag_v1` using that session, streamNVBFT001 and object yields `eff7e10e711114f9dcdd685cbb80049fd2d8eb9d0e6989e8c5bda03eda3da57c`. Exactly one final snapshot transfer matches:1/1 fragments assembled, hash_chunks1, verified_us2114581660, feedback emitted, no duplicate data/repair. Combined with the target receiver event, this establishes diagnostic final reception/integrity/time acceptance.
- Receiver-side transport_done_us, work_entered_us, work_finished_us and stored_receipt_created_us are null; no fabricated receiver DONE timestamp or durable business receipt. This is authenticated Iroh hop-stream relay carrying the native object channel, not native UDP multihop, asset settlement or consensus finality. C actual forwarding is independently audited by C/A.

`relay-00.json` through `relay-02.json` are the three original matching-circuit snapshots (00 precedes object arrival;01/02 include reception). result.json, collector-exit.json, final liveness and final stdout/stderr are byte-preserving copies; see returned-file-hashes.json. receiver-correlation.json is explicitly derived analysis, not a replacement raw result. Initial liveness is retained as initial-liveness.json, and earlier process/publication records remain historical. Do not rearm.

## Original D_RELAY_OBSERVER_ARMED receipt

D synchronized main6d386e23 and started one original Watch-Relay.ps1 with its adjacent Read-RelayProgress.ps1, HostId D, -Common95 and MaximumWaitSeconds7200. Product binaries, node lifecycle and configuration were not changed. C was not operated.

- Actual observer PID51148, created2026-11-17T04:10:47.8517560Z (12:10:47 Beijing), hidden independent administrator PowerShell7.
- Original node PID23340; fixed common95 EXE hash verified by the original observer.
- Actual READY2026-11-17T04:10:48.9611648Z (12:10:48 Beijing). Maximum window ends approximately14:10:49 Beijing; terminal observation or64-snapshot bound can end it earlier.
- Fresh local EvidenceRoot: D:\WEB3_AI\SUPERVM\target\d-common95-relay-20261117-1210\observation.
- Local launcher/exit/stdout/stderr directory: D:\WEB3_AI\SUPERVM\target\d-common95-relay-20261117-1210. Launcher PID47312 waits only to record this observer's exit; it does not restart anything.
- Elevated precheck found no existing Watch-Relay process. No observer was stopped or restarted.

Adjacent ready.json and liveness.json are actual observer output. process.json contains actual creation time and command; process-status.json records terminal-file presence at initial readiness; publication-status.json records the subsequent publication check. stdout/stderr are point-in-time copies, not final logs. No result/error/collector-exit is fabricated when absent. Local originals continue updating.

A may use the fresh D READY together with C's independently obtained READY for its own ordered A->B->C->D operation. No object was sent here. Readiness is not relay forwarding, receiver integrity, Object DONE, business finality or production acceptance. Do not launch another collector from this receipt.
