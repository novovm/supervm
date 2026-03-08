# SVM2026 Unified Account Code Audit (2026-03-07)

## Scope

- Baseline docs read:
  - `docs_CN/UNIFIEDACCOUNT/*` in SUPERVM
- Audited code in SVM2026:
  - `src/vm-runtime/src/chain_linker/account.rs`
  - `src/vm-runtime/src/chain_linker/atomic_swap.rs`
  - `src/vm-runtime/src/chain_linker/cross_contract.rs`
  - `src/vm-runtime/src/chain_linker/cross_mining.rs`
  - `contracts/web30/sdk/src/web3005.ts`
  - related docs consistency checks

## Findings (By Severity)

### Critical

1. Atomic swap path lacks signature verification and nonce/replay enforcement.
   - Evidence:
     - `atomic_swap.rs:126-127` (`verify_signature` is TODO and not executed)
     - `atomic_swap.rs:255-256` (nonce check is TODO)
     - `atomic_swap.rs:269-270` writes success receipt even though signature/nonce path is incomplete
   - Impact:
     - Replay and unauthorized swap execution risk if this feature is enabled.

2. Cross-mining verification is effectively bypassed.
   - Evidence:
     - `cross_mining.rs:269-273` (`verify_mining_hash` returns `true` unconditionally)
     - `cross_mining.rs:157-158` task persistence is TODO (task lifecycle incomplete)
   - Impact:
     - Invalid submissions can pass verification logic; reward distribution trust boundary collapses when enabled.

### High

3. Cross-contract execution paths are stubs returning successful results.
   - Evidence:
     - `cross_contract.rs:214-231`, `235-252`, `256-268`
     - All three execution branches (`WASM/EVM/Solana`) return `success: true` with placeholder outputs.
   - Impact:
     - Callers can receive false-positive execution success semantics; state model and security model diverge from expected behavior.

4. Unified account binding uniqueness is not enforced globally.
   - Evidence:
     - `account.rs:143` only stores per-account `linked_accounts: HashMap<u64, Vec<u8>>`
     - `account.rs:200-205` only prevents duplicate `chain_id` inside one account, no global reverse index.
     - `account.rs:186-196` numeric alias assignment has no global collision check.
     - `account.rs:283-285` allocator `is_used()` is a stub (`false`).
   - Impact:
     - Violates `1 PersonaAddress -> 1 UCA` uniqueness requirement from SUPERVM unified-account spec.

5. Chain typing fallback may silently route unknown chain IDs to EVM.
   - Evidence:
     - `cross_contract.rs:377-382` defaults unknown `chain_id` to `ChainType::EVM`.
   - Impact:
     - Misclassification and unsafe execution assumptions for unsupported chains.

### Medium

6. Account ID length rule can be bypassed by direct enum construction.
   - Evidence:
     - Validation path enforces 20-byte public key in `account.rs:26-27`.
     - Tests directly construct 32-byte `PublicKey` without constructor in `query_aggregation.rs:479`.
   - Impact:
     - Inconsistent identity canonicalization and potential serialization/index mismatch.

7. Documentation and implementation status are inconsistent.
   - Evidence:
     - `contracts/web30/README.md:130` claims `WEB3005 Identity` complete.
     - `contracts/web30/IMPLEMENTATION.md:19-26` claims WEB3005 complete.
     - `contracts/web30/IMPLEMENTATION.md:194` marks identity as "规划中".
     - `contracts/web30/identity` directory is missing in repository.
     - `standards/WEB3005-IDENTITY-REPUTATION-STANDARD.md:563` references `src/vm-runtime/src/adapter/account.rs` (path missing; actual code in `chain_linker/account.rs`).
   - Impact:
     - Migration and integration decisions may be made on stale or contradictory evidence.

## Notes on Feature Gating

- `vm-runtime` feature flags indicate `atomic-swap`, `cross-contract`, and `cross-mining` are opt-in, not default.
- Risk remains significant if these features are enabled in builds/tests without additional hardening.

## Recommended Remediation Order

1. Block feature enablement for `atomic-swap`, `cross-contract`, `cross-mining` until signature/nonce/verification gaps are closed.
2. Implement global uniqueness index for `(persona_type, chain_id, external_address) -> uca_id` and explicit conflict rejection events.
3. Add nonce scope policy (`persona` at minimum), replay guard, and audited event emission in all account entry points.
4. Replace execution stubs with explicit `ERR_NOT_IMPLEMENTED` fail-closed behavior until real backends are integrated.
5. Reconcile all WEB3005 docs with actual code state and fix broken path references.
