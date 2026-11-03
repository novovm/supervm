# C scheduled 07:06 original authentication probe results

Node: **validator-C**. Date: 2026-11-04 Asia/Shanghai. **Both original probe processes exited 0 and confirmed one authenticated round trip each. Session IDs differ. B observer reconciliation remains with A.**

The retained terminal was armed at 2026-11-04T07:00:49.141756+08:00; it actually started at **2026-11-04T07:06:00.000546+08:00**, ran first then second sequentially, and finished at 2026-11-04T07:06:00.452726+08:00. No catch-up, retry, config alteration or additional probe was run during this review. Candidate program remains d7ef793b; unchanged config ZIP pin is `99a4dfbd8cf819c9e4286a0aaf0edaf976ae61a8b3ab203a80cb7bad22e9cebd`.

| Case | Exit | accepted / round_trip_confirmed | Session ID (16-byte hex) | Server-reported observed endpoint |
| --- | --- | --- | --- | --- |
| first | 0 | true / true | `5a7211a1bbf2271f6240c82e8fd29b3b` | `114.92.30.222:59072` |
| second | 0 | true / true | `a87cf0ded56caee98acb6e1b16b4f417` | `114.92.30.222:59088` |

Both reports use scope `novovm_native_authenticated_endpoint_v2`, mode probe, handshake version2, signature suite65; `authenticated_exchanges=1`, `aoem_called=true`, original inner frame preserved and `novorudp_wire_changed=false`. Ledger semantics remain false. The two stdout JSON objects match their respective original report objects; both stderr files are empty.

## Identity and input checks

- C local peer: `novovm-mldsa65:f0f7edca9d8c05bda48841860887e5cf2b2a6d8096dc00f18d9af36a0add354f`. Matches the original peer-id derivation from C's full approved public key; C public-key SHA256 remains `a3760975c77a717ad64e9d18c95f1f6b4d043298b9f359aeaf9fe61687355b2f`.
- B remote peer: `novovm-mldsa65:a44d68f612d0bfbcce70fe4900383060c385d588103513e6c9a9296bd0dd3ad6`. Matches the original derivation from the full pinned B public key; plain public-key SHA256 is `741cb7b6d0964de5a1a70adf64f1399cacf117f6aae838fd78e1000320b9a676`. Peer ID is not the plain public-key fingerprint.
- Both C config files retain their READY_C hashes. NAT executable, ctl and candidate AOEM retain their approved hashes. No identity generation or replacement was performed.
- Local binds were `0.0.0.0:54636` and `0.0.0.0:54638`. The observed public endpoints are values in the authenticated responses, not an independently read B log or proof that B can initiate a fresh connection through C's home NAT.

## Verbatim evidence for A

Original report/stdout/stderr and retained-terminal execution record are copied byte-for-byte below. No sealed identity, private key or wrapping credential is included. Source originals remain untouched. The earlier uncoordinated 06:52 timeout and its report are preserved separately in their original directory and are not replaced by this successful scheduled run.

| Original evidence copy | Bytes | SHA256 |
| --- | --- | --- |
| [first.report.json](first.report.json) | 922 | `83b5154193d521503f6ab856f306589a81908a2eed80e2dc8b32b808cdaa8ee5` |
| [first.stdout.txt](first.stdout.txt) | 923 | `ffcb86df249e9cc1e849bf0b99b42896aac7a43cb975c3e03d7d11a1abfeddba` |
| [first.stderr.txt](first.stderr.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| [second.report.json](second.report.json) | 927 | `bb1b914ab8e6fdc04314781fe5fb63f914a23fd300b8168dc1ac8ae93c6a4e05` |
| [second.stdout.txt](second.stdout.txt) | 928 | `9dad9dc0a4602a980945756d2dd27b79e11ce65afd0d062f0f93c5936b6f448b` |
| [second.stderr.txt](second.stderr.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| [execution.json](execution.json) | 1717 | `f804c3648bb1cfd4beeaaa270da3dd5f1933bb01c6c83da81b447e3f7780b01b` |

[Machine-readable reconciliation summary](summary.json), LF SHA256 `ca5f3907999df9fed12b659bb1e6e9d12c72793a38a76c21472b944d768a83d3`.

A: please reconcile C's two local/remote peer IDs, both session IDs and both observed endpoints with the corresponding original B observation report, then return `received_commit / C_B_reconciled / B_report_refs / first_blocker`. C has not retrieved B's observer reports and does not sign that comparison on A's behalf. User-reported A/B success, D's separate window, LAN pairings, four-host acceptance and an actual old-session packet replay counterexample remain separate. Two new sessions do not prove the replay counterexample. No validator startup, genesis, balances, ledger execution, throughput or post-quantum key-agreement qualification is claimed.
