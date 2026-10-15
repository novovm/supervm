# Original July structural consumer: bounded extraction checkpoint

Date: 2026-10-16 (Asia/Shanghai). This is NOT product finality or release acceptance.

## Source and scope

Implementation is committed in the detached, repository-local July recovery
worktree at `9797a346`, parent `4fa89d80`. It is not activated on shared main.
No new branch, controller, signer, execution owner or database was introduced.
Shared main publishes this evidence while the full product migration continues.

The original gate now calls the shared bounded structural receiver. The original
UdpTransport can explicitly return untrusted native frames from its same socket;
legacy protocol handling remains separate. Actual node lifecycle wiring is NOT
completed. Raw frames/checksums/session IDs do not authenticate peers. Production
must reuse the existing handshake, AEAD, session generation and object reassembly
before structured business admission; no second socket poller or authentication
state machine is authorized by this checkpoint.

Signed APFL V3 tests consume existing shared-column views and the existing strict
wallet verifier, without rebuilding raw transactions. They prove authentication,
not current nonce/funds, economic execution, durable admission or voting authority.

Changes: bounded session fingerprints and ordered 128-payload delivery; conflicting
retransmissions rejected; payload allocations moved, not cloned. Constructor
requires enough bytes for every configured buffer slot at maximum payload size,
preventing small-current/large-future payloads from blocking next-bulk repairs.
Rejected gate frames no longer change the ACK destination. The frontier missing
count changes only on accepted new members; its next range is scanned once after
drain, replacing a repeated up-to-128-position scan for each new packet.

## Verification

Windows debug, locked dependencies, sequential build ownership:

- `cargo test --locked -p novovm-node --lib native_structural -- --test-threads=1`:
  final 8 passed, including real original-socket delivery, ordered/repaired frames,
  wrong sessions, conflicting payloads, invalid wallet signature, budget overflow,
  cross-bulk repair space and independently recomputed frontier-count invariants.
- `cargo test --locked -p novovm-network --lib udp_ -- --test-threads=1`:
  28 passed (including existing protocol behavior and three new dispatch tests).
- Original gate's six unit tests passed after extraction/ACK correction and before
  the final frontier optimization; the final receiver had the eight tests above.
- Final release gate built successfully. Existing dead-code/unused-import and
  duplicate-bin warnings remain; this is not strict-Clippy or remote-CI acceptance.
- Earlier integration-test compilation failed (incorrect Vec `.unwrap()`, then
  debug-link/PDB errors and disk-full). The fixed socket test is now a lib test,
  avoiding unnecessary builds of every binary; no failed result was called PASS.

## Same-condition finite-load observations

Windows loopback, externally powered (BatteryStatus 2, charge 100), no competing
cargo/rustc/link processes during runs. Each run: 4,800 original APFL payloads,
128 rows/payload = 614,400 rows, bulk 128, original pacing/ACK settings and original
core SDK, persistence backend `none`. Each retained run passed transport and AOEM
counts with 38 successful opcode-114 routes, 76 hot plans, 614,438 writes, no
canonical materialization and no reported decode/execution errors.

| Run | Transport ms | AOEM execution ms | Receiver total ms | First-packet ms | Combined ms |
|---|---:|---:|---:|---:|---:|
| baseline-1 | 400 | 3075 | 4049 | 510 | 3539 |
| candidate-1 (repeated scan) | 729 | 3080 | 4376 | 498 | 3878 |
| baseline-2 | 505 | 3061 | 4129 | 497 | 3632 |
| candidate-2 (repeated scan) | 621 | 3069 | 4260 | 507 | 3753 |
| baseline-3 | 732 | 3050 | 4360 | 514 | 3846 |
| final-1 (frontier count) | 628 | 3066 | 4272 | 512 | 3760 |
| baseline-4 | 509 | 3092 | 4157 | 497 | 3660 |
| final-2 (frontier count) | 630 | 3052 | 4259 | 513 | 3746 |

Aggregate rows / summed combined time, not average TPS. As in the original
reproduction, combined = receiver_elapsed_ms - receiver_first_packet_ms:

- Initial baseline pair: 171,356.85 rows/s; initial candidate: 161,027.39 (-6.03%).
- Adjacent final baseline pair: 163,709.03; final candidate: 163,709.03 (equal
  summed millisecond durations, not proof of identical underlying latency).
- All four baseline runs: 167,445.66 rows/s. The final pair remains below this
  broader baseline; do not select only the favorable comparison or claim stable
  throughput improvement. Variation and other receive costs remain unassigned.

`receiver_elapsed_ms` starts at receiver-loop initialization before the first
packet, and ends after execution before report construction/output. Subtract its
recorded first-packet timestamp to keep the original comparison boundary.
An initial interactive update mistakenly used the raw receiver total, reporting
150,256.79 -> 142,288.10 and 144,276.15 -> 144,039.39. Those include startup wait;
they are retained here for transparency but are NOT the primary comparison.
The combined measurement is not report durability, pure GPU time or a transaction
latency distribution. Historical reports' transport-derived effective
business TPS must not be relabeled as this combined timing or finalized TPS.
These fixture rows use `preserve_original_signature_no_crypto_v0`, not the true
wallet-signature path tested separately. No BFT, RocksDB durability, per-transaction
P95/P99, physical multi-machine or production capacity was measured here.

Frozen EXE SHA256:

- baseline: `1EAD93EDE803EF22FD14BA851DAE7B0585684EC2AB0DD9699AC318B9A43D0836`
- initial candidate: `5B0B6A7709047C95660A9B4678D1454581B2AE22A7AB868C150984D956EE1EA2`
- final candidate: `CF7DFBC3A6ADEBC77BB348CA32209351F98B9397DE3346FA667062875AE097F4`
- unchanged AOEM DLL: `2B9A91AE7D7D8A518D9EE2363EA1B402AE48AB3509E51B424C3C564F61BF730E`

All eight original JSON/log sets are retained beneath this directory. Metadata's
supplied checkout is not proof of frozen-binary provenance; use these explicit
binary identities. The baseline predates extraction, initial candidate includes
the repeated scan, final candidate corresponds to recovery `9797a346`.

## Remaining integration

Do not expand the untrusted raw branch into a production admission bypass. Reuse
NativeHandshake/NativeUdpSession and NativeObjectReceiver through the original
node's sole socket owner; preserve complete-object boundaries (encrypted datagrams
are not whole APFL batches), peer/session budgets, replay and replacement rules.
The NOVOCHAT machine currently owns endpoint/datagram wire-budget work; integrate
its reviewed changes rather than overwriting or duplicating the shared SDK.
Then join complete structured inputs to the existing full AOEM semantic/economic
plan and unique publication/recovery path. This checkpoint does not complete that
handoff or reauthorize the resident bootstrap early-return replacement.
