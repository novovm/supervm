# C 09:30 A/C recheck: two successful original authenticated round trips

Node **validator-C**, 2026-11-04 Asia/Shanghai. The original one-shot packaged script and both probe processes exited **0**. **First and second each confirmed an authenticated round trip, with different16-byte session IDs and the expected LAN source192.168.71.125.** A still needs to reconcile its original observer report; C does not sign that second side on A's behalf.

| Original case | Exit | Actual local start (+08) | accepted / round_trip_confirmed | Session ID hex | Server-reported observed endpoint |
| --- | --- | --- | --- | --- | --- |
| first | 0 | 2026-11-04T09:30:00.522086+08:00 | true / true | `cb91145344f0b50838e916d9b5841c4c` | `192.168.71.125:52748` |
| second | 0 | 2026-11-04T09:30:00.902586+08:00 | true / true | `926ca66c7e1c31f248a3fb7340c0e4e5` | `192.168.71.125:58936` |

First ended 2026-11-04T09:30:00.898904+08:00; second ended 2026-11-04T09:30:01.153363+08:00. They ran sequentially once under the09:30 reservation, using the frozen d7ef793b NAT executable, original identity, full A public-key pin and unchanged10-second bounds. No review-time probe, catch-up, retry or additional invocation was performed.

## Evidence checks

Both native reports use `novovm_native_authenticated_endpoint_v2`, mode probe, handshake version2 and signature suite65, authenticated_exchanges=1, AOEM called, original inner frame preserved, wire_changed=false and ledger_semantics=false. Both stdout JSON objects equal their respective original native report objects; both stderr files are empty.

C local peer ID is `novovm-mldsa65:f0f7edca9d8c05bda48841860887e5cf2b2a6d8096dc00f18d9af36a0add354f`, matching the original public key whose SHA256 is `a3760975c77a717ad64e9d18c95f1f6b4d043298b9f359aeaf9fe61687355b2f`. A peer ID is `novovm-mldsa65:859f2be748346b2378334d6e5873a9d71d02c9d6584b52cf9297fcb36285ec40`, matching the full pinned A key whose SHA256 is `0656e6de01e498b50d7b67b10ff58f4c600cdf61663066b03f358a7901839eae`. Peer IDs were checked against the original domain-separated derivation, not equated with plain key fingerprints.

The authenticated responses report192.168.71.125:52748 and192.168.71.125:58936, and each source port equals the corresponding local bind port. Original routes-before selects source192.168.71.125/Ethernet interface24 to192.168.71.74, on-link192.168.71.0/24 via0.0.0.0. These agree with the planned LAN path; route lookup alone is not packet evidence. A must match observer local/remote peer, sessions and actual source observations before final paired acceptance.

Packaged script SHA, new ZIP SHA, both recheck config SHAs and actual NAT/ctl/core SHAs still match the armed/qualified inputs. The new configs differ from08:35 only in report_path. All11 source hashes from the earlier08:35 report remain unchanged, including its failed C/A first and absent second. That failure is not erased or relabeled as unexecuted. The already-successful D/B and other pairings were not rerun. A's prior firewall-event explanation remains A's separately documented evidence, not a claim that C modified rules or proved every packet's original drop cause.

## Verbatim original files for A

All raw files below are copied byte-for-byte, source originals retained. A report-local `.gitattributes` prevents CRLF normalization of raw evidence and changes no runtime configuration.

| Original file | Bytes | SHA256 |
| --- | --- | --- |
| [raw/execution.json](raw/execution.json) | 336 | `4bdaa4b768ef561c0f20aeeeecdd0ba97503772bd60a7db0ad6f209c01fc606a` |
| [raw/routes-before.json](raw/routes-before.json) | 337 | `136b60d8c69e195ab21f1d932f34e3b001946de6b995517744e7ca984b3d9f2e` |
| [raw/first.report.json](raw/first.report.json) | 922 | `03d904e7df6c50ae504cbf3372c216672710119a7c96913de8b199e1671f75e7` |
| [raw/first.stdout.txt](raw/first.stdout.txt) | 923 | `cb5c9a174f613e7ed8788b7b6c014f747422bb10edb36eac28b68f2bdef2d1a2` |
| [raw/first.stderr.txt](raw/first.stderr.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| [raw/second.report.json](raw/second.report.json) | 927 | `de1cb751a2662e40d34bb8d4404cb22450195c7369efe528c6e21dc982bfdff0` |
| [raw/second.stdout.txt](raw/second.stdout.txt) | 928 | `acb495c87c15bcf34e67c11e268496cb9f6d69411f78c12369d69d0848b2d068` |
| [raw/second.stderr.txt](raw/second.stderr.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

[Structured summary](summary.json), LF SHA256 `f54738374859fc4555b55e8f48f5cd933ea5357bee574beaf3b339f1d711fb0e`.

A please return `received_commit / A_observer_original_refs / CA_peer_session_source_match / final_matrix_status`. This client result does not by itself certify every directed four-host path, inbound NAT initiation, consensus, genesis, NOV transfer, throughput or post-quantum key agreement. No sealed identity, private key, credential, password or token is uploaded. No program, identity, authentication code, timeout, firewall or configuration was changed during review.
