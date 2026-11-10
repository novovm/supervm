# Native first-handshake session clock — 2026-11-10

## Current cutover status: expired time-queue defect, not downtime permission

### Fixed replacement 8fed4b76 (2026-11-10 16:00 CST)

Fixed program source is `8fed4b76f518935b30fcfb13930ccd3d7aae7381`.
Both Windows/Linux original source-process recovery suites passed **6/6**.
Both fixed packages then independently passed identity/NAT **7/7**, original
owner component I/O **15/15**, actual packaged-node recovery **6/6**, packaged
UCA recovery **1/1**, and startup/runtime **1/1**. Every archive member was
verified independently, including inherited AOEM checksums. These are local
qualifications, not four-host acceptance.

[Fixed manifest](candidate-8fed4b76/candidate-manifest.json) binds all program,
client, library and archive hashes. Windows archive: **150603521 bytes**,
`29bee97601a7fb8ba698bf8bb5bff356e1c97c8e8d2a750be871f3c928fe73e4`;
Linux archive: **359039913 bytes**,
`c279cc1a4860f14e2142e0fa6bbf245d9f0f48f1e05358a9bd3efae7efa9953a`.
Local directory:
`C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-8fed4b76-20261110`.

One exact-source Full **42776380753**, attempt 1, was dispatched after local
validation. At this snapshot native recovery, security and both actual-runtime
platform jobs passed; Rust checks and the aggregate result were still pending.
The creation-time candidate manifest retains its earlier CI snapshot, not a
claim of final success. Do not dispatch another Full or reuse failed e7 output.

Original updater adaptations passed Windows ten gate groups (including actual
package hashes) and Linux thirteen tests. The first generated Windows test
incorrectly retained historical 082/5181 expected pins; it failed before any
deployment. The generator now derives current 1ef pins from its verified
manifest. The updater itself and its rejection checks were not weakened.
Raw evidence and the old generated test are retained in `updater-evidence.zip`.
The process helper's canonical LF digest still matches its original fixed pin.

The new fixed operator has an exact-scope clone of its old UDP reply rule;
firewall profiles are unchanged. A/B pre-update authenticated Status returned
the same original root (17ms/51ms). B archive upload is in progress; no installed
node has changed. After Full qualification proceed directly B -> A -> C -> D
with the [existing update plan](candidate-8fed4b76/UPGRADE.md), without another
stop-permission request. C/D package transfer instructions are in
[CD-UPDATE.md](candidate-8fed4b76/CD-UPDATE.md).

### Prior failed candidate and the correction

The user explicitly authorizes direct stop/cutover of this unused internal
development system. Downtime is not a deployment blocker and no repeated
stop permission is required. No installed node has been changed in this round.

The fixed `e7b0cca7` Windows/Linux archives passed their local package checks
(identity 7, owner I/O 15, original-node recovery 6, UCA recovery 1 and startup
qualification 1 per platform). The Linux archive was uploaded to B, and its
SHA256 matches `e531af4b7ddcbed564e98d4114e5ca66f8640c97dc0af8b445e74b634d905e97`.
Staging is not installation. The generated candidate manifest retains its
creation-time CI snapshot; this section supersedes that snapshot for acceptance.

Full run **42770392703**, attempt 1, is bound to exact source
`e7b0cca7fb823a7b15e8788090052b17ddfd2454`. Its native recovery job
**129676752806 failed**: 4 cases passed, late-highest and late-lowest failed.
Both retained node stderr files contain `original tick stopped; drain=Ok(())`
caused by `native send operation deadline`. The remaining CI jobs do not
override this failure. This candidate is not qualified for cutover.

The original lifecycle queued time measurements with a two-second monotonic
deadline, but retired them only in assembly `poll_inner`. I/O is deliberately
independent of assembly and could attempt an expired, unadmitted time object
first. `NativeObjectLink::try_send_until` correctly rejected that operation;
the error propagated through `poll_io` and stopped the original node. This is
not a RocksDB opening, Finished authentication or stop-permission failure.

The minimal correction reuses that same time-flight retirement in
`accept_io_report`, before send admission, including carrier replacement.
It does not require a wall-clock capture. Unadmitted stale objects are released;
admitted objects retain custody/accounting until the original link returns a
terminal event. Assembly still retires stale sessions before creating samples.
Consensus deliveries, TTLs, authentication and resource budgets are unchanged.

A deterministic original-lifecycle counterexample uses an already-expired
queued measurement, without sleeping or changing timeouts. On the old Linux
owner it reproduces exactly `native send operation deadline`. An earlier
Windows ordering variant also showed the expired flight remained queued behind
consensus traffic. The final regression additionally checks fresh admission,
terminal expiry/accounting and preservation of the queued consensus object.
The existing recovery CI step explicitly lists and runs this regression before
the unchanged six process cases. Correction validation: **44/44 lifecycle tests
on Windows and 44/44 on Linux passed**, including real AOEM and UDP component
owners, skew/jump/replay, object DONE/cancel, recovery queues and the new expiry
counterexample. Strict node lib/tests Clippy (`iroh-transport`, release) passed
on both platforms; 27 CI-scope tests, format and diff checks passed. Subsequent
source/package recovery results and the one replacement Full are recorded above.
This does not rewrite the failed e7 run or close unrelated historical stalls.

