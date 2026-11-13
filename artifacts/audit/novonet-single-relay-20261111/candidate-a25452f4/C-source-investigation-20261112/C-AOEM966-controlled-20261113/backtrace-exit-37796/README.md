# C backtrace run PID37796: native crash after the observation window

**FAILED / C NOT READY. No node fix or relay readiness is claimed.**

The same a254 executable and AOEM966 runtime opened C's original database with
`existing` at 08:41:01 Beijing on 2026-11-13. Its owner records node SHA256
`1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d` and AOEM SHA256
`5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`.

## Actual timeline

| Beijing time | Evidence |
| --- | --- |
| 09:41:52 | Observer's last liveness: original PID37796, task Running, UDP41052/42052, stderr0 |
| 09:42:07 | Bounded observer returned `WINDOW_ENDED`; it did not restart the node |
| 10:15:52 | Application Error1000, record546615, PID0x93a4, `c0000005`, VCRUNTIME140.dll+0x11268 |
| 10:15:56 | Original owner recorded exit `-1073741819`, `start=existing` |
| 11:11:51–11:11:52 | Read-only supplement: no node process and no C UDP listeners |

The crash happened about34 minutes **after** the observer ended. Its earlier
clean window is not a stability qualification. `RUST_BACKTRACE=full` was set,
but this native access violation emitted no Rust panic trace: stderr is empty.
The original minidump supplies the native stack instead.

## Call-chain comparison

The new dump matches a254 PDB GUID `E16D0B11-47CA-4EBF-B482-C91380CC57F8`, age1.
The original matching PDB has not arrived. C analyzed the dump using the original
a254 executable and its unwind information, without substituting875 symbols.

Compared with the earlier PID27928 dump, **13 of14 node return RVAs are identical**.
Frame6 has a different return RVA within the same runtime function; all14 frames
belong to the same functions. See [machine-readable comparison](comparison-public.json).
The prior normalized machine-code comparison with875 points to:

`native tick -> observe_tick -> RpcLifecycle::status -> profile::snapshot -> hash
formatting -> u8 LowerHex -> String append -> two-byte memory copy`.

That source interpretation is cross-binary comparison, not exact a254 line symbols.
This time the faulting instruction reads from `rdx=0x8000000000000005`, length2.
Windows records an all-ones exception address for that noncanonical operand.
PID27928 instead faulted writing through an invalid destination in the same copy
path. This narrows the repeated exposure site; it does **not** establish where
the invalid pointer was introduced, or prove a VCRUNTIME, AOEM, formatter or
hardware defect. Replacing AOEM did not eliminate the observed failure.

## Isolation and collection limits

C exercised the unchanged original profile source, except removal of the unused
`mod traffic` declaration, with populated2048-entry critical/delivery/control
buffers and two snapshot/JSON-roundtrip readers. In180.051seconds it completed
6,616 snapshots and81,248,256 checked hash strings, exit0, without reproducing the
crash. See [result](profile-isolation-public.json) and
[test driver](profile-isolation-main.rs). This fixture uses no AOEM, network,
node database or real identities. It does not acquit those components or qualify
the full node. Build dependencies were serde_json1.0.149 (preserve_order/raw_value),
indexmap2.14.2 and sha2 0.10.9; rustc1.94.0 commit4a4ef493. Run with
`NOVOVM_PROFILE_STAGES=1`, `NOVOVM_PROFILE_CRITICAL_PATH=1` and `RUST_BACKTRACE=full`.

The elevated collector copied the original logs and6,203,358-byte dump and checked
source/destination hashes, then exited1 while reading an empty XML `#text` property.
That failure is retained in [collector-error.json](collector-error.json); it is
not reported as a successful collection script. The raw dump remains local,
SHA256 `3acb7ec2450d6132334a1700764c3f50704f6cf4f98f2f9e08e6143c4c7c7ca0`.
No dump, raw memory, private key, packet payload or database was uploaded.

[Read-PublicExit.ps1](Read-PublicExit.ps1) fixes only that diagnostic XML parsing:
namespace-aware element enumeration and `InnerText` handle empty values. Its
namespaced-empty-field regression and one actual read-only run passed, exit0.
[The supplement](public-exit-supplement.json) independently captures the crash
event and current process/listener absence. It does not claim access to the
privileged task state or replace the earlier failed collector.

## Handoff and remaining work

- A: keep C unavailable for double relay; no object should be sent against an old READY.
- C retains diagnosis ownership. Locate the write/lifetime violation that first
  changes the formatter/String state, not merely the later copy failure. An exact
  a254 PDB would improve variable/line attribution; it is an artifact request only.
- The current minidump records the crash stack, not an execution history of memory
  writes. Any next instrumented run must capture the first violation with matching
  symbols; removing diagnostics or swallowing faults is not a causal repair.
- No further node recovery, observer launch, object send, package update, database
  change or identity change was performed in this round. D was not operated.
- Node crash root cause and repair remain **open**. Double relay remains **blocked**.
