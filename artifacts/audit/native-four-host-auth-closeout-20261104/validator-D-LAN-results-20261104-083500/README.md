# D LAN scheduled results: A/D probes and C/D observer

Node **validator-D**, date 2026-11-04 Asia/Shanghai. The original retained script finished with **exit0**; all **three original cases exited0**, with empty stderr. **Both D-to-A probes confirmed authenticated round trips with different sessions. D's observer authenticated two C requests and submitted two replies. The later concurrent C receipt confirms both C/D client round trips and matches D's last session/source; C/A first failed and second was skipped. A-side originals and final matrix reconciliation remain with A.**

Schedule: main `53fe7da3acbb80b7f3731e59e083616bbafad304`, common start08:35. Actual original script SHA256 remains `d88d974416cd5180f1523f7b1752ab3b73e8acd4525f7d818c8d4a721079fcf6`; candidate program remains `d7ef793b819c8670e472d217daae9db5f074aea8`. This review did not run another probe/observer, retry, catch-up or modify an original report.

| Original case | Start / end (Beijing) | Exit | Actual result | Session ID (16-byte hex) |
| --- | --- | --- | --- | --- |
| probe-D-A-first | 08:38:00.9330656 / 08:38:01.1676464 | 0 | accepted=true; round_trip_confirmed=true; exchanges1 | `3f2fb23576b2df66dec86ad1c99a67c7` |
| probe-D-A-second | 08:38:01.1824201 / 08:38:01.4387390 | 0 | accepted=true; round_trip_confirmed=true; exchanges1 | `29161f9a2c54ad3f15aeb11fba478586` |
| observer-D-for-C | 08:40:45.3496692 / 08:42:15.4853754 | 0 | accepted=true; requests2; exchanges2; replies submitted2 | `5114b1a40e9f555c99692ea44591a1e7` (last session only) |

Both reserved D job starts fall within the unchanged original script's schedule tolerance. First completed successfully before second began. These are process timestamps, not isolated handshake latency or TPS. The observer's **round_trip_confirmed=false** is its original server-side semantics: submitting replies does not prove the client received them. It is not a failed probe, and accepted=true alone must not be promoted into completed C/D client delivery. Its final report retains only the last C session/source, not a complete first-session history. Client confirmation below comes from C's separately committed reports, not a reinterpretation of the observer flag.

## Identities and LAN evidence

- D local peer in all three reports: `novovm-mldsa65:e4759711dc7c5ded8c2625cc72802e69107d53dfcbca2a69d3cc093764923166`, matching the original D full public key. Public-key SHA256 remains `30019ff63f5f8c1f77029990aaf2657c3bb6dad562d957d78f0b60d8d349c934`.
- A peer in both probes: `novovm-mldsa65:859f2be748346b2378334d6e5873a9d71d02c9d6584b52cf9297fcb36285ec40`; full pinned public-key SHA256 `0656e6de01e498b50d7b67b10ff58f4c600cdf61663066b03f358a7901839eae`.
- C peer in D's observer: `novovm-mldsa65:f0f7edca9d8c05bda48841860887e5cf2b2a6d8096dc00f18d9af36a0add354f`; full pinned public-key SHA256 `a3760975c77a717ad64e9d18c95f1f6b4d043298b9f359aeaf9fe61687355b2f`.
- Peer IDs were cross-checked with a diagnostic reference of the original domain-separated `peer_id_from_mldsa65_public_key_v1` over full raw keys, not substituted with plain public-key fingerprints. No runtime implementation or configuration was changed.
- A's authenticated replies reflect D at **192.168.71.59:52970** and **192.168.71.59:52971**, matching the two local bind ports. D's observer sees C at **192.168.71.125:59561** for the last session. These are the intended LAN sources, not B/public/Relay endpoints.
- Original pre-execution route records select source192.168.71.59 and physical interface16 for A74 and C125, on-link192.168.71.0/24, next-hop0.0.0.0. These route selections and native observed addresses support the intended LAN path; they do not replace paired reports or a packet-level physical-link audit.
- Config hashes, original NAT/ctl/core files and sealed identity length/UTC last-write ticks still match the approved receipts. No reinstall, generation, clock/VPN/firewall change or production-timeout adjustment occurred. Observer exited normally; a local post-execution query found **0 UDP41050 listeners**, without displacing any process.

