# NOV signature/nonce relation proof v1 (diagnostic only)

This opt-in workspace builds an actual RISC0 guest. It is intentionally separate
from the product workspace: ordinary node builds do not install a zkVM toolchain.
It calls AOEM through the existing `AoemExecFacade`, not a new crypto backend in
the node or NOV business logic inside AOEM.

## Exact statement

The guest verifies Ed25519 over the existing shared TxIR signing digest, checks
the existing 20/32-byte sender binding, matches a nonzero public chain claim,
and checks/advances a public nonce claim using shared protocol rules. Its journal
binds a versioned domain, message digest, public key, chain-scoped nonce identity,
chain, nonce and next nonce. Guest input is length-prefixed postcard (64 KiB max);
the journal is raw postcard bytes. Trailing bytes within the postcard payload
are rejected. This diagnostic's size cap is not a new network transaction limit.

`verify_strict` intentionally rejects weak-key/noncanonical signatures. This is
a narrower relation than the legacy Adapter CPU `verify` fallback, not a silent
change to production signature acceptance. Ordinary signatures remain compatible;
full AOEM/Adapter adversarial signature equivalence is NOT established here.

**This is not complete transaction validity.** In particular:

- Expected nonce is a public claim, not an authenticated parent-state read.
- Chain and expected journal must be selected independently by the verifier.
- Account delegation, fee/nonce owner authority, execution policies, transaction
  hash canonicality, business execution, balances and state/receipt roots are not
  proven. The test deliberately uses a fixed synthetic transaction hash.
- No `proof_sealed`, `chain_canonical`, `safe` or `finalized` flags are changed.
- No node service, public RPC, FULLMAX package or Windows DLL is replaced.

## Reproduce (Linux / WSL, from repository root)

Prerequisites: pinned repository Rust toolchain, installed RISC0 Rust toolchain,
and a trusted AOEM library with the real portable RISC0 backend enabled. RISC0
crates are pinned at 1.2.6 to match the tested AOEM ABI backend. Both workspace and
guest lockfiles are committed. Paths are supplied by the operator, not compiled
into the source. The library may be a directly loaded portable-receipt sidecar.

```sh
export RUSTFLAGS="--diagnostic-width=120"
cargo build --manifest-path proofs/native-auth/Cargo.toml \
  -p novovm-auth-proof-probe --locked
cargo run --manifest-path proofs/native-auth/Cargo.toml \
  -p novovm-auth-proof-probe --locked -- \
  run <trusted-AOEM-library> <new-output-directory>
```

The output parent must exist; the directory and receipt must not exist. `run`
first rejects three invalid guest witnesses (signature, chain, nonce), generates
a real receipt with development mode removed, then starts a fresh verifier after
the producer exits. Verification pins the locally compiled guest ID and frozen
public statement, never an ID or journal supplied by the producer. The verifier
does not call the signing fixture or receive a signature/private witness. It also
rejects wrong statement, wrong image, mutated receipt and truncated receipt.
The same diagnostic binary contains producer code; this is process isolation,
not a claim of a separately packaged verifier-only executable.

```sh
# Host/core tests and lint; skip only the nested guest compilation for these.
RISC0_SKIP_BUILD=1 cargo test --manifest-path proofs/native-auth/Cargo.toml \
  --workspace --locked
RISC0_SKIP_BUILD=1 cargo clippy --manifest-path proofs/native-auth/Cargo.toml \
  --workspace --all-targets --locked -- -D warnings
```

Skipping guest compilation is not proof verification. A probe built with an empty
guest refuses to run; rebuild without `RISC0_SKIP_BUILD` before proving/verifying.
The nested guest is separately compiled and exercised by the real roundtrip.
This separation avoids the host Clippy wrapper leaking into the RISC-V build.

Reference: RISC0's [guest I/O](https://docs.rs/risc0-zkvm/1.2.6/risc0_zkvm/guest/env/index.html)
and [guest build integration](https://docs.rs/risc0-build/1.2.6/risc0_build/).

Next: authenticate the nonce/state inputs against the selected parent, and reuse
the actual production business transitions in the guest. Do not reinterpret this
limited proof as an execution or mainnet finality proof.