Raw cloud failure evidence: `ci-native-recovery-e7b0cca7.zip`, 313841 bytes,
SHA256 `03c9a93a40f577adab873b0aea31226563c883c146b1b8d09efc5875e0b7c2e5`.
Local old/new output and reproduction script: `time-expiry-local-evidence.zip`.
The first Windows test invocation used a mistyped DLL path and failed before
the test; the corrected executable invocation produced the retained old-owner
counterexample. An initial WSL runner omitted Cargo from PATH and ran no test;
the retained `old-repro` run uses the login environment and reproduces the
actual deadline error. Neither setup failure is counted as a product result.

Source base: `b4f22132`. The containing commit implements the next step after
the post-session relative-time work (`c87855ac`); it is not an installed version
or four-machine acceptance. No OS clock, A/B/C/D process, database, identity,
discovery cache, production timeout or resource limit was changed.

## Original owners and protocol contract

`NativeEndpoint -> NativeHandshake -> NodeHandshakeInitiatorV1/ResponderV1`
continues to own native authentication. Original ML-DSA signing/verification,
ephemeral ECDH, HKDF, AEAD, continuation cookies, replay cache and wire budgets
are reused. There is no additional socket, discovery service, timer service or
transport fallback. NovoRUDP priority and Iroh fallback are unchanged.

Native handshakes explicitly require signed profile **3**, with a separate
signature domain `novovm-native-session-clock-mldsa65-v3`. The binary offer and
response keep the existing fixed field layout: in this new profile only,
`issued_at_ms=0` is the relative origin and `expires_at_ms` is the signed attempt
duration (1..60000 ms). These fields are not UTC credentials. Existing public
profile-2 validators still require profile 2 and retain their wall-time checks;
changing an old signature's version/time fields cannot authenticate profile 3.
There is no downgrade negotiation or silent native profile-2 retry.

Native authentication now requires Offer, Response, initiator Finished and
responder Finished. Finished binds the complete signed transcript, session and
role, and is protected with the original derived channel. The two Finished
frames consume sequence zero in their respective directions; application frames
continue at sequence one, without resetting replay state. The existing reliable
Carrier and native paths share the Finished frame constructor but use distinct
domains/transcripts. **UDP does not invent a TLS exporter or a
`VerifiedTransportBindingV1`**: the reliable-carrier SessionV2 contract cannot be
passed to a bare datagram owner as if it were already bound.

The native Finished outer discriminator is `NVHSHK03`; offer/response still use
the original binary continuation envelope. Finished is one bounded datagram,
retries reuse identical ciphertext, and its reserved stream cannot be submitted
or delivered as application data. No channel is exposed until the peer's
Finished verifies **and** the local Finished was actually submitted successfully.
Preparing or queueing it is not sending it. Transport completion still is not
Object DONE or Business Receipt.

The original immutable local `Instant` attempt deadline governs all flights.
Peer duration can shorten, never extend it. Wall-time jumps do not change it.
Duplicate offers cannot accelerate retries or reset deadlines. Old signed
offers replayed after bounded replay-cache eviction cause a fresh responder
secret; old Finished cannot activate that channel. Unconfirmed first contacts
expire without pinning listener slots. Existing reconnect/retirement/route
learning still require the original allowed-stream AEAD confirmation before
replacing a live session or route.

This proof authenticates the pinned transport identities and fresh session,
not UTC, chain finality or asset authority. It does not add a new chain-domain
field to the original transport transcript. The existing higher-layer
chain/genesis/protocol binding remains necessary; post-session time challenges
retain that binding. Original ECDH is unchanged: ML-DSA authentication does not
establish post-quantum key agreement.

## Compatibility and next deployment boundary

Both native endpoints and affected Node/Client consumers need the same profile.
Installed `1ef31893` native clients/nodes are not compatible with this first
handshake profile; mixed-version native authentication fails closed. Existing
reliable-carrier profiles remain separately validated, not an automatic bypass.
A future fixed candidate and coordinated update plan must include those
consumers. This change does not install that candidate.

After authenticated establishment, the existing relative-time owner can measure
peer offsets before validating non-business probes. A truthful peer clock offset
does not require changing either OS clock. This is **not** a global trustworthy
UTC estimate. Consistently dishonest samples, cross-platform suspend detection,
time-measurement flood/remeasurement storms, real C clock tests and migration of
discovery leases/chat consumers retain the boundaries in handoff section 36.1.
Asset authorization and consensus are not consumers of this peer-time estimate.

## Local evidence

Raw outputs, including intermediate failures: [logs.zip](logs.zip). Network
unit fixtures use the repository's branded component signing fixtures, not an
AOEM ML-DSA implementation. Original UDP/QUIC sockets are real loopback sockets;
injected wall-clock values are not actual A/B/C/D clocks. Neither is public
network acceptance. The separately identified runtime executable test, when
run, uses actual AOEM and protected temporary identities.

