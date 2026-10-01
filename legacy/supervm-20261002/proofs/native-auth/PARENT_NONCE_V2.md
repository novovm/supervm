# Parent-state-bound nonce relation v2

This slice removes the freely supplied `expected_nonce` from the new V2 input.
`ParentAuthInput` contains the TxIR, expected chain, expected parent root and the
complete parent state wire. It authenticates the wire against the root before
looking up the signer nonce; it then runs the existing signature/nonce relation.
The journal binds the parent root and the authentication result under the new
`novovm-parent-state-signature-nonce-relation/v2` domain.

The parent root MUST be independently selected by the verifier for the expected
chain (for example from an already validated parent block). A root supplied by
the prover is not a trust anchor. This code does not validate parent finality,
fork choice, block headers, QC, or the complete parent state schema.

## Existing state commitment, not a new tree

The node commits the complete canonical state projection, using the existing
`novovm-native-aoem-semantic-ledger-state-digest-v3` domain followed by NUL.
The shared `native_state_wire_root_v3` preserves those exact bytes. Existing
frozen V3 roots pass unchanged; no database migration or new Merkle root exists.

The bounded reader consumes the node's tagged canonical binary representation
(not JSON text). It checks the projection schema and V2 identity scheme, then
reads `module_state_shards.native_execution.native_auth_next_nonces` using the
existing chain-scoped signer identity digest. A missing signer in a valid table
means zero, exactly as in the node; a missing/invalid table or legacy identity
scheme is rejected, never silently treated as genesis. Object keys must be
strictly ordered and unique. Trailing/truncated wire and excessive nesting are
rejected. Nonce values must be canonical unsigned 64-bit integers.

The witness includes the whole projection: unrelated balances and other shards
remain root-bound even though the relation does not execute or interpret them.
Proof-side limits are 1 MiB state wire, 65,536 values and depth 64, plus 64 KiB
input overhead. These bounds deliberately restrict this prototype; they do NOT
change the node's production state-size limits. Larger states need a later
explicit design, not truncation or omission of committed state.

## Code and evidence

- Protocol: `crates/novovm-protocol/src/native_parent_nonce.rs`.
- Host integration tests: `crates/novovm-node/src/native_parent_nonce_tests.rs`.
- Relation: `core/src/parent.rs`.
- Separate guest: `methods/guest/src/bin/novovm-parent-nonce-guest.rs`.

2026-09-27 local validation:

- Three node tests use actual production projection/encoding, existing frozen
  roots, nonzero signer nonce, other-chain/signer lookup, altered balances,
  malformed tables, wrong roots, and malformed/bounded binary input.
- Ten existing node identity/replay/restart regressions pass.
- Eight proof-workspace host/core tests pass (including two new parent tests).
  Parent core fixtures are synthetic; they are not chain-finalized snapshots.
- The V2 RISC-V guest builds with the installed RISC0 toolchain, without skip.

V2 has a separate `parent_nonce` diagnostic entrypoint; the original default
host probe still runs V1. V1's previous real receipt is not evidence for
this new guest. Always pin the image from the matching trusted build; a rebuild
is not a promise of the same image ID. Never obtain pins from an untrusted proof.

## Node export and opt-in V2 roundtrip

`export_native_parent_state_wire_v3(state, expected_parent_root)` is a read-only
node API. It uses the production projection/encoder, rejects a root mismatch or
oversize witness, and does not choose a parent or access a running database.

The ignored `export_parent_proof_fixture_v2` test exports a constructed node
state and a transaction created by the node's normal signing/TxIR conversion
functions. This is actual node encoding, but NOT a captured finalized/live
parent. It sets chain 1 and signer nonce 19. The destination must be new, with
an existing parent inside the current repository.

From repository root in PowerShell:

```powershell
$env:NOVOVM_PARENT_PROOF_EXPORT_DIR = Join-Path (Get-Location) 'artifacts/audit/parent-proof-new'
cargo test -p novovm-node --lib export_parent_proof_fixture_v2 --locked -- --ignored --nocapture --test-threads=1
```

From the same repository in Linux/WSL (use the same exported directory):

```sh
export RUSTFLAGS="--diagnostic-width=120"
cargo build --manifest-path proofs/native-auth/Cargo.toml \
  -p novovm-auth-proof-probe --bin parent_nonce --locked
target/debug/parent_nonce prepare artifacts/audit/parent-proof-new 1
target/debug/parent_nonce run <trusted-AOEM-library> artifacts/audit/parent-proof-new
```

`prepare` is a TRUSTED controller step: it reads the local node export and
creates `witness.postcard` and `expected.journal` without overwriting anything.
Do not run it on a prover-supplied snapshot and call that trusted parent selection.
`run` passes only the witness to the producer, waits for producer exit, then
passes only the pre-existing expected journal and receipt to the verifier.
Both processes pin the guest image from their trusted build. Verification runs
with development mode enabled to check that receipt acceptance is not relaxed.
No snapshot, transaction signature, or private witness is read by the verifier.

To verify a completed receipt again without regenerating it:

```sh
RISC0_DEV_MODE=1 target/debug/parent_nonce verify <trusted-AOEM-library> \
  <trusted-expected.journal> <receipt.bin>
```

2026-09-27 export/probe validation: the opt-in export passed, as did three normal
parent tests, eight proof-workspace host/core tests, strict node/proof host
Clippy and the real guest build. The node-exported fixture root was
`5054c01fdfedea751cfb56dc4b6297f24ea00ca210090af03794bff8bd999e62`.
The real V2 guest rejected an altered root, altered snapshot and damaged
signature. The real V2 run completed successfully; its result was collected and
independently reverified on 2026-09-28. The original controller exited with code 0
after the producer exited and the fresh verifier accepted the receipt.

## V2 fixture proof acceptance (2026-09-28)

- Receipt: `artifacts/audit/parent-proof-v2-20260927/receipt.bin`, 1,643,504 bytes.
- Receipt SHA256: `34ef47928cdac12a83650a2d45102fc8ed36a4b22c4e94d7aa441c6405c309eb`.
- Pre-pinned expected journal SHA256:
  `0831aba12d3a30dc781c002d157ba8562ec260712f6789ddbe71984c038424ab`.
- Trusted compiled image words: `[4271428485, 664905082, 1097516772, 1629651488,
  1454212260, 2451898230, 2455128016, 3299132115]`.
- The final host verifier was rebuilt, then accepted this same receipt with
  `RISC0_DEV_MODE=1`. No private transaction or parent snapshot was supplied to it.
- Negative checks cover both domains, parent root, nonce identity, nonce and
  successor, chain, message and public key; appended/empty journal, wrong image,
  mutated receipt and truncated receipt are also rejected.
- Nine proof-workspace host/core tests and strict Clippy pass. The guest was
  built without skip mode for real verification. No new proof generation was
  needed for these host-only negative-test changes.
- Both normal CI and host-only proof CI passed for baseline `2703ed5`.
  CI does not generate this proof; local proof verification is separate evidence.

The generated binary receipt is local and not committed. The recorded hashes
identify the tested artifacts, not additional trust anchors or consensus proofs.
No AOEM source, release DLL, node state, or finality flag changed in this acceptance.

The next functional stage is to reuse actual production business state transitions
inside the proof, binding their output state/receipt roots, while integrating
independently selected real parents instead of the test fixture.
Business execution, resulting balance/state roots, receipt roots, delegated
authority, finality and public/multi-device acceptance remain outside this slice.
