# A single-relay sender DONE; C/D correlation pending

At 2026-11-12 09:47 Beijing, the original875 A node prepared one diagnostic
A -> C -> D circuit and sent exactly one fixed non-business object.
The sender helper exited0. No direct-delivery test was repeated.

- Object: `a8536fd1dbd85b4ffa4d3ff8ff7ac4220c2e00d0fa95b62347fdf2ad1db09fb3`
- Session: `1689752becc7449ff129b61be6f6cf95`
- Nonce: `d1c39f293b21ef04831b065ee7e6c34f2e79f600b4b07961a7a05faa7235e3fa`
- Original owner admitted at1794448043327ms and recorded object DONE/Delivered
  at1794448043349ms. This is bounded object custody, NOT a Business Receipt.
- `authenticated-route-before-send.json` records origin A, path C then D,
  downstream readiness, matching native session and no stream drop before send.
  This is the original diagnostic Iroh hop-stream relay with the native object
  channel, NOT evidence that the relay hops used native UDP datagrams. An
  advertised address/relay list does not establish the actual outer IP route.
- C's recovered original875 node PID13020 passed A's authenticated Status query
  at01:40:50Z,36ms, with the original state root. C's observer PID25144 and D's
  observer PID18808 passed the existing readiness gate with their actual receipts.
  D refresh at01:42:39Z confirmed process creation identity and no terminal files.

## Preserved pre-send failure and scoped resolution

The first launcher at09:45 exited1 before prepare/send: command slot occupied.
Read-only inspection found the exact409-byte prior RelayCancel (tag5), last
written2026-11-11T12:26:59Z, SHA256
`6f9e7fd8457b56dab0f2916ddb4bfbbf056522825c43568d3b9e37956490ff1a`.
The original owner reported its matching reservation cancelled, inactive and
send_requested=false. The original file was moved recoverably to local evidence,
not overwritten or deleted. A fresh one-shot helper rechecked those facts and
the existing readiness gate before using the unchanged original sender.
No database, key, node program, service, firewall or protocol limit was changed.
The initial failure is retained in preflight-stderr.log / preflight-exit.json.

## Next action

C/D return ONLY the existing observer outputs: result/error, relevant snapshots,
exit records and stdout/stderr. C must show actual forwarding; D must show final
object integrity/custody for the same object/session. No observer restart, object
resend or node restart. Until those independent records match, single-relay
acceptance remains PENDING. The two C memory faults remain unresolved.

Copied JSON/log evidence is byte-preserved; hashes may be obtained with SHA256.
Large before/after projections contain only the original Save-Public fields.
Raw dumps, private custody and configuration are not included.
