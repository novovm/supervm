# C LAN scheduled results: preserve C/A failure and C/D successes

Node **validator-C**, 2026-11-04 Asia/Shanghai. The original one-shot script exited **1 overall**: **08:35 C/A first failed; its second was not executed. The independent 08:41 C/D first and second both succeeded with different sessions.** No review-time probe, catch-up, retry, repair or configuration change was performed.

The original source/program pins remain schedule53fe7da3, programd7ef793b and the previously qualified AOEM. Actual script SHA `adc87b9b2836776694c43062525951906118fbfaa9f5aaa192e69abcaf78df07` still matches ARMED_LAN_C. All four C config hashes and actual NAT/ctl/core hashes still match the qualified receipts. Source original reports and outputs remain untouched.

| Original case | Exit | Actual local start (+08) | accepted / round_trip_confirmed | Session ID (16-byte hex) | Authenticated reported source |
| --- | --- | --- | --- | --- | --- |
| probe-C-A-first | 1 | 2026-11-04T08:35:00.802641+08:00 | false / false | `NONE` | `UNKNOWN` |
| probe-C-D-first | 0 | 2026-11-04T08:41:00.862068+08:00 | true / true | `ba1be5926a32c08cb9357f66e871bde4` | `192.168.71.125:59560` |
| probe-C-D-second | 0 | 2026-11-04T08:41:01.059542+08:00 | true / true | `5114b1a40e9f555c99692ea44591a1e7` | `192.168.71.125:59561` |
| probe-C-A-second | NOT RUN | NOT RUN | N/A | NONE | UNKNOWN |

## 08:35 C/A failure

C/A first ran08:35:00.8026418 to08:35:11.3044114 (+08); exit1. Original failure report says accepted=false, round_trip_confirmed=false, authenticated_exchanges=0, session_id=null and observed_endpoint=null. stdout is empty; original stderr is exactly `Error: native authenticated probe did not complete; retain failure report` plus CRLF. The original script correctly skipped C/A second: no second execution entry, report, stdout or stderr exists. No fabricated second exit code is assigned.

The configured peer ID matches A's pinned public key, but in this failed report it is the **expected** identity, not proof A was authenticated. Local bind0.0.0.0:65044 is a wildcard binding, not an actual observed source address. Pre-run routes select source192.168.71.125, but C has no authenticated A observation and cannot claim which source A received, whether A received the packet, or why the exchange failed. Timing, firewall, VPN, packet loss and signature explanations remain unproven. A should inspect its already captured original observer timing/report rather than asking C to alter parameters or rerun automatically.

## 08:41 C/D successes and LAN address

Both original C/D reports say accepted=true, round_trip_confirmed=true and authenticated_exchanges=1; signature suite65/handshake version2, original frame preserved, AOEM called, ledger_semantics=false. Their stdout JSON matches each native report; stderr is empty. C's local peer ID matches its original full public key; the remote peer matches D's published full key. Both session IDs are16 bytes and differ.

The authenticated responses report **192.168.71.125:59560** and **192.168.71.125:59561**, equal to C's local source ports and expected LAN source IP. Configured destination is192.168.71.59:41050. The original pre-run routes for A and D both select C Ethernet/interface24, prefix192.168.71.0/24, next-hop0.0.0.0; these route lookups do not themselves prove UDP delivery. The original routes-before JSON depth8 warning is retained, not suppressed; required top-level source/interface/prefix/next-hop fields remain readable. A must still match D's original observer report to C's peer/session/source values before signing paired LAN qualification.

## Unmodified original evidence for A

The following files are copied **byte-for-byte**, including the failed C/A report, empty outputs, CRLF stderr and328367-byte original routes-before.json. A report-local `.gitattributes` preserves only this directory's raw bytes; it does not change network/runtime configuration. The earlier B tests and historical failed attempts are not overwritten.

| Original file | Bytes | SHA256 |
| --- | --- | --- |
| [raw/execution.json](raw/execution.json) | 531 | `6655e30db0a17f1d72f9caa2727c88766407bf2a18142df09c8564c1c7cbe9e5` |
| [raw/routes-before.json](raw/routes-before.json) | 328367 | `61df75af24743e56691fc6c8aa9f8cca5cb2f901217df99db4046517e8cdb563` |
| [raw/probe-C-A-first.json](raw/probe-C-A-first.json) | 771 | `4a95c3b9e128efb6e102a64c587f0bb0d0605e606d10f917344d567ffed9a695` |
| [raw/probe-C-A-first.stdout.txt](raw/probe-C-A-first.stdout.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| [raw/probe-C-A-first.stderr.txt](raw/probe-C-A-first.stderr.txt) | 75 | `96073c5161980397eddbc6c834ef45f3d5874513b99c7d0d32ad60e245eee18e` |
| [raw/probe-C-D-first.json](raw/probe-C-D-first.json) | 928 | `41d799afa4e52bebe73efa8bcf0d526e7911c02463135ed3f2b66d61f1a7f81a` |
| [raw/probe-C-D-first.stdout.txt](raw/probe-C-D-first.stdout.txt) | 929 | `aef72e7e20cd23148cdce65a20b40b7930d0d664feee093ca28aa24a0e4901ed` |
| [raw/probe-C-D-first.stderr.txt](raw/probe-C-D-first.stderr.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| [raw/probe-C-D-second.json](raw/probe-C-D-second.json) | 924 | `64174224164df470653ce9749b0bbfb5271a10d0ca8d029ecb491235e8a159ae` |
| [raw/probe-C-D-second.stdout.txt](raw/probe-C-D-second.stdout.txt) | 925 | `c544d50da514b9cedce7e755a14cb8116cc106555d725553def05d3c184c75af` |
| [raw/probe-C-D-second.stderr.txt](raw/probe-C-D-second.stderr.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

[Structured summary](summary.json), LF SHA256 `a8970c53505ebdc2b8c47da181152734c56e14360ebb054bb1382d11b1fac578`.

A please return `received_commit / C_A_original_observer_refs / C_A_first_blocker / C_D_paired_reconciled / D_observer_refs`. C does not sign A/D observers, assign a root cause to the C/A timeout, or mark all LAN/four-host pairs accepted. No private key, sealed identity or credential is uploaded; no validator startup, genesis, balances or business TPS result is claimed.
