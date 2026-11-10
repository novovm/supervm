# Native first-handshake session clock — 2026-11-10

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
