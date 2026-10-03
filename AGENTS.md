# Agent Highest Law

The active development project is the `SUPERVM` Git repository that contains
this file.

Definitions:

- `SUPERVM_ROOT` is the canonical repository root returned by
  `git rev-parse --show-toplevel` from this file's directory.
- `WORKSPACE_ROOT` is the parent directory of `SUPERVM_ROOT`.

Hard rules:

- Only `SUPERVM_ROOT` may be modified for NOVOVM / SUPERVM development tasks.
- The sibling repository `WORKSPACE_ROOT\MEV` is reference-only on this
  machine. Do not edit, commit, push, format, test-generate files, or apply
  patches there unless the user explicitly asks for MEV recovery work.
- Other sibling repositories under `WORKSPACE_ROOT` are reference-only unless
  explicitly named by the user as the active project.
- Before any code edit, resolve the Git repository root and verify that the
  target is inside `SUPERVM_ROOT`.
- If the prompt mentions SUPERVM/NOVOVM/AOEM runtime/NovoRUDP/full async
  pipeline, treat `SUPERVM_ROOT` as the only writable workspace.
- If a tool invocation starts in any other workspace, stop and switch to
  `SUPERVM_ROOT` before modifying files.
- Never require a particular drive letter or workspace parent-directory name.

Shared product and multi-device development contract:

- User clarification 2026-10-03 supersedes the overbroad 2026-10-02 isolation
  rule. Preserve/restore `crates/novovm-node` as the actual product controller
  and `novovm-exec` as the unified AOEM facade, together with existing product
  modules and supporting directories. Isolate faulty execution paths, not the
  entire original product. Follow
  `docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md` for scope and acceptance.
  The archive and uncommitted drafts remain protected: do not develop in,
  overwrite, or directly link unreviewed legacy sources. Restore reviewed code
  to the product layout and integrate useful runtime components; do not roll
  back all later fixes or create a second product chain. The current Cargo/CI
  still build runtime libraries only; documentation does not restore the node.

- Before starting or resuming NOVOVM work, read
  `docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md` first, then
  `docs/NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md`,
  `docs/NOVOVM_DELIVERY_ALIGNMENT.md`, and the latest section of
  `docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md`; verify the actual branch,
  working tree, local and remote commit, and current file claims rather than
  relying on an earlier conversation snapshot. The recovery roadmap is the
  current implementation priority; conflicting runtime-only rules and older
  next-step assignments are historical, not active authorization.
- High-performance finalized transactions, cryptographic private assets, and
  post-quantum security remain core product delivery objectives. A runnable
  chain, component benchmark, storage-ownership gate, or green CI does not
  replace these objectives. Do not reduce scope without the user's agreement.
- Keep AOEM domain-neutral. Host business-policy ownership does not justify
  treating serial Host computation plus AOEM persistence as completed parallel
  transaction execution. Follow the delivery contract's acceptance boundaries.
- Preserve the original algebraic and heterogeneous execution design without
  restoring old placeholders. Algebraic reordering requires explicit business
  effects and checked preconditions; failed transactions may still consume
  nonce and fees. BFT certificates and local execution readback are not business
  execution validity proofs. Component benchmarks do not sign off mainchain TPS.
- Work on `main`; do not create branches without explicit user authorization.
  Coordinate overlapping work through the delivery document's lightweight
  claim table. Preserve uncommitted work and other machines' commits; never
  force-push shared history. Push completed authorized changes promptly.
- Use repository-relative documentation links. State exactly which commit,
  product path, and test environment support a completion claim.

Recovery note:

- On 2026-06-24, three transport-hardening commits were mistakenly applied to
  the sibling `MEV` repository and then reverted on MEV main:
  - `935b660`
  - `efa71a2`
  - `77d9ec7`
- The revert commits on MEV are:
  - `bae3b57`
  - `ba61f0f`
  - `f5eea5b`
