# NOVONET authenticated relative-time integration — 2026-11-10

Source base: `3fc0e2ac`. This record qualifies the source in its containing commit,
not the installed `1ef31893` binaries. No node, OS clock, account, chain state,
cache, firewall, production deadline or transport preference was changed.

Design, call ownership and remaining gaps: shared handoff section 36,
`docs/NOVORUDP_SHARED_CLIENT_HANDOFF.md`.

Raw platform logs, including intermediate failures: [logs.zip](logs.zip).
Extract to this directory to use the individual filenames referenced below.

## Evidence scope

The new original-owner regression uses four local original NativeLifecycle /
NativeChannel / UDP owners, original AOEM stores and component signing fixtures.
Its injected clock offsets are +900 / -444 / +1200 / -900 ms, not OS clock changes.
It asserts that the old direct wall-clock comparison rejects the same object,
then requires both sender Object DONE and receiver integrity acceptance on the
new path. It also exercises jump invalidation and nonce refusal after remeasurement.
The final form issues at the sender's current time, without backdating the probe.
This is not A/B/C/D, public-network, ML-DSA deployment or business-finality evidence.

Shared tests cover three-sample consistency, either-direction asymmetry, signed
positive/negative offsets, stale references, nonce/context/session binding,
clock jumps, poll gaps, monotonic rollback, expiry, backwards remeasurement and
an immutable conservative wait window at the fresh-message boundary.

## Results

| Check | Windows | Linux |
| --- | --- | --- |
| Original lifecycle / archive recovery | 43 passed | 43 passed |
| Original structured codec | 35 passed | 35 passed |
| Final time and fresh-boundary guard | 21 passed | 21 passed |
| Final affected probe owner (included in 43) | 5 passed | 5 passed |
| Strict Clippy | passed | passed |
| Node / Client entry compilation | passed | passed |

The last one-line `issued <= upper` guard disallows retaining definitely-future
probes (including a new 60-second-future counterexample); only the established
uncertainty boundary may wait. `*-time-final` / `*-owner-final` and final Clippy
logs qualify that tightening. The final 43/35 groups were also rerun directly
using the exact final test executable built by the owner-final Cargo command
(same original AOEM environment, `--include-ignored --test-threads=1`). This
avoids rebuilding unchanged dependencies and is not a copied test implementation.
There are 99 distinct selected tests per platform; the five owner tests are a
subset, not five extra distinct tests. Raw outputs are `*-lifecycle-final.txt`
and `*-codec-final.txt`.

Final **test-executable** SHA256 (not deployable Node/Client qualification):

- Windows `novovm_node-2448a5ce12032170.exe`:
  `2024936dfee4db9ecfde702a7e3199cec4c2ffdcc36221cb9e85f67ad2d21596`.
- Linux `novovm_node-4b75a2f28f91797a`:
  `a762cf68f9dbf8d98870c5e66f707532e2b6cfa91f3e565fe5cce9139342dd13`.

No cloud Full was dispatched; no fixed release archive was generated.

## Reproduction

Both platforms use `--locked`, the original `iroh-transport` feature for node
tests, `AOEM_DLL_MANIFEST_REQUIRED=1`, `AOEM_SEMANTIC_GRAPH_V2_WORKERS=2`, and
explicit `NOVOVM_AOEM_TEST_LIBRARY` / `AOEM_DLL_MANIFEST` from the retained 1ef
AOEM package. Those AOEM files are test dependencies, not new application binaries.

| Input | SHA256 |
| --- | --- |
| Windows AOEM DLL | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` |
| Windows AOEM manifest | `07d61c1cab0ab57844d0cf18c5f32609161ed65b6a9fcb3c044929cd19bc24b1` |
| Linux AOEM SO | `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` |
| Linux AOEM manifest | `0ccb0a626c06de4eb92f04192062dad3e9df655816525c4a46ceb1930c6726e2` |

Commands (Windows shared-time run uses the existing debug target; Linux uses release):

```text
cargo test --locked -p novovm-network --lib network_time -- --test-threads=1
cargo test --release --locked -p novovm-node --lib --features iroh-transport native_lifecycle::probe::tests -- --include-ignored --test-threads=1 --nocapture
cargo test --release --locked -p novovm-node --lib --features iroh-transport native_lifecycle:: -- --include-ignored --test-threads=1
cargo test --release --locked -p novovm-node --lib --features iroh-transport consensus::transport:: -- --include-ignored --test-threads=1
cargo clippy --locked -p novovm-network -p novovm-node --lib --tests --features iroh-transport -- -D warnings
```

Linux Clippy uses `--release`; Linux shared-time tests also enable `iroh-transport`.
Windows build target is the existing C: build directory; Linux uses the existing
WSL release target. No installed process or database is used by these fixtures.

## Development failures retained as failures

- The first node test build exposed an exhaustive legacy-codec oracle match
  missing the new `NetworkTime` variant (E0004); the oracle now explicitly rejects
  the new tag as it does other tags outside its historical codec scope.
- Intermediate Windows Clippy / Linux test builds overlapped an edit to the
  shared API, seeing new node calls with the previously compiled dependency
  (`generation` / `validity_window` E0599). They failed, not passed. Final checks
  run against the frozen shared and node source together.
- Earlier 18/20-test and backdated-probe successes are intermediate evidence;
  they do not replace the final fresh-probe / 21-test qualification.
- `windows-owner.txt` and `linux-owner.txt` preserve a 4-pass/1-fail run of the
  final waiting implementation against the old concurrent test driver. That
  driver froze the node poll Instant at `start`, and stopped on sender DONE
  before waiting for receiver acceptance (assertion 0 versus 1). The test now
  supplies the progressing monotonic Instant and requires both endpoints'
  completion within the unchanged ten seconds. Neither assertion nor deadline
  was weakened. Final `*-lifecycle.txt` runs include the repaired test.

The old 444 ms real C rejection and other historical failures remain unchanged.
No source-only result closes that real incident before deployment and dual-ended
evidence. First native-handshake time bootstrap, authenticated discovery leases,
chat consumers, platform suspend notifications, independent UTC trust and real
C testing are not qualified by this work. Backwards peer-clock corrections may
remain refused until the conservative anti-revival floor is reached.