New regressions cover first contact with opposite extreme clock values and
wall jumps, both Finished directions losing a packet, tampering, old-proof
replay with a fresh replay cache, duplicate floods, unchanged deadlines,
expired/unsent completion, profile downgrade/retagging and expired listener
capacity. Original native objects, reconnect, relay, Carrier, discovery,
relative-time and node lifecycle/recovery regressions are reused.

| Check | Windows | Linux |
| --- | --- | --- |
| Original native UDP / endpoint / objects | 213 passed | 210 passed |
| Overlay signature/channel and binary codec | 31 passed, 2 performance tests ignored | 31 passed, 2 performance tests ignored |
| Bound transport / Finished | 45 passed | 45 passed |
| Bound sessions / actual loopback QUIC | 33 passed | 33 passed |
| Relative-time owner | 21 passed | 21 passed |
| Original discovery RPC | 16 passed | 16 passed |
| Original node lifecycle / archive recovery, actual AOEM | 43 passed | 43 passed |
| Original node structured codec | 35 passed | 35 passed |
| Actual protected CLI + NAT executable authentication / same-port reconnect | 1 passed | 1 passed |
| Strict Clippy, network/node lib/tests | passed | passed |
| Node / native-client entry compilation | passed | passed |

The six network groups total 359 Windows / 356 Linux passed tests. The two
ignored tests are explicitly release-only isolated performance benchmarks; they
were not executed and are not counted as passes or throughput evidence.
Node tests are 78 additional tests per platform. The five probe-owner tests
inside the 43-test group include the existing clock-skew/jump/replay integration,
not five extra independent tests.

Reproduction uses `--locked`, `iroh-transport`, two build workers and existing
isolated C:/WSL build targets, without rebuilding on any deployed host:

```text
cargo test --locked -p novovm-network --lib --features iroh-transport --no-run
# Run that exact Cargo-reported executable (Linux uses --release) with filters:
novorudp:: product_overlay:: transport_binding:: bound_session:: network_time:: discovery_control:: --test-threads=1
cargo test --release --locked -p novovm-node --lib --features iroh-transport native_lifecycle:: -- --include-ignored --test-threads=1
# Same node executable: consensus::transport:: --include-ignored --test-threads=1
cargo clippy --locked -p novovm-network -p novovm-node --lib --tests --features iroh-transport -- -D warnings
cargo check --locked -p novovm-node --features iroh-transport --bin novovm-node --bin novovm-native-client
```

Linux Clippy/entry checks use `--release`. Node/AOEM tests explicitly set
`AOEM_DLL_MANIFEST_REQUIRED=1`, `AOEM_SEMANTIC_GRAPH_V2_WORKERS=2`,
`NOVOVM_AOEM_TEST_LIBRARY` and `AOEM_DLL_MANIFEST` to the retained 1ef AOEM package.
Windows AOEM SHA256 is
`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`;
Linux is `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33`.
These are test libraries, not evidence that installed nodes acquired the new
handshake. The user has now authorized uniform four-node/client upgrading;
fixed candidates, their required gate and actual updates remain separate work.

The actual-executable test is
`novovmctl --test native_identity_cli actual_protected_nat_executables_authenticate_and_reconnect_on_the_same_udp_port`
with `--include-ignored --test-threads=1 --nocapture`. Both the current ctl and
NAT executable were freshly built (`--release --locked`; node feature
`iroh-transport`). Temporary protected identities are generated by the original
CLI, with actual AOEM signatures, then two independent processes reconnect to
the same UDP port. The unreachable counterexample exits 1 as required. Windows
16.05 seconds / Linux 27.23 seconds are test durations, not latency benchmarks.
Raw executable hashes are in `windows-actual-nat.txt` / `linux-actual-nat.txt`.
These temporary local processes are not the installed four nodes or a fixed
release archive. Total distinct passed tests: **438 Windows / 435 Linux**.

## Development failures (not rewritten as passes)

- Initial build exposed the shared Carrier `FINISHED_STREAM` constant being
  incorrectly gated to tests during extraction. Its production validation still
  uses it; the constant remains available in production.
- Intermediate native runs retained fixtures that expected completion directly
  after Response. Another deferred-I/O fixture recognized only `NVHSHK02` and
  incorrectly counted Finished as a business write. The fixtures now observe
  actual Finished and classify both native handshake discriminators. Original
  delivery assertions and deadlines are retained.
- The binary codec's unknown-version counterexample used 3. Since 3 is the
  explicitly supported new profile, it now rejects unknown version 4 instead;
  a separate native test rejects a legacy signature retagged as profile 3.
- A Linux shell loop lost its variable expansion at the Windows/WSL boundary
  and selected `::` rather than the intended groups. The exact local test
  process and loop were stopped; the partial output is not a completed test
  result. The corrected invocation passes all six filters literally in one
  test-executable run. No cloud run or installed node was stopped.
