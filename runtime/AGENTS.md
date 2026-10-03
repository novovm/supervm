# Runtime components pending product integration

- This is not a replacement for the whole SUPERVM product. Read root AGENTS.md
  and `../docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md` first. Preserve useful
  components for integration under novovm-node and the unified novovm-exec
  facade. Work on main; do not introduce another branch.
- This is a preserved nested Cargo workspace. Use the explicit
  `--manifest-path runtime/Cargo.toml` from the repository root for component
  tests; root Cargo builds the restored product. Keep both lock files.
- Do not depend on, include, or wrap code from `legacy/` in an active build.
  Migrate a reviewed local algorithm when needed, record its source and tests,
  and remove the old orchestration assumptions from the migrated boundary.
- Owned batch input and output must not carry a database handle, workspace
  guard, signer, or cached permission to publish. AOEM remains a generic engine.
- No whole-candidate control-loop blocking, per-key cross-thread RPC, or fixed
  legacy candidate-slot catalog may become the replacement architecture.
- Do not claim a module test proves AOEM execution, mainchain TPS, business
  validity proofs, finality, privacy, PQ integration or deployability.
