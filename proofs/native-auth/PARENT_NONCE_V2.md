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

V2 proof generation, independent V2 receipt verification, and a product witness
export/verification entrypoint are **NOT EXECUTED / NOT INTEGRATED**. The existing
host probe still runs V1, not V2. V1's previous real receipt is not evidence for
this new guest. Always pin the image from the matching trusted build; a rebuild
is not a promise of the same image ID. Never obtain pins from an untrusted proof.

The next step is to export the node's exact selected-parent projection and test
V2 generation/verification against independently pinned parent root and journal.
Business execution, resulting balance/state roots, receipt roots, delegated
authority, finality and public/multi-device acceptance remain outside this slice.