All three native reports retain handshake version2, suite65, `network_only=true`, `aoem_called=true`, inner-frame preservation and `ledger_semantics=false`. Each original stdout JSON matches the corresponding native report, and all three stderr files contain zero bytes.

## Concurrent C receipt and remaining failure

C commit `5e5003f3` was published concurrently and merged without replacing either host's original evidence. [C's original results](../validator-C-LAN-results-20261104-083500/README.md) show both C/D probes exited0 with round_trip_confirmed=true. Their peers reverse-match D's observer identities. Sessions are `ba1be5926a32c08cb9357f66e871bde4` and `5114b1a40e9f555c99692ea44591a1e7`; the second session and source `192.168.71.125:59561` exactly match D's last observer record, with request/reply counts2. The first C source is `192.168.71.125:59560`; D's final aggregate has no separate first-session history. This listed correlation does not invent that missing history or replace A's final matrix review.

**C/A is not green:** C/A first exited1, accepted=false, authenticated_exchanges=0, session/observed_endpoint=null; stderr is `Error: native authenticated probe did not complete; retain failure report`. C/A second was correctly not executed. The failed report's expected A peer ID is not proof of authenticated A contact. Root cause remains unknown pending A's original observer timing/report; do not assert firewall, VPN, packet-loss or signature causes, alter parameters or rerun automatically. D/A success and C/D correlation do not erase this independent C/A failure.

## Exact raw evidence for A

The [original-evidence.zip](original-evidence.zip) contains **11 original files**: three native reports, their stdout/stderr, `execution.json` and `routes-before.json`. ZIP size **21914 bytes**, SHA256 **`7f27496cf76a3ce244a9c16606a85ade37ec8149c6d9e70d1279fc2738c7f07c`**. Every entry's length/SHA256 was verified against its untouched source. [RAW-FILES.sha256](RAW-FILES.sha256) and [summary.json](summary.json) record per-file hashes/lengths. The three native reports are also provided as directly readable JSON copies.

Original `execution.json` has20 CRLFs (532 bytes, SHA256 `4b48e376c568baebc752b1eb02b9188ef7a1ed035384d5555333c833c430e6c8`); original `routes-before.json` has10876 CRLFs (328419 bytes, SHA256 `1f600f4bfd5b74045581a20688bd8f72579bad72b10cd9bcbb7b62ff486df2d7`). They stay inside the binary ZIP to preserve raw bytes across Git checkouts, rather than normalizing originals or changing shared Git rules.

The original route JSON-depth8 warning disclosed in ARMED_LAN_D is preserved. Required four source/route records and core fields are readable; nested CIM metadata may be truncated. No evidence was rewritten or original script rerun to conceal that warning. On extraction into a new empty directory, verify the 11 files against RAW-FILES before interpreting them; do not overwrite existing evidence.

Summary Git/LF SHA256: **`e0673e982209c6bab8d1323254c6344ad02d2b8284efef4805c05f34d8dce095`**. Original native reports: [D/A first](probe-D-A-first.report.json), [D/A second](probe-D-A-second.report.json), [D-for-C observer](observer-D-for-C.report.json).

**A:** reconcile A's observer with both D probe reports, review the listed C/D correlation, and inspect A's original C observer for the failed C/A case. Return `received_commit / A_D_reconciled / C_D_reconciled / C_A_first_blocker / opposite_report_refs`. D has received C's original reports but not A's observer originals and does not sign A's acceptance. No four-host, consensus, chain, throughput, complete post-quantum key-agreement or separate stale-session replay gate is promoted. No sealed contents, private key, credential, password or token is published; Git publication is not A acknowledgment.
