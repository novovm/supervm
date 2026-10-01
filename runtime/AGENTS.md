# Active replacement implementation

- This is the only active runtime source area. Read the root AGENTS.md and
  current rebuild plan first. Work on main; do not introduce another branch.
- Do not depend on, include, or wrap code from `legacy/` in an active build.
  Migrate a reviewed local algorithm when needed, record its source and tests,
  and remove the old orchestration assumptions from the migrated boundary.
- Owned batch input and output must not carry a database handle, workspace
  guard, signer, or cached permission to publish. AOEM remains a generic engine.
- No whole-candidate control-loop blocking, per-key cross-thread RPC, or fixed
  legacy candidate-slot catalog may become the replacement architecture.
- Do not claim a module test proves AOEM execution, mainchain TPS, business
  validity proofs, finality, privacy, PQ integration or deployability.
