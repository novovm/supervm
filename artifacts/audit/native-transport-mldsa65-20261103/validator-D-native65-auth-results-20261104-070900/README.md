# D scheduled 07:09 original authentication probe results

Node: **validator-D**. Date: 2026-11-04 Asia/Shanghai. **Both original probe processes exited 0 and confirmed one authenticated round trip each. Session IDs differ. B observer reconciliation remains with A.**

The retained foreground terminal armed at **07:02:25.5292076+08:00**. First started at **07:09:00.026677+08:00**, exited 0 at 07:09:00.345351; second then started at **07:09:00.3544414+08:00**, exited 0 at 07:09:00.7458924. Completion was recorded at 07:09:00.7482341. This is the reserved 2026-11-04 07:09 execution, not a catch-up or review-time retry. These are local process timestamps, not handshake-only latency or a performance qualification.

Candidate source remains `d7ef793b819c8670e472d217daae9db5f074aea8`; config ZIP SHA256 remains `99a4dfbd8cf819c9e4286a0aaf0edaf976ae61a8b3ab203a80cb7bad22e9cebd`. No original configuration or report was overwritten.

| Case | Exit | accepted / round_trip_confirmed | Session ID (16-byte hex) | Server-reported observed endpoint |
| --- | --- | --- | --- | --- |
| first | 0 | true / true | `8cb4ea3aae03518b95a1d9adcad27686` | `114.92.30.222:61367` |
| second | 0 | true / true | `7b23789962dc401fd33156c53fd13dfd` | `114.92.30.222:61374` |

Both native reports use `novovm_native_authenticated_endpoint_v2`, handshake version2 and signature suite65; `authenticated_exchanges=1`, `aoem_called=true`, `novorudp_inner_frame_preserved=true`, `novorudp_wire_changed=false`, and `ledger_semantics=false`. Each original stdout JSON matches its saved report; both stderr files are empty. The extra `VoidTaskResult` lines in the waiting shell's console are wrapper output, not the original program stdout or a probe error.

## Identity and input review

- D local peer: `novovm-mldsa65:e4759711dc7c5ded8c2625cc72802e69107d53dfcbca2a69d3cc093764923166`; raw public-key SHA256 remains `30019ff63f5f8c1f77029990aaf2657c3bb6dad562d957d78f0b60d8d349c934`.
- B remote peer: `novovm-mldsa65:a44d68f612d0bfbcce70fe4900383060c385d588103513e6c9a9296bd0dd3ad6`; the full pinned public-key SHA256 is `741cb7b6d0964de5a1a70adf64f1399cacf117f6aae838fd78e1000320b9a676`.
- Both peer IDs match diagnostic reference calculations of the original `peer_id_from_mldsa65_public_key_v1` domain-separated hash over full raw public keys. This audit calculation is not a replacement runtime or a plain-fingerprint-as-peer-id input.
- Both config hashes still match READY_D and the armed record. The original NAT executable, ctl and candidate AOEM match their approved sizes/hashes; sealed identity length and exact UTC last-write ticks match the prior receipt. No new identity, library, clock setting or production timeout was substituted.
- Local binds were `0.0.0.0:61541` and `0.0.0.0:61543`. Observed public endpoints above came from the authenticated responses; this review did not independently retrieve B logs or retest routing/VPN. Do not infer physical direct routing or fresh B-initiated reachability through home NAT.

## Original evidence and preflight history

Original report/stdout/stderr, [armed record](armed.json), [first execution](first.execution.json), [second execution](second.execution.json) and [completion](completion.json) are copied byte-for-byte; source originals remain untouched. Their sizes/hashes are in [summary.json](summary.json).

| Original evidence copy | Bytes | SHA256 |
| --- | --- | --- |
| [first.report.json](first.report.json) | 926 | `05fa5d9fb7f7bb7cd0e3e25fd3ca23e5421a31e71ef3704d1708a987040278bb` |
| [first.stdout.txt](first.stdout.txt) | 927 | `fabbe61f98f529cc79c5d3c1badb23e5d9cd05c062353e3085e93d9a42f2a0ee` |
| [first.stderr.txt](first.stderr.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| [second.report.json](second.report.json) | 922 | `1849dac50981e7c5c468dec084281d9fba1bb898d2e58259e967dcc0c31749e3` |
| [second.stdout.txt](second.stdout.txt) | 923 | `15edc3c2ed70224e5aea75780f1b159e928b44e520912213ec6bdbc95ec68f82` |
| [second.stderr.txt](second.stderr.txt) | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

Before arming, the local waiting helper stopped at 07:01:40 because converting an already parsed PowerShell DateTime back through a string lost timezone/fractional precision. Independent exact UTC ticks and file length matched the original sealed metadata. Only that timestamp reader was corrected; no identity or security check was relaxed. [The stopped preflight](preflight-stopped.json) is retained, and no probe ran in that preflight. It is not a network failure or an overwritten authentication result.

Summary Git/LF SHA256: `279acdeb02bf92a16eede91f20825d86970298549f20f25d9c5fa011041093e3`.

**A:** please reconcile both D local/remote peer IDs, session IDs and observed endpoints with the corresponding original B observation report, then return `received_commit / D_B_reconciled / B_report_refs / first_blocker`. D has not retrieved B's observer report and does not sign that comparison for A. Two successful new sessions do not prove the old-session packet replay counterexample, four-host/LAN acceptance, B-initiated NAT traversal, TPS or post-quantum key agreement. No validator, chain, database, scheduled task or service was started/created, and no sealed identity, private key, credential, password or token is published. Git publication is not A's acknowledgment.
