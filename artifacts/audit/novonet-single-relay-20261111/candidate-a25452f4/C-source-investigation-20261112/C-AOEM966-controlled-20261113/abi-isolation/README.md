# C local ABI-preservation isolation, 2026-11-13

Diagnosis only; no node binary, database, identity or network configuration changed.
The live backtrace process remains diagnostic, not relay-ready. No root-cause fix is claimed.

The standalone Windows x86_64 assembly wrapper calls the actual AOEM exported keygen,
sign and verify functions. It checks eight nonvolatile GPRs and XMM6–XMM15 against
sentinels, then restores its caller's registers. A deliberate RBX/XMM6 clobber control
must report exactly those two bits; a clean control must return its original result.
This is a local diagnostic harness, not a replacement production ABI or crypto implementation.
It does not qualify exception unwinding, all AOEM APIs or the full node workload.

- Runtime source: AOEM `9662470daa39b01f832705654e330830b8cd1bfa`.
- Actual DLL SHA256: `5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`.
- Harness executable SHA256: `f0919416726cc3e4c9349c3728a7e33fe19ba832547127e141fc250c770e54ee`.
- Run 1 exit 101: harness requested nonexistent `aoem_mldsa_sign`; it did not exercise crypto.
  Both original outputs are retained. The harness was corrected to the existing
  `aoem_mldsa_sign_v1` export; this was not an AOEM defect or product change.
- Run 2 exit 0: 10,000 sign/valid-verify/changed-message-reject rounds plus keygen,
  30,001 direct FFI calls, no sentinel mismatch. Generated test secret was zeroized
  and freed; no key bytes were printed or saved.
- Separately, the existing profile/FFI isolation ran 30 times with exit 0 each.
  `profile-runs.jsonl` retains the per-run exit codes/timestamps; this does not establish
  long-running node stability.

Reproduce offline with `python build_probe.py`, `cargo build --release --locked`, then
set `C_DIAGNOSTIC_AOEM_DLL` to the verified fixed DLL and run the harness. Never point
this fixture at a node database or use original identity material. No deployment follows
automatically from this result.

The node liveness snapshot records the same PID37796 running during this test, both
original listeners, stderr length 0. The earlier real IndexMap panic remains unresolved.
