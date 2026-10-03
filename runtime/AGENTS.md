# Runtime components pending product integration

- This is not a replacement for the whole SUPERVM product. Read root AGENTS.md
  and `../docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md` first. Preserve useful
  components for integration under novovm-node and the unified novovm-exec
  facade. Work on main; do not introduce another branch.
- This is a preserved nested Cargo workspace. Use the explicit
  `--manifest-path runtime/Cargo.toml` from the repository root for component
  tests; root Cargo builds the restored product. Keep both lock files.
- Consensus migration source is pinned at `2da3583c`. Reviewed wire/round/collector
  rules and their tests now live in `crates/novovm-consensus/src/round_bft`.
  Do not evolve a second protocol copy here or expose raw signing solely to
  cross the workspace boundary. The retained source and unreviewed drafts are
  historical migration material until the whole journal/publication integration
  and acceptance allow retirement. Its existing component tests do not activate
  round-BFT in the original node or grant finality to read-only decision checks.
- Do not depend on, include, or wrap code from `legacy/` in an active build.
  Migrate a reviewed local algorithm when needed, record its source and tests,
  and remove the old orchestration assumptions from the migrated boundary.
- Owned batch input and output must not carry a database handle, workspace
  guard, signer, or cached permission to publish. AOEM remains a generic engine.
- No whole-candidate control-loop blocking, per-key cross-thread RPC, or fixed
  legacy candidate-slot catalog may become the replacement architecture.
- Do not claim a module test proves AOEM execution, mainchain TPS, business
  validity proofs, finality, privacy, PQ integration or deployability.
